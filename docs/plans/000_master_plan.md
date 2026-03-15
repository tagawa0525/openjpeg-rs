# openjpeg-rs 移植マスタープラン

Status: PLANNED

## Context

C版 OpenJPEG（uclouvain/openjpeg）をRustに純粋再実装する。JPEG 2000コーデック（ITU-T T.800 / ISO/IEC 15444-1）のリファレンス実装で、コアライブラリ約42K LOC + CLIツール約11K LOC。

単一crate構成（workspace不使用）。コアコーデックの外部依存は `thiserror` のみ。Rust edition 2024。

## スコープ外

JPIP（Part-9）、JPWL（Part-11）、JP3D（Part-10）、JPSEC（Part-8）、JPX（Part-2拡張）、HTJ2Kエンコーダ（C版も未実装）、ビューア/GUI、Java/Pythonバインディング。

## Phase一覧

| Phase | 名称                           | C版ソース                                         | 概算LOC      | 依存     |
| ----- | ------------------------------ | ------------------------------------------------- | ------------ | -------- |
| 100   | 基盤プリミティブ               | bio, cio, event, image, opj_intmath, opj_common   | ~1,400       | なし     |
| 200   | 符号化プリミティブ             | mqc, tgt, sparse_array                            | ~1,200       | 100      |
| 300   | 変換・符号化レイヤー（スカラ） | mct, invert, dwt（スカラ）, t1（スカラ）, t1_luts | ~10,500      | 200      |
| 400   | パケット管理 + タイル処理      | pi, t2, tcd                                       | ~6,800       | 300      |
| 500   | J2Kコードストリーム            | j2k（復号先行→符号化）                            | ~13,600      | 400      |
| 600   | JP2フォーマット + 公開API      | jp2, openjpeg                                     | ~4,900       | 500      |
| 700   | HTJ2K                          | ht_dec, t1_ht_luts                                | ~3,700       | 300, 500 |
| 800   | マルチスレッド                 | rayon（feature flag）                             | ~950         | 600      |
| 900   | SIMD最適化                     | dwt/t1/mct のSIMDパス                             | (既存最適化) | 300, 800 |
| 1000  | CLIツール                      | opj_compress, opj_decompress, opj_dump            | ~10,700      | 600      |

### 依存グラフ

```text
100 → 200 → 300 → 400 → 500 → 600 → 800 → 900
                │                 │         │
                └──── 700 ────────┘    1000 ─┘
```

### マイルストーン

| MS | Phase    | 内容                                              |
| -- | -------- | ------------------------------------------------- |
| M1 | 200完了  | 基盤プリミティブが単体テスト通過                  |
| M2 | 400完了  | タイル単位の符号化/復号が動作                     |
| M3 | 500a完了 | J2Kファイルの復号が動作                           |
| M4 | 600完了  | **MVP**: JP2/J2Kの符号化・復号が公開API経由で動作 |
| M5 | 700完了  | HTJ2K復号対応                                     |
| M6 | 900完了  | SIMD + マルチスレッドで実用性能                   |
| M7 | 1000完了 | C版CLIツール同等の機能                            |

## Rust型マッピング

| C版                           | Rust                                                                    | 備考                           |
| ----------------------------- | ----------------------------------------------------------------------- | ------------------------------ |
| `OPJ_BOOL`                    | `bool`                                                                  |                                |
| `OPJ_INT32` / `OPJ_UINT32`    | `i32` / `u32`                                                           |                                |
| `OPJ_FLOAT32` / `OPJ_FLOAT64` | `f32` / `f64`                                                           |                                |
| `OPJ_BYTE`                    | `u8`                                                                    |                                |
| `OPJ_OFF_T`                   | `i64`                                                                   |                                |
| `OPJ_SIZE_T`                  | `usize`                                                                 |                                |
| `opj_image_t`                 | `Image`                                                                 |                                |
| `opj_image_comp_t`            | `ImageComp`                                                             | `data: Vec<i32>`               |
| `opj_cparameters_t`           | `CompressParams`                                                        | JPWL除外                       |
| `opj_dparameters_t`           | `DecompressParams`                                                      | JPWL除外                       |
| `opj_bio_t`                   | `Bio`                                                                   | スライス+インデックス          |
| `opj_stream_private_t`        | `MemoryStream`（初期）→ `Stream` trait（Phase 600）                     |                                |
| `opj_event_mgr_t`             | `EventManager`                                                          | `Fn` クロージャ                |
| `opj_mqc_t`                   | `Mqc`                                                                   | 状態遷移をインデックスベースに |
| `opj_tgt_node_t`              | `TgtNode`（`Vec` アリーナ）                                             | 親ポインタ→インデックス        |
| `opj_tcd_tile_t` 階層         | `TcdTile` / `TcdTileComp` / `TcdResolution` / `TcdBand` / `TcdPrecinct` |                                |
| `opj_j2k_t`                   | `J2k`                                                                   |                                |
| `opj_tcp_t` / `opj_cp_t`      | `Tcp` / `Cp`                                                            |                                |
| `OPJ_PROG_ORDER`              | `ProgressionOrder` enum                                                 |                                |
| `OPJ_COLOR_SPACE`             | `ColorSpace` enum                                                       |                                |
| `OPJ_CODEC_FORMAT`            | `CodecFormat` enum                                                      |                                |

## 設計判断

### エラーハンドリング

`thiserror` による `Error` enum 導出 + `Result<T>` 型エイリアス。

### ストリームI/O

初期は具象型 `MemoryStream`（`Vec<u8>` + カーソル）。公開API設計（Phase 600）で `Stream` trait を導入するか判断。

### J2Kマーカーハンドラ

C版の `opj_procedure_list_t`（関数ポインタリスト）はRustでは `match` 式に置換。全マーカーは静的に既知。

### j2k.c の分割

13K LOCの単一ファイルを論理分割: `j2k/mod.rs`, `j2k/markers.rs`, `j2k/read.rs`, `j2k/write.rs`, `j2k/params.rs`。

### SIMD

Phase 300はスカラのみ。Phase 900で `std::arch`（SSE2/AVX2）を追加。全SIMD関数にスカラフォールバックを維持。

### スレッド

Phase 800で `rayon` を `parallel` feature flag で追加。デフォルトはシングルスレッド。

### unsafe方針

原則禁止。Phase 900のSIMD（`std::arch`）でのみ許容。

### 不要なC版コード

`opj_malloc.c/h`, `opj_clock.c/h`, `function_list.c/h` — Rustの標準機能で代替。

## モジュール構成（最終形）

```text
src/
├── lib.rs
├── error.rs              # Error, Result
├── types.rs              # 共通定数、列挙型、パラメータ構造体、整数演算
├── image.rs              # Image構造体
├── io/                   # I/O基盤（Level 0）
│   ├── mod.rs
│   ├── bio.rs            # ビットI/O
│   ├── cio.rs            # バイトI/Oストリーム
│   └── event.rs          # イベント管理
├── coding/               # 符号化プリミティブ＋Tier-1（Level 0-1）
│   ├── mod.rs
│   ├── mqc.rs            # MQ算術コーダ
│   ├── tgt.rs            # タグツリー
│   ├── sparse_array.rs   # スパース配列
│   ├── t1.rs             # Tier-1符号化
│   └── t1_luts.rs        # T1ルックアップテーブル
├── transform/            # 信号変換（Level 1）
│   ├── mod.rs
│   ├── mct.rs            # 多成分変換
│   ├── invert.rs         # 行列逆行列
│   └── dwt.rs            # 離散ウェーブレット変換
├── tier2/                # パケット管理（Level 2）
│   ├── mod.rs
│   ├── pi.rs             # パケットイテレータ
│   └── t2.rs             # Tier-2パケット化
├── tcd.rs                # タイルコーダ/デコーダ（Level 3）
├── j2k/                  # J2Kコードストリーム（Level 4）
│   ├── mod.rs
│   ├── markers.rs
│   ├── read.rs
│   ├── write.rs
│   └── params.rs
├── jp2.rs                # JP2ファイルフォーマット（Level 5）
├── ht_dec.rs             # HTJ2Kデコーダ
├── api.rs                # 公開APIファサード（Level 6）
└── bin/                  # CLIツール
    ├── opj_decompress.rs
    ├── opj_compress.rs
    └── opj_dump.rs
```

## Feature flags

```toml
[features]
default = []
parallel = ["dep:rayon"]                    # Phase 800
cli = ["dep:clap", "dep:png", "dep:tiff"]   # Phase 1000
```

## テスト戦略

| 段階             | Phase   | 手法                                         |
| ---------------- | ------- | -------------------------------------------- |
| 単体テスト       | 100-300 | 各関数の入出力検証、ラウンドトリップ、境界値 |
| 統合テスト       | 400+    | パイプライン結合（タイル符号化→復号）        |
| 適合性テスト     | 500+    | C版テストデータ（.j2k/.jp2）での復号結果比較 |
| ラウンドトリップ | 600+    | 符号化→復号で元画像一致                      |
| ファジング       | 600+    | `cargo-fuzz` による堅牢性テスト              |

テストデータは初期段階では手作りバイト列、Phase 500以降でC版の `tests/` を参照。

## 検証

各Phaseの計画書を個別に作成し、Phase単位でPRを出す。各PRで:

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
```

## リファレンスファイル（実装時に参照）

- `reference/openjpeg/src/lib/openjp2/openjpeg.h` — 公開API・全型定義
- `reference/openjpeg/src/lib/openjp2/j2k.h` — 内部コーデック構造体
- `reference/openjpeg/src/lib/openjp2/tcd.h` — タイル階層データ構造
- `reference/openjpeg/src/lib/openjp2/mqc.h` — MQ算術コーダ
