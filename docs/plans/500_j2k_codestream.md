# Phase 500: J2Kコードストリーム

Status: IMPLEMENTED

## Context

Phase 400（PI・T2・TCD）完了後の次フェーズ。J2Kコードストリームのマーカー読み書きと復号/符号化の統合パイプラインを実装する。Phase 400で `src/j2k/params.rs` にパラメータ構造体を配置済み。

Phase 500が完了すると、生のJ2Kコードストリーム（.j2k/.j2c）のデコード/エンコードが可能になる。

## C版対応ファイル

| C版ファイル | Rustモジュール   | 概要                                           | C版LOC |
| ----------- | ---------------- | ---------------------------------------------- | ------ |
| `j2k.c`     | `j2k/markers.rs` | マーカー読み書き（SIZ, COD, QCD, SOT等）       | ~5,000 |
| `j2k.c`     | `j2k/read.rs`    | 復号パイプライン（ヘッダ解析・タイル復号）     | ~4,000 |
| `j2k.c`     | `j2k/write.rs`   | 符号化パイプライン（ヘッダ書込・タイル符号化） | ~3,000 |
| `j2k.h`     | `j2k/mod.rs`     | J2K状態構造体、マーカー定数                    | ~600   |
| `j2k.h`     | `j2k/params.rs`  | パラメータ構造体の追加フィールド               | ~200   |

C版合計: ~13,600 LOC → Rust推定: ~5,000-6,500 LOC

## サブPR構成

| PR   | ブランチ                    | スコープ                                             | 推定LOC | 依存      |
| ---- | --------------------------- | ---------------------------------------------------- | ------- | --------- |
| 500a | `feat/j2k-markers-infra`    | マーカー定数 + J2Kデコーダ状態 + マーカー読み基盤    | ~800    | Phase 400 |
| 500b | `feat/j2k-main-header`      | SIZ/COD/QCD/COM マーカー読み（メインヘッダ解析完成） | ~1,200  | 500a      |
| 500c | `feat/j2k-tile-decode`      | SOT/SOD処理 + タイルヘッダ読み + タイル復号統合      | ~1,500  | 500b      |
| 500d | `feat/j2k-encode`           | マーカー書き込み + 符号化パイプライン                | ~1,500  | 500c      |
| 500e | `feat/j2k-optional-markers` | COC/QCC/POC/RGN/TLM/PLT等の追加マーカー              | ~1,000  | 500c      |

マージ順: 500a → 500b → 500c → (500d, 500e は並行可)

## 設計判断

### C版 procedure_list の置き換え

C版の関数ポインタリスト+exec パターンは、Rustでは直接的な関数呼び出しチェーンで表現する。

### マーカーハンドラ

C版のマーカーハンドラテーブルは `match` 式でディスパッチする。

### デコーダ状態機械

```text
None → MhSoc → MhSiz → Mh → TphSot → Tph → Data → Eoc
```

### J2Kコーデック分離

C版の `opj_j2k_t` union を `J2kDecoder` と `J2kEncoder` の2つの構造体に分離する。

### スコープ外（延期）

| 項目                       | 延期先    | 理由             |
| -------------------------- | --------- | ---------------- |
| JP2ファイルフォーマット    | Phase 600 | J2Kラッパー層    |
| Cinema/IMFプロファイル検証 | Phase 600 | プロファイル固有 |
| HTJ2Kデコード              | Phase 700 | 別仕様           |
| ウィンドウ復号             | Phase 800 | 最適化フェーズ   |

## 検証

各PR完了時:

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
```

## リファレンスファイル

- `reference/openjpeg/src/lib/openjp2/j2k.c` — C版J2K実装
- `reference/openjpeg/src/lib/openjp2/j2k.h` — マーカー定数、構造体定義
- `reference/openjpeg/src/lib/openjp2/openjpeg.h` — 公開API型定義
