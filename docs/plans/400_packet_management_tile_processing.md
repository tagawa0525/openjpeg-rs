# Phase 400: パケット管理 + タイル処理（PI・T2・TCD）

Status: PLANNED

## Context

Phase 300（MCT・DWT・T1）完了後の次フェーズ。JPEG 2000パイプラインの統合レイヤーを実装する。

- **PI（パケットイテレータ）**: 5種のプログレッション順序（LRCP, RLCP, RPCL, PCRL, CPRL）に従いパケットを列挙する。T2が使用。
- **T2（Tier-2符号化）**: パケット単位でコードブロックデータをビットストリームに符号化/復号する。Tag Tree（Phase 200）とBIO（Phase 100）を使用。
- **TCD（タイルコーダ/デコーダ）**: タイル単位でDC Level Shift → MCT → DWT → T1 → T2パイプラインを統合する最上位モジュール。

Phase 400が完了すると、タイル単位の符号化/復号が可能になり、Phase 500（J2Kコードストリーム）でマーカー付きの完全なストリームを構成できる。

### 前提: J2K構造体の最小定義

PI・T2・TCDはJ2Kの符号化パラメータ構造体（`CodingParameters`, `TileCodingParameters`, `TileCompCodingParameters`等）に依存する。Phase 400では、これらの構造体の **最小限のデータ定義** を `src/j2k/params.rs` に配置する。マーカー読み書きロジックはPhase 500で実装する。

## C版対応ファイル

| C版ファイル     | Rustモジュール  | 概要                                                   | C版LOC |
| --------------- | --------------- | ------------------------------------------------------ | ------ |
| `pi.c/h`        | `tier2/pi.rs`   | パケットイテレータ（5種のプログレッション順序）        | ~2,150 |
| `t2.c/h`        | `tier2/t2.rs`   | Tier-2パケット符号化/復号                              | ~1,700 |
| `tcd.c/h`       | `tcd.rs`        | タイルコーダ/デコーダ（パイプライン統合）              | ~2,930 |
| `j2k.h`（一部） | `j2k/params.rs` | 符号化パラメータ構造体（CP, TCP, TCCP, POC, Stepsize） | ~300   |

C版合計: ~7,080 LOC → Rust推定: ~3,500-4,500 LOC

## サブPR構成

| PR   | ブランチ                   | スコープ                                            | 推定LOC | 依存      |
| ---- | -------------------------- | --------------------------------------------------- | ------- | --------- |
| 400a | `feat/tcd-data-structures` | TCD階層データ構造 + J2Kパラメータ構造体 + types追加 | ~800    | Phase 300 |
| 400b | `feat/tier2-pi`            | パケットイテレータ（5種のプログレッション順序）     | ~800    | 400a      |
| 400c | `feat/tier2-t2`            | Tier-2パケット符号化/復号                           | ~1,000  | 400a,400b |
| 400d | `feat/tcd-pipeline`        | TCDパイプライン統合（init_tile, encode, decode）    | ~1,200  | 400a-400c |

マージ順: 400a → 400b → 400c → 400d

## 設計判断

### TCD階層の所有権モデル

C版はポインタの多段階間接参照を使い各レベルで手動メモリ管理する。Rustでは `Vec` による所有権ベースの階層を構成する:

```text
TcdTile
  └─ comps: Vec<TcdTileComp>
       └─ resolutions: Vec<TcdResolution>
            └─ bands: Vec<TcdBand>
                 └─ precincts: Vec<TcdPrecinct>
                      ├─ incltree: TagTree
                      ├─ imsbtree: TagTree
                      └─ cblks: TcdCodeBlocks (enum)
```

C版の `union { enc; dec; blocks; }` はRustの `enum` で安全に表現する。

### PIの「continue from current position」パターン

C版のPI `next_*` 関数は `goto LABEL_SKIP` で「前回の位置から再開」を実現する。Rustでは `first` フラグと内部状態（compno, resno, precno, layno）の保持で同等の動作を再現する。

### J2Kパラメータ構造体の段階的定義

Phase 400ではPI・T2・TCDが参照する最小限のフィールドのみ定義する。J2Kのマーカー読み書きに必要なフィールドはPhase 500で追加する。

### 符号化/復号のコードブロック分離

C版の `union { enc; dec; }` はRustの `enum TcdCodeBlocks` で安全に分離する。

### スコープ外（延期）

| 項目                                        | 延期先    | 理由               |
| ------------------------------------------- | --------- | ------------------ |
| ウィンドウ復号（`dwt_decode_partial_tile`） | Phase 500 | J2K統合時に必要    |
| マルチスレッドT1（`thread_pool`）           | Phase 900 | 最適化フェーズ     |
| PPM/PPTパケットヘッダストア                 | Phase 500 | J2Kマーカー依存    |
| Codestream info/index構造体                 | Phase 500 | J2K統合時に必要    |
| `opj_tcd_marker_info`（PLTマーカー）        | Phase 500 | エンコーダマーカー |
| Cinema/IMFプロファイル特殊処理              | Phase 600 | プロファイル対応   |
| HTJ2K（HT codeblock style）                 | Phase 700 | 別仕様             |

## モジュール詳細

### j2k/params.rs（新規）

J2Kの符号化パラメータ構造体。PI・T2・TCDが参照する最小限の定義。

| C版構造体         | Rust                        | 備考                       |
| ----------------- | --------------------------- | -------------------------- |
| `opj_stepsize_t`  | `Stepsize`                  | 量子化ステップサイズ       |
| `opj_tccp_t`      | `TileCompCodingParameters`  | タイル成分符号化パラメータ |
| `opj_poc_t`       | `Poc`                       | プログレッション順序変更   |
| `opj_tcp_t`       | `TileCodingParameters`      | タイル符号化パラメータ     |
| `opj_cp_t`        | `CodingParameters`          | 符号化パラメータ全体       |
| `J2K_T2_MODE`     | `T2Mode`                    | T2モード                   |
| `J2K_QUALITY_...` | `QualityLayerAllocStrategy` | レート配分戦略             |

### TCD階層データ構造（tcd.rs）

| C版構造体                  | Rust              | 概要                                  |
| -------------------------- | ----------------- | ------------------------------------- |
| `opj_tcd_layer_t`          | `TcdLayer`        | レイヤー情報（numpasses, len, disto） |
| `opj_tcd_cblk_enc_t`       | `TcdCblkEnc`      | 符号化コードブロック                  |
| `opj_tcd_seg_t`            | `TcdSeg`          | 復号セグメント                        |
| `opj_tcd_seg_data_chunk_t` | `TcdSegDataChunk` | セグメントデータチャンク              |
| `opj_tcd_cblk_dec_t`       | `TcdCblkDec`      | 復号コードブロック                    |
| `opj_tcd_precinct_t`       | `TcdPrecinct`     | プリシンクト（Tag Tree含む）          |
| `opj_tcd_band_t`           | `TcdBand`         | サブバンド                            |
| `opj_tcd_resolution_t`     | `TcdResolution`   | 解像度レベル                          |
| `opj_tcd_tilecomp_t`       | `TcdTileComp`     | タイル成分                            |
| `opj_tcd_tile_t`           | `TcdTile`         | タイル                                |
| `opj_tcd_t`                | `Tcd`             | タイルコーダ/デコーダ本体             |

### tier2/pi.rs

パケットイテレータ。5種のプログレッション順序に従いパケットを列挙する。

| C版構造体             | Rust           | 備考                                |
| --------------------- | -------------- | ----------------------------------- |
| `opj_pi_resolution_t` | `PiResolution` | pdx, pdy, pw, ph                    |
| `opj_pi_comp_t`       | `PiComp`       | dx, dy, numresolutions, resolutions |
| `opj_pi_iterator_t`   | `PiIterator`   | パケット列挙の状態保持              |

### tier2/t2.rs

Tier-2パケット符号化/復号。BIOとTag Treeを使用してパケット形式のビットストリームを処理する。

### tcd.rs パイプライン関数

#### 符号化パイプライン

```text
1. dc_level_shift_encode   ← DCオフセット除去
2. mct_encode              ← 色空間変換（RGB → YCbCr）
3. dwt_encode              ← ウェーブレット変換
4. t1_encode               ← コードブロック係数のMQ算術符号化
5. rate_allocate_encode    ← レート配分（二分探索）
6. t2_encode               ← パケット化
```

#### 復号パイプライン

```text
1. t2_decode               ← デパケット化
2. t1_decode               ← MQ算術復号
3. dwt_decode              ← 逆ウェーブレット変換
4. mct_decode              ← 逆色空間変換（YCbCr → RGB）
5. dc_level_shift_decode   ← DCオフセット復元
```

## 依存関係

```text
j2k/params.rs (符号化パラメータ構造体)
    |
    +--- types.rs (定数追加)
    |
    +--- tcd.rs (TCD階層データ構造)
    |         |
    |         +--- coding/tgt.rs (TagTree)
    |         +--- coding/t1.rs (TcdPass)
    |
    +--- tier2/pi.rs (パケットイテレータ)
    |         |
    |         +--- j2k/params.rs
    |         +--- image.rs (Image)
    |         +--- types.rs (ProgressionOrder)
    |
    +--- tier2/t2.rs (Tier-2符号化/復号)
    |         |
    |         +--- tier2/pi.rs
    |         +--- tcd.rs
    |         +--- io/bio.rs (Bio)
    |         +--- coding/tgt.rs (TagTree)
    |
    +--- tcd.rs パイプライン関数
              |
              +--- tier2/t2.rs
              +--- coding/t1.rs
              +--- transform/dwt.rs
              +--- transform/mct.rs
```

## 検証

各PR完了時:

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
```

## リファレンスファイル

- `reference/openjpeg/src/lib/openjp2/pi.c` — パケットイテレータ実装
- `reference/openjpeg/src/lib/openjp2/pi.h` — PI構造体・API定義
- `reference/openjpeg/src/lib/openjp2/t2.c` — Tier-2符号化/復号実装
- `reference/openjpeg/src/lib/openjp2/t2.h` — T2 API定義
- `reference/openjpeg/src/lib/openjp2/tcd.c` — タイルコーダ/デコーダ実装
- `reference/openjpeg/src/lib/openjp2/tcd.h` — TCD構造体・API定義
- `reference/openjpeg/src/lib/openjp2/j2k.h` — CP/TCP/TCCP/POC構造体定義
- `reference/openjpeg/src/lib/openjp2/openjpeg.h` — 公開API型定義
