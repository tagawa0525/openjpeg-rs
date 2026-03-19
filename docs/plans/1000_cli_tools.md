# Phase 1000: CLIツール

Status: IMPLEMENTED

## Context

Phase 800（マルチスレッド）完了後の次フェーズ。C版の`opj_decompress`, `opj_compress`, `opj_dump`に相当するCLIツールを実装する。

Phase 1000が完了すると、コマンドラインからJPEG 2000ファイルの復号・メタデータ表示が可能になる（マスタープランM7）。

**注意**: 現在の`encode()`はヘッダ+空タイルのスケルトン実装のため、`opj_compress`の本格実装はエンコードパイプライン完成後に延期する。Phase 1000ではデコード（`opj_decompress`）とメタデータ表示（`opj_dump`）を優先する。

## C版対応ファイル

| C版ファイル        | Rustバイナリ         | 概要                       | C版LOC |
| ------------------ | -------------------- | -------------------------- | ------ |
| `opj_decompress.c` | `opj_decompress`     | JPEG 2000復号 → 画像出力   | ~4,600 |
| `opj_dump.c`       | `opj_dump`           | コードストリーム情報表示   | ~1,600 |
| `opj_compress.c`   | `opj_compress`       | 画像入力 → JPEG 2000符号化 | ~4,900 |
| `convert.c/h`      | `image_io`モジュール | 画像フォーマット変換       | ~5,200 |

C版CLI合計: ~16,300 LOC → Rust推定: ~2,000-3,000 LOC（clap + 画像クレート活用）

## サポートフォーマット

### 入力（復号元）

| フォーマット | 拡張子     | 備考                    |
| ------------ | ---------- | ----------------------- |
| J2K          | .j2k, .j2c | 生コードストリーム      |
| JP2          | .jp2       | JP2ファイルフォーマット |

### 出力（復号先）

| フォーマット | 拡張子     | 備考                                     |
| ------------ | ---------- | ---------------------------------------- |
| PGM/PPM      | .pgm, .ppm | Portable Graymap/Pixmap（P5/P6バイナリ） |
| PGX          | .pgx       | JPEG 2000テストフォーマット（1成分）     |
| PNG          | .png       | `png`クレート使用（`cli` feature）       |

## サブPR構成

| PR    | ブランチ              | スコープ                                   | 推定LOC | 依存      |
| ----- | --------------------- | ------------------------------------------ | ------- | --------- |
| 1000a | `feat/cli-decompress` | CLI基盤 + PGM/PPM/PGX出力 + opj_decompress | ~1,000  | Phase 600 |
| 1000b | `feat/cli-dump`       | opj_dump（コードストリーム情報表示）       | ~400    | 1000a     |
| 1000c | `feat/cli-png`        | PNG出力対応（`cli` feature + pngクレート） | ~300    | 1000a     |

マージ順: 1000a → (1000b, 1000c は並行可)

## 設計判断

### バイナリ構成

`opj_decompress`はディレクトリベース（サブモジュール`image_io`を含むため）。`opj_dump`は単一ファイル。

```toml
[[bin]]
name = "opj_decompress"
path = "src/bin/opj_decompress/main.rs"
required-features = ["cli"]

[[bin]]
name = "opj_dump"
path = "src/bin/opj_dump.rs"
required-features = ["cli"]
```

### Feature flag

```toml
[features]
cli = ["dep:clap"]
cli-png = ["cli", "dep:png"]
```

CLIツールは`cli` feature配下。コアライブラリへの影響なし。

### CLIフレームワーク

`clap` deriveマクロを使用。C版のgetopt手動パースを大幅簡略化。

### 画像フォーマットI/O

PGM/PPM/PGXは標準ライブラリのみで実装（外部依存なし）。PNGは`png`クレートを使用（`cli-png` feature）。

### opj_decompress オプション

```text
-i <file>         入力JPEG 2000ファイル
-o <file>         出力画像ファイル（拡張子でフォーマット判定）
-r <reduce>       解像度削減レベル数
-l <layers>       復号品質レイヤー上限
-v                詳細出力
```

### opj_dump オプション

```text
-i <file>         入力JPEG 2000ファイル
-o <file>         出力ファイル（デフォルト: stdout）
```

### スコープ外（延期）

| 項目                    | 延期先                       | 理由                       |
| ----------------------- | ---------------------------- | -------------------------- |
| opj_compress            | エンコードパイプライン完成後 | 現在のencode()はスケルトン |
| TIFF/BMP入出力          | 将来                         | 外部クレート依存           |
| デコード領域指定 (-d)   | 将来                         | ウィンドウデコード未実装   |
| タイル指定デコード (-t) | 将来                         | API未対応                  |
| ディレクトリ一括処理    | 将来                         | 低優先                     |

## 検証

各PR完了時:

```bash
cargo test
cargo test --features cli
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --features cli -- -D warnings
cargo fmt -- --check
```

## リファレンスファイル

- `reference/openjpeg/src/bin/jp2/opj_decompress.c` — C版復号ツール
- `reference/openjpeg/src/bin/jp2/opj_dump.c` — C版ダンプツール
- `reference/openjpeg/src/bin/jp2/convert.c` — C版画像フォーマット変換
