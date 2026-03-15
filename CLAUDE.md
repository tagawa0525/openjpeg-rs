# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## openjpeg-rs

C版 [OpenJPEG](https://github.com/uclouvain/openjpeg) のRust移植。FFIバインディングではなく純粋な再実装。Rust edition 2024。
OpenJPEGはJPEG 2000コーデック（ITU-T T.800 / ISO/IEC 15444-1）のリファレンス実装。ロスレス・ロッシー圧縮、タイル分割、多解像度、プログレッシブ復号をサポート。

## ビルド・テスト・リント

```bash
cargo check
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
cargo test mqc_encode    # 特定テスト
```

## リファレンス

C版ソースは外部リポジトリを直接参照する（サブモジュールは使用しない）。必要に応じてローカルに手動cloneできる（`reference/` は `.gitignore` に含まれる）。

- [uclouvain/openjpeg](https://github.com/uclouvain/openjpeg) — C版OpenJPEG（移植元）

### C版の主要ソース（`src/lib/openjp2/`）

| ファイル         | 内容                                                         |
| ---------------- | ------------------------------------------------------------ |
| `openjpeg.h`     | 公開API定義（コーデック生成・圧縮・展開・ストリーム）        |
| `j2k.c/h`        | J2Kコードストリーム読み書き・マーカー処理（コーデック中核）  |
| `jp2.c/h`        | JP2ファイルフォーマット（J2Kのラッパー、メタデータボックス） |
| `tcd.c/h`        | タイルコーダ/デコーダ（タイル単位でDWT→T1→T2を制御）         |
| `dwt.c/h`        | 離散ウェーブレット変換（5-3可逆 / 9-7非可逆）                |
| `t1.c/h`         | Tier-1符号化（コードブロック係数のコンテキストMQ算術符号化） |
| `t2.c/h`         | Tier-2符号化（パケット化・プログレッション順序管理）         |
| `mqc.c/h`        | MQ算術コーダ（T1が使用するエントロピー符号化器）             |
| `mct.c/h`        | 多成分変換（RGB↔YCbCr等の色空間変換）                        |
| `pi.c/h`         | パケットイテレータ（5種のプログレッション順序を管理）        |
| `bio.c/h`        | ビット入出力                                                 |
| `cio.c/h`        | バイト入出力ストリーム                                       |
| `tgt.c/h`        | タグツリー（レート歪み最適化用）                             |
| `sparse_array.c` | スパース配列（メモリ効率化）                                 |
| `ht_dec.c`       | HTJ2Kデコーダ（High-Throughputモード）                       |

### C版の外部依存

| ライブラリ | 用途                        | コアライブラリでの要否 |
| ---------- | --------------------------- | ---------------------- |
| libm       | 数学関数（DWT・量子化等）   | 必須（Unix）           |
| pthreads   | マルチスレッド符号化・復号  | 必須（スレッド有効時） |
| libpng     | PNG画像入出力               | CLIツールのみ          |
| zlib       | PNG依存・ZIP圧縮            | CLIツールのみ          |
| libtiff    | TIFF画像入出力              | CLIツールのみ          |
| lcms2      | ICCプロファイルによる色管理 | CLIツールのみ          |

コアコーデック（libopenjp2）はlibm・pthreads以外の外部依存がない。PNG/TIFF/lcms2はCLIツール（opj_compress, opj_decompress, opj_dump）専用。Rust移植では標準ライブラリがlibm・スレッドをカバーするため、コアコーデックは外部crateゼロで実装可能。

### JPEG 2000仕様書

ITU-T T.800 / ISO/IEC 15444-1

## アーキテクチャ

### JPEG 2000処理パイプライン

```text
■ 符号化
入力画像 → MCT(色空間変換) → DWT(ウェーブレット変換) → 量子化
  → T1(算術符号化) → T2(パケット化) → J2Kマーカー付与 → [JP2ラッパー] → 出力

■ 復号（逆順）
入力 → [JP2パース] → J2Kマーカー解析 → T2(デパケット化)
  → T1(算術復号) → 逆量子化 → 逆DWT → 逆MCT → 出力画像
```

### モジュール依存関係（移植順序）

```text
Level 0: bio, cio, mqc, tgt, event, sparse_array  （基盤I/O・符号化プリミティブ）
Level 1: mct, dwt, t1                              （変換・符号化レイヤー）
Level 2: pi, t2                                     （パケット管理）
Level 3: tcd                                        （タイル処理統合）
Level 4: j2k                                        （コードストリーム）
Level 5: jp2                                        （ファイルフォーマット）
Level 6: openjpeg（公開API）                        （コーデックファサード）
```

下位レベルから順に移植する。上位モジュールは下位モジュールに依存するが逆はない。

### モジュール構成

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

### 主要データ構造

| C版                 | 概要                                                                                          |
| ------------------- | --------------------------------------------------------------------------------------------- |
| `opj_image_t`       | 画像全体（境界座標、成分配列、色空間、ICCプロファイル）                                       |
| `opj_image_comp_t`  | 画像成分（サンプリング間隔、精度、符号、ピクセルデータ）                                      |
| `opj_cparameters_t` | 圧縮パラメータ（タイル、DWT段数、コードブロックサイズ、品質レイヤー、プログレッション順序等） |
| `opj_dparameters_t` | 展開パラメータ（解像度削減、レイヤー数制限、ROI指定）                                         |
| `opj_tcd_tile_t`    | タイル（成分→解像度→サブバンド→プリシンクト→コードブロック階層）                              |

## PRワークフロー

### コミット構成

1. RED: テスト（`#[ignore = "not yet implemented"]` 付き）
2. GREEN: 実装（`#[ignore]` 除去）
3. REFACTOR: 必要に応じて
4. 全テスト・clippy・fmt通過を確認

### PR作成〜マージ

1. PR作成
2. `/gh-actions-check` でCopilotレビューワークフローが `completed/success` になるまで待つ
3. `/gh-pr-review` でコメント確認・対応
4. レビュー修正は独立した `fix(<scope>):` コミットで積む（RED/GREENに混入させない）
5. push後の再レビューサイクルも完了を確認
6. `docs/plans/` の進捗ステータスを更新（`docs:` コミット）
7. 全チェック通過後 `/gh-pr-merge --merge`

### 規約

- ブランチ命名: `feat/<module>-<機能>`, `test/<スコープ>`, `refactor/<スコープ>`, `docs/<スコープ>`
- コミット: Conventional Commits、scopeにモジュール名
- マージコミット: `## Why` / `## What` / `## Impact` セクション
- 計画書 (`docs/plans/`) を実装着手前にコミットすること

## 計画書

`docs/plans/NNN_<機能名>.md`（NNN = Phase番号×100 + 連番）。Status: PLANNED → IN_PROGRESS → IMPLEMENTED。C版の対応ファイル・関数を明記。
