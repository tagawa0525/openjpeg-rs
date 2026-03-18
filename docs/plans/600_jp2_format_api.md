# Phase 600: JP2ファイルフォーマット + 公開API

Status: IMPLEMENTED

## Context

Phase 500（J2Kコードストリーム）完了後の次フェーズ。JP2ファイルフォーマット（J2Kのラッパー層）のボックス読み書きと、コーデック全体を統合する公開APIファサードを実装する。

Phase 600が完了すると、JP2/J2K両形式の復号・符号化が公開API経由で動作する（マスタープランM4: MVP）。

## C版対応ファイル

| C版ファイル  | Rustモジュール | 概要                                       | C版LOC |
| ------------ | -------------- | ------------------------------------------ | ------ |
| `jp2.h`      | `jp2/mod.rs`   | ボックス定数、JP2構造体、カラー情報型      | ~520   |
| `jp2.c`      | `jp2/read.rs`  | ボックス読み（IHDR, COLR, FTYP等）+ 復号   | ~1,700 |
| `jp2.c`      | `jp2/write.rs` | ボックス書き（IHDR, COLR, FTYP等）+ 符号化 | ~1,700 |
| `openjpeg.h` | `types.rs`     | 圧縮/展開パラメータ（追加フィールド）      | ~400   |
| `openjpeg.c` | `api.rs`       | 公開APIファサード（コーデック生成・操作）  | ~1,100 |

C版合計: ~5,420 LOC → Rust推定: ~2,000-2,500 LOC

## JP2ファイル構造

```text
JP2ファイル:
  ├─ JP   (Signature Box)      [12B固定: length=12, type=0x6A502020, magic=0x0D0A870A]
  ├─ FTYP (File Type Box)      [20B: brand="jp2 ", minver=0, cl=["jp2 "]]
  ├─ JP2H (JP2 Header Box)     [スーパーボックス]
  │   ├─ IHDR (Image Header)   [22B: H(4), W(4), NC(2), BPC(1), C(1), UnkC(1), IPR(1)]
  │   ├─ BPCC (Bits Per Comp)  [可変: numcomps × 1B] ※BPC=255の場合のみ
  │   ├─ COLR (Colour Spec)    [可変: METH(1), PREC(1), APPROX(1), EnumCS(4) or ICC]
  │   ├─ CDEF (Channel Def)    [可変: N(2), {cn, typ, asoc}×N] ※オプション
  │   ├─ CMAP (Comp Mapping)   [可変: {CMP, MTYP, PCOL}×N] ※オプション
  │   └─ PCLR (Palette)        [可変: NE(2), NPC(1), data] ※オプション
  └─ JP2C (Codestream Box)     [可変: J2Kコードストリーム全体]
```

各ボックスは `[4B length][4B type][payload]` 形式。length=1の場合は拡張長（8B）を使用。

## サブPR構成

| PR   | ブランチ                  | スコープ                                          | 推定LOC | 依存      |
| ---- | ------------------------- | ------------------------------------------------- | ------- | --------- |
| 600a | `feat/jp2-decode`         | JP2モジュール基盤 + 必須ボックス読み + Jp2Decoder | ~800    | Phase 500 |
| 600b | `feat/jp2-optional-boxes` | オプションボックス（CDEF/CMAP/PCLR）+ カラー適用  | ~400    | 600a      |
| 600c | `feat/jp2-encode`         | ボックス書き込み + Jp2Encoder                     | ~500    | 600a      |
| 600d | `feat/api-facade`         | Stream trait + 公開APIファサード（api.rs）        | ~500    | 600c      |

マージ順: 600a → (600b, 600c は並行可) → 600d

## 設計判断

### JP2モジュール構成

C版の`jp2.c`（3,424 LOC）をRustでは3ファイルに分割:

```text
src/jp2/
  ├── mod.rs      # ボックス定数、JP2型定義（Jp2Color, Jp2Comps等）
  ├── read.rs     # ボックス読み関数 + Jp2Decoder
  └── write.rs    # ボックス書き関数 + Jp2Encoder
```

### ボックスハンドラ

C版の`opj_jp2_header_handler_t`（関数ポインタテーブル）はRustでは`match`式でボックスタイプごとにディスパッチする（J2Kマーカーハンドラと同じ方針）。

### JP2デコーダ/エンコーダ分離

C版の`opj_jp2_t`は復号・符号化で同一構造体を共有するが、Rustでは`Jp2Decoder`と`Jp2Encoder`に分離する（J2kDecoder/J2kEncoderと同じ方針）。

### Stream trait

マスタープランの設計判断に従い、Phase 600で`Stream` traitを導入する。既存の`MemoryStream`をこのtraitの実装にする。

```rust
pub trait Stream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
    fn write(&mut self, buf: &[u8]) -> Result<usize>;
    fn seek(&mut self, pos: u64) -> Result<()>;
    fn position(&self) -> u64;
    fn len(&self) -> u64;
}
```

ファイルI/Oは`std::fs::File`ベースの`FileStream`として600dで追加。

### 公開API設計

C版の`opj_codec_t`（不透明ポインタ+関数ポインタテーブル）は、Rustではenumベースのディスパッチに置換:

```rust
pub enum Codec {
    J2kDecoder(J2kDecoder),
    J2kEncoder(J2kEncoder),
    Jp2Decoder(Jp2Decoder),
    Jp2Encoder(Jp2Encoder),
}
```

C版の`opj_cparameters_t`/`opj_dparameters_t`はRust版の`CompressParams`/`DecompressParams`として`api.rs`に定義。

### カラースペースマッピング

JP2のEnumCS値とRust `ColorSpace` enumの対応:

| JP2 EnumCS | 値 | ColorSpace |
| ---------- | -- | ---------- |
| 16         | -- | Srgb       |
| 17         | -- | Gray       |
| 18         | -- | Sycc       |
| 24         | -- | Eycc       |

### スコープ外（延期）

| 項目                       | 延期先     | 理由                   |
| -------------------------- | ---------- | ---------------------- |
| HTJ2Kデコード              | Phase 700  | 別仕様                 |
| マルチスレッド             | Phase 800  | feature flag           |
| SIMD最適化                 | Phase 900  | パフォーマンスフェーズ |
| CLIツール                  | Phase 1000 | 入出力フォーマット依存 |
| Cinema/IMFプロファイル検証 | 将来       | プロファイル固有       |
| JPIP (Part-9)              | スコープ外 | 別プロトコル           |

## 検証

各PR完了時:

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
```

Phase 600全体完了時の追加検証:

- JP2エンコード→デコード ラウンドトリップ
- J2Kエンコード→JP2デコード（ボックス付加のみ）のクロス検証

## リファレンスファイル

- `reference/openjpeg/src/lib/openjp2/jp2.c` — C版JP2実装
- `reference/openjpeg/src/lib/openjp2/jp2.h` — ボックス定数、構造体定義
- `reference/openjpeg/src/lib/openjp2/openjpeg.c` — C版公開API実装
- `reference/openjpeg/src/lib/openjp2/openjpeg.h` — 公開API型定義
