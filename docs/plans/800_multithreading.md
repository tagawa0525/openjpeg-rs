# Phase 800: マルチスレッド（rayon feature flag）

Status: PLANNED

## Context

Phase 700（HTJ2K）完了後の次フェーズ。`rayon`による並列処理を`parallel` feature flag配下に追加する。デフォルトはシングルスレッドのまま。

C版はカスタムスレッドプール（`thread.c`, ~950 LOC）+ 各モジュールのジョブ投入で実装。Rust版では`rayon`のwork-stealingスケジューラで代替し、スレッドプール自体の実装は不要。

Phase 800が完了すると、`--features parallel`でT1コードブロック復号/符号化・DWT行列処理・MCTサンプル処理が並列化される。

## C版対応コード

| C版ファイル/関数                    | 並列化ポイント     | 概要                                                 |
| ----------------------------------- | ------------------ | ---------------------------------------------------- |
| `thread.c/h` (~950 LOC)             | スレッドプール基盤 | カスタムジョブキュー+ワーカースレッド → rayon で代替 |
| `t1.c: opj_t1_decode_cblks()`       | コードブロック単位 | 全プリシンクト→全コードブロックをジョブ投入          |
| `t1.c: opj_t1_encode_cblks()`       | コードブロック単位 | 同上（符号化側）                                     |
| `dwt.c: opj_dwt_encode_procedure()` | 行チャンク単位     | 垂直/水平パスを行分割してジョブ投入                  |
| `dwt.c: opj_dwt_decode_tile()`      | 行チャンク単位     | 同上（復号側）                                       |

C版合計: ~950 LOC（thread基盤）+ 各モジュールの並列化コード → Rust推定: ~600-800 LOC（rayonがスレッドプールを提供するため大幅削減）

## 並列化対象と粒度

```text
タイル復号パイプライン:
  T2(デパケット化)     — 順次（ビットストリーム依存）
  T1(コードブロック復号) ← ★ コードブロック単位で並列（最大効果）
  逆量子化 + 逆DWT     ← ★ 行/列単位で並列
  逆MCT               ← △ サンプル単位（粒度小、大画像のみ有効）
  DC level shift       — 順次（軽量、並列化不要）
```

### 優先順位

1. **T1コードブロック処理**（最高優先）: 一般的な画像で数百〜数千のコードブロックがあり、各コードブロックは完全に独立。C版の主要な並列化ポイント。
2. **DWT行/列処理**（高優先）: 垂直パスの各列、水平パスの各行が独立。解像度レベルごとに呼び出される。
3. **MCTサンプル処理**（低優先）: 各サンプルが独立だが粒度が小さい。大画像（>1Mサンプル）でのみ有効。

## サブPR構成

| PR   | ブランチ                | スコープ                                             | 推定LOC | 依存      |
| ---- | ----------------------- | ---------------------------------------------------- | ------- | --------- |
| 800a | `feat/parallel-t1`      | rayon feature flag + T1コードブロック並列復号/符号化 | ~400    | Phase 600 |
| 800b | `feat/parallel-dwt-mct` | DWT行/列並列処理 + MCTサンプル並列処理               | ~300    | 800a      |

マージ順: 800a → 800b

## 設計判断

### Feature flag

```toml
[features]
default = []
parallel = ["dep:rayon"]

[dependencies]
rayon = { version = "1.10", optional = true }
```

`#[cfg(feature = "parallel")]`でrayon版を選択、デフォルトはシングルスレッド版。

### T1コードブロック並列化（800a）

C版の`opj_t1_decode_cblks()`/`opj_t1_encode_cblks()`に対応するモジュールレベル関数を`coding/t1.rs`に追加。TCD階層（Component→Resolution→Band→Precinct→Codeblock）を走査し、各コードブロックのdecode/encodeジョブを収集して実行。

```rust
// コードブロック復号ジョブの収集と並列実行
pub fn t1_decode_cblks(tile: &mut TcdTile, tcp: &TileCodingParameters) -> Result<()> {
    // 1. 全コンポーネント→解像度→バンド→プリシンクト→コードブロックを走査
    // 2. 各コードブロックの復号ジョブ(入力データ+パラメータ)を収集
    // 3. parallel feature:  rayon::par_iter で並列実行
    //    default:           iter で順次実行
    // 4. 結果をコードブロックに書き戻す
}
```

**独立性の根拠**: 各コードブロックは固有のセグメントデータ(`segs`, `chunks`)を持ち、復号結果は`decoded_data`フィールドに書き込む。コードブロック間でデータ共有なし。

**T1ワークスペース**: C版はTLS（Thread-Local Storage）でT1ワークスペースを再利用するが、Rust版では各ジョブでT1インスタンスを生成する（64×64 i32 = 16KBと軽量）。プロファイリングで問題があれば`thread_local!`を検討。

### DWT行/列並列化（800b）

C版の行チャンク分割と同様、垂直パスの列群・水平パスの行群を分割して並列実行。

```rust
pub fn dwt_decode_tile_53(
    comp_data: &mut [i32], width: u32, height: u32, numresolutions: u32
) -> Result<()> {
    for resno in 0..numresolutions {
        // 水平パス: 行ごとに独立
        #[cfg(feature = "parallel")]
        rows.par_chunks_mut(stride).for_each(|row| { ... });

        // 垂直パス: 列ごとに独立（列抽出→変換→書き戻し）
        #[cfg(feature = "parallel")]
        (0..width).into_par_iter().for_each(|col| { ... });
    }
}
```

**注意点**: 垂直パスの列並列化は、メモリアクセスパターンが非連続（ストライドアクセス）なため、キャッシュ効率を考慮して適切なチャンクサイズを設定する。

### MCTサンプル並列化（800b）

3成分スライスに対するサンプル単位の並列処理。`rayon::join`で成分ペアを分割するか、インデックスベースの`par_iter`を使用。

### スコープ外（延期）

| 項目             | 延期先    | 理由                                                  |
| ---------------- | --------- | ----------------------------------------------------- |
| SIMD最適化       | Phase 900 | feature flag分離                                      |
| タイルレベル並列 | 将来      | J2Kパーサがタイルを逐次読み込むため、パーサ変更が必要 |
| T2パケット並列   | 将来      | ビットストリーム順序依存                              |

## 検証

各PR完了時:

```bash
cargo test
cargo test --features parallel
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --features parallel -- -D warnings
cargo fmt -- --check
```

Feature flag on/offの両方でテスト通過を確認。

## リファレンスファイル

- `reference/openjpeg/src/lib/openjp2/thread.c` — C版スレッドプール
- `reference/openjpeg/src/lib/openjp2/thread.h` — スレッドプールAPI
- `reference/openjpeg/src/lib/openjp2/t1.c` — C版T1コードブロック並列化（`opj_t1_decode_cblks`, `opj_t1_encode_cblks`）
- `reference/openjpeg/src/lib/openjp2/dwt.c` — C版DWT行並列化
