# Phase 1100: パイプライン統合（復号→符号化）

Status: IN_PROGRESS

## Context

Phase 100〜1000で個別のビルディングブロック（MQC, T1, DWT, MCT, TagTree, PI等）は全て実装・テスト済み。J2K/JP2のマーカー読み書きも完成している。

しかし、これらを繋ぐ**パイプライン統合**が未実装であり、現在の `decode()` はメタデータのみ返し（ピクセルデータなし）、`encode()` は空のタイルデータを書き込むスタブ状態。

本Phaseでは、既存のビルディングブロックを接続し、実際にJPEG 2000ファイルの符号化・復号が動作する状態にする。

### 現状の問題

```text
decode()の現状:
  J2Kヘッダ解析 ✅ → タイルデータ読み込み ✅ → T2デパケット化 ❌ → T1復号 ❌ → 逆DWT ❌ → 逆MCT ❌
  → 結果: Imageにメタデータのみ、ピクセルデータなし

encode()の現状:
  ヘッダ書き込み ✅ → write_tile(dummy 4bytes) ❌ → EOC ✅
  → 結果: 構造的に正しいが画像データなしのJ2Kファイル
```

### ゴール

```text
decode(): J2K/JP2ファイル → 完全なピクセルデータを持つImage
encode(): Image → 有効なJ2K/JP2ファイル（C版で復号可能）
```

## C版対応コード

| C版関数                             | 概要                                                | Rustの対応先           |
| ----------------------------------- | --------------------------------------------------- | ---------------------- |
| `opj_t2_decode_packets()`           | パケットヘッダ解析 + コードブロック分割データ抽出   | `src/tier2/t2.rs`      |
| `opj_t2_decode_packet()`            | 単一パケットのデコード                              | `src/tier2/t2.rs`      |
| `opj_t2_encode_packets()`           | パケットヘッダ構築 + コードブロックデータパケット化 | `src/tier2/t2.rs`      |
| `opj_t2_encode_packet()`            | 単一パケットのエンコード                            | `src/tier2/t2.rs`      |
| `opj_tcd_decode_tile()`             | T2→T1→逆DWT→逆MCT→DCシフト                          | `src/tcd.rs`           |
| `opj_tcd_encode_tile()`             | DCシフト→MCT→DWT→T1→T2                              | `src/tcd.rs`           |
| `opj_tcd_rateallocate()`            | レート歪み最適化によるレイヤー割り当て              | `src/tcd.rs`           |
| `opj_tcd_makelayer()`               | 品質レイヤー構築                                    | `src/tcd.rs`           |
| `opj_j2k_decode()`                  | ヘッダ解析→タイル復号→画像組み立て                  | `src/j2k/read.rs`      |
| `opj_dwt_calc_explicit_stepsizes()` | 量子化ステップサイズ計算                            | `src/transform/dwt.rs` |

## 依存グラフ

```text
1100a (T2 decode) → 1100b (TCD decode) → 1100c (J2K/API decode統合)
                                              ↓
1100d (T2 encode) → 1100e (TCD encode + rate alloc) → 1100f (J2K/API encode統合)
                                              ↓
                                         1100g (ラウンドトリップ検証)
```

復号を先行し、符号化はその後。復号が動作すればC版で作成した.j2kファイルで検証可能。

## サブPR構成

| PR    | ブランチ                   | スコープ                         | 推定LOC | 依存         |
| ----- | -------------------------- | -------------------------------- | ------- | ------------ |
| 1100a | `feat/t2-decode`           | T2パケットデコード               | ~500    | なし         |
| 1100b | `feat/tcd-decode`          | TCDタイル復号パイプライン        | ~400    | 1100a        |
| 1100c | `feat/j2k-decode-pipeline` | J2Kデコーダ統合 + API            | ~300    | 1100b        |
| 1100d | `feat/t2-encode`           | T2パケットエンコード             | ~400    | 1100a        |
| 1100e | `feat/tcd-encode`          | TCDタイル符号化 + レート割り当て | ~500    | 1100d        |
| 1100f | `feat/j2k-encode-pipeline` | J2Kエンコーダ統合 + API          | ~300    | 1100e        |
| 1100g | `feat/roundtrip-tests`     | ラウンドトリップ検証テスト       | ~200    | 1100c, 1100f |

マージ順: 1100a → 1100b → 1100c → 1100d → 1100e → 1100f → 1100g

## 設計判断

### 復号先行

復号を先に実装する理由:

1. C版の `opj_compress` で作成した .j2k ファイルで検証可能
2. 符号化は復号のインバース — 復号が正しければ符号化の設計が明確になる
3. 復号は符号化より単純（レート割り当てが不要）

### T2パケットデコード (1100a)

パケットは以下の構造:

```text
[packet header] [packet body]
  ├─ inclusion (tag tree)
  ├─ zero bit-plane (tag tree)
  ├─ number of passes
  ├─ pass lengths
  └─ [codeblock data segments...]
```

C版の `opj_t2_decode_packet()` に対応。既存の `Bio`, `TagTree`, `t2_getnumpasses()`, `t2_getcommacode()` を使用。

```rust
/// T2パケットデコード: パケットヘッダを解析し、コードブロックのセグメントデータを抽出
pub fn t2_decode_packet(
    tile: &mut TcdTile,
    pi: &PacketIterator,  // 現在のパケット位置（レイヤー/解像度/成分/プリシンクト）
    data: &[u8],          // パケットデータ
    data_offset: &mut usize,
) -> Result<()>
```

### TCDタイル復号パイプライン (1100b)

```rust
impl Tcd {
    pub fn decode_tile(&mut self, tile_index: usize, tile_data: &[u8]) -> Result<()> {
        // 1. T2 デパケット化: パケットデータ → コードブロックセグメント
        t2_decode_packets(tile, pi, tile_data)?;

        // 2. T1 復号: MQ算術復号 → 係数復元
        t1_decode_cblks(tile)?;

        // 3. 逆DWT: ウェーブレット逆変換（各成分）
        for comp in &mut tile.comps {
            dwt_decode_2d_53(comp.data, ...);  // or dwt_decode_2d_97
        }

        // 4. 逆MCT: 色空間逆変換（YCbCr → RGB等）
        if tile.mct {
            mct_decode(c0, c1, c2);  // or mct_decode_real
        }

        // 5. DCレベルシフト
        self.dc_level_shift_decode();

        Ok(())
    }
}
```

### 量子化ステップサイズ計算 (1100b)

`opj_dwt_calc_explicit_stepsizes()` を実装。TCDの初期化時に量子化パラメータからステップサイズを計算する。5-3可逆DWTでは量子化なし（ステップサイズ=1）、9-7非可逆DWTではサブバンド依存のステップサイズが必要。

### レート割り当て (1100e)

C版の `opj_tcd_rateallocate()` に対応。PCRD（Post Compression Rate Distortion）最適化:

```text
各コードブロックの各パスについて:
  歪み削減量 ΔD と符号化コスト ΔR を計算
  → 全パスをΔD/ΔR（傾き）でソート
  → 目標ビットレートに達するまで傾きの大きいパスから選択
  → 選択されたパスまでをレイヤーに含める
```

初期実装では固定品質（全パス含める）を先に実装し、レート制御は後続で追加。

### テスト戦略

| PR    | テスト手法                                                                 |
| ----- | -------------------------------------------------------------------------- |
| 1100a | 手作りパケットバイト列でのユニットテスト                                   |
| 1100b | 手作りタイルデータでの統合テスト                                           |
| 1100c | C版 `opj_compress` で作成した .j2k ファイルでの復号テスト                  |
| 1100d | エンコード→デコードの出力バイト列検証                                      |
| 1100e | 品質レイヤーの正しい構築検証                                               |
| 1100f | C版 `opj_decompress` で復号可能な .j2k 出力検証                            |
| 1100g | encode→decode ラウンドトリップでピクセル完全一致（5-3）/ 誤差範囲内（9-7） |

### スコープ外（延期）

| 項目                                                 | 理由                     |
| ---------------------------------------------------- | ------------------------ |
| 領域指定デコード (`set_decode_area`)                 | 基本デコードを先行       |
| 成分指定デコード (`set_decoded_components`)          | 同上                     |
| 解像度削減デコード (`set_decoded_resolution_factor`) | 同上                     |
| ストリームコールバックAPI                            | MemoryStreamで十分       |
| `j2k_dump` / codestream info                         | デバッグ用、優先度低     |
| CIOリトルエンディアン                                | JPEG 2000標準はBE        |
| T1 64x64 SIMD最適化                                  | 機能完成後の最適化       |
| COC/QCC/POC/RGN書き込み                              | 単一成分・単一品質で十分 |

## 検証

各PR完了時:

```bash
cargo test
cargo test --features parallel
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --features parallel -- -D warnings
cargo fmt -- --check
```

1100c完了時（復号統合）:

- C版で作成した .j2k ファイルの復号テスト
- ピクセルデータが正しく復元されることを検証

1100g完了時（ラウンドトリップ）:

- 5-3可逆: encode→decode でピクセル完全一致
- 9-7非可逆: encode→decode でPSNR閾値以上

## リファレンスファイル

- `reference/openjpeg/src/lib/openjp2/t2.c` — T2パケット符号化/復号
- `reference/openjpeg/src/lib/openjp2/tcd.c` — タイル処理パイプライン
- `reference/openjpeg/src/lib/openjp2/j2k.c` — J2Kデコード/エンコードメインループ
- `reference/openjpeg/src/lib/openjp2/dwt.c` — `opj_dwt_calc_explicit_stepsizes()`
