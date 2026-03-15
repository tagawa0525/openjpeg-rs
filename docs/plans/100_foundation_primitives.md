# Phase 100: 基盤プリミティブ

Status: IMPLEMENTED

## 概要

コーデック全体の基盤となるモジュール群を実装する。エラー型、共通定数・列挙型・整数演算、ビット/バイトI/O、イベント管理、画像データ構造。

## C版対応ファイル

| C版ファイル                | Rustモジュール | 概要                                           |
| -------------------------- | -------------- | ---------------------------------------------- |
| （なし）                   | `error.rs`     | エラー型・Result型エイリアス                   |
| `opj_common.h`             | `types.rs`     | 共通定数                                       |
| `openjpeg.h`（型定義部分） | `types.rs`     | 列挙型・パラメータ構造体                       |
| `opj_intmath.h`            | `types.rs`     | 整数演算ユーティリティ                         |
| `bio.c/h`                  | `io/bio.rs`    | ビット単位入出力                               |
| `cio.c/h`                  | `io/cio.rs`    | バイトストリーム入出力（初期は`MemoryStream`） |
| `event.c/h`                | `io/event.rs`  | イベント（ログ）管理                           |
| `image.c` + `openjpeg.h`   | `image.rs`     | 画像データ構造                                 |

## モジュール詳細

### error.rs

単一の `Error` enum と `Result<T>` 型エイリアス。

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("buffer too small")]
    BufferTooSmall,
    #[error("end of stream")]
    EndOfStream,
    #[error("I/O error: {0}")]
    IoError(String),
}

pub type Result<T> = std::result::Result<T, Error>;
```

Phase進行に伴いバリアントを追加する。

### types.rs — 定数

C版 `opj_common.h` + `openjpeg.h` の定数マクロ。

| C版定数                                           | Rust                                  | 値                     |
| ------------------------------------------------- | ------------------------------------- | ---------------------- |
| `OPJ_J2K_MAXRLVLS`                                | `J2K_MAXRLVLS`                        | 33                     |
| `OPJ_J2K_MAXBANDS`                                | `J2K_MAXBANDS`                        | `3 * J2K_MAXRLVLS - 2` |
| `OPJ_J2K_DEFAULT_NB_SEGS`                         | `J2K_DEFAULT_NB_SEGS`                 | 10                     |
| `OPJ_J2K_STREAM_CHUNK_SIZE`                       | `J2K_STREAM_CHUNK_SIZE`               | 0x100000               |
| `OPJ_J2K_DEFAULT_CBLK_DATA_SIZE`                  | `J2K_DEFAULT_CBLK_DATA_SIZE`          | 8192                   |
| `OPJ_PATH_LEN`                                    | `PATH_LEN`                            | 4096                   |
| `OPJ_IMG_INFO`                                    | `IMG_INFO`                            | 1                      |
| `OPJ_J2K_MH_INFO` / `OPJ_J2K_TH_INFO`             | `J2K_MH_INFO` / `J2K_TH_INFO`         | 2 / 4                  |
| `OPJ_J2K_TCH_INFO` / `OPJ_J2K_MH_IND`             | `J2K_TCH_INFO` / `J2K_MH_IND`         | 8 / 16                 |
| `OPJ_J2K_TH_IND` / `OPJ_JP2_INFO` / `OPJ_JP2_IND` | `J2K_TH_IND` / `JP2_INFO` / `JP2_IND` | 32 / 64 / 128          |
| `OPJ_COMMON_CBLK_DATA_EXTRA`                      | `COMMON_CBLK_DATA_EXTRA`              | 2                      |
| `OPJ_COMP_PARAM_DEFAULT_CBLOCKW`                  | `COMP_PARAM_DEFAULT_CBLOCKW`          | 64                     |
| `OPJ_COMP_PARAM_DEFAULT_CBLOCKH`                  | `COMP_PARAM_DEFAULT_CBLOCKH`          | 64                     |
| `OPJ_COMP_PARAM_DEFAULT_NUMRESOLUTION`            | `COMP_PARAM_DEFAULT_NUMRESOLUTION`    | 6                      |

### types.rs — 列挙型

C版 `openjpeg.h` の列挙型。

```rust
pub enum ProgressionOrder { Unknown, Lrcp, Rlcp, Rpcl, Pcrl, Cprl }
pub enum ColorSpace { Unknown, Unspecified, Srgb, Gray, Sycc, Eycc, Cmyk }
pub enum CodecFormat { Unknown, J2k, Jpt, Jp2 }
```

各enumに `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`。

### types.rs — 整数演算

C版 `opj_intmath.h` のインライン関数群。Rustでは `#[inline]` 関数。

| C版関数                         | Rust関数                | 備考                                 |
| ------------------------------- | ----------------------- | ------------------------------------ |
| `opj_int_min` / `opj_uint_min`  | 不要                    | `std::cmp::min` で代替               |
| `opj_int_max` / `opj_uint_max`  | 不要                    | `std::cmp::max` で代替               |
| `opj_uint_adds`                 | 不要                    | `u32::saturating_add` で直接使用     |
| `opj_uint_subs`                 | 不要                    | `u32::saturating_sub` で直接使用     |
| `opj_int_clamp`                 | 不要                    | `i32::clamp` で直接使用              |
| `opj_int64_clamp`               | 不要                    | `i64::clamp` で直接使用              |
| `opj_int_abs`                   | 不要                    | `i32::abs` で直接使用                |
| `opj_int_ceildiv`               | `int_ceildiv`           | 切り上げ除算                         |
| `opj_uint_ceildiv`              | `uint_ceildiv`          | 切り上げ除算（u32）                  |
| `opj_uint64_ceildiv_res_uint32` | `uint64_ceildiv_as_u32` | u64切り上げ除算→u32                  |
| `opj_int_ceildivpow2`           | `int_ceildivpow2`       | 2^bでの切り上げ除算                  |
| `opj_int64_ceildivpow2`         | `int64_ceildivpow2`     | i64版                                |
| `opj_uint_ceildivpow2`          | `uint_ceildivpow2`      | u32版                                |
| `opj_int_floordivpow2`          | `int_floordivpow2`      | 2^bでの切り捨て除算                  |
| `opj_uint_floordivpow2`         | 不要                    | `>> b` で直接表現                    |
| `opj_int_floorlog2`             | `int_floorlog2`         | 床log2                               |
| `opj_uint_floorlog2`            | `uint_floorlog2`        | u32版                                |
| `opj_int_fix_mul`               | `int_fix_mul`           | 固定小数点乗算（13ビットシフト）     |
| `opj_int_fix_mul_t1`            | スタブ                  | T1_NMSEDEC_FRACBITS（Phase 300）依存 |
| `opj_int_add_no_overflow`       | 不要                    | `i32::wrapping_add` で直接使用       |
| `opj_int_sub_no_overflow`       | 不要                    | `i32::wrapping_sub` で直接使用       |

標準ライブラリで直接代替可能な関数はラッパーを作らず呼び出し側で直接使用する。

### io/bio.rs

ビット単位の入出力。C版の `opj_bio_t` をRustに移植。

```rust
pub struct Bio<'a> {
    data: &'a mut [u8],  // エンコード時は &mut [u8]
    pos: usize,          // 現在のバイト位置
    buf: u32,            // 一時バッファ
    ct: u32,             // エンコード: 書き込み可能ビット数 / デコード: 読み取りビット数
}
```

C版はポインタ（start/end/bp）だが、Rustではスライス+インデックスで安全に置換。
エンコーダ/デコーダは別コンストラクタ `Bio::encoder()` / `Bio::decoder()` で生成。

| C版関数                  | Rustメソッド                     |
| ------------------------ | -------------------------------- |
| `opj_bio_init_enc`       | `Bio::encoder(buf)`              |
| `opj_bio_init_dec`       | `Bio::decoder(buf)`              |
| `opj_bio_write`          | `bio.write(v, n)` → `Result<()>` |
| `opj_bio_putbit`         | `bio.put_bit(b)` → `Result<()>`  |
| `opj_bio_read`           | `bio.read(n)` → `Result<u32>`    |
| `opj_bio_flush`          | `bio.flush()` → `Result<()>`     |
| `opj_bio_inalign`        | `bio.inalign()` → `Result<()>`   |
| `opj_bio_numbytes`       | `bio.num_bytes()` → `usize`      |
| `opj_bio_byteout`        | 内部メソッド `byte_out()`        |
| `opj_bio_bytein`         | 内部メソッド `byte_in()`         |
| `opj_bio_getbit`         | 内部メソッド `get_bit()`         |
| `opj_bio_create/destroy` | 不要（Rustの所有権で管理）       |

C版で無視されていたエラー（byteout/byteinの戻り値無視）はRustでは `Result` で伝播する。

### io/cio.rs

バイトストリーム入出力。Phase 100では `MemoryStream`（`Vec<u8>` + カーソル）を実装。
C版の `opj_stream_private_t` はコールバック関数ポインタベースだが、初期実装ではメモリバッファ専用の具象型とする。

```rust
pub struct MemoryStream {
    data: Vec<u8>,
    position: usize,
    is_input: bool,
}
```

バイトオーダー変換関数（C版の `opj_read_bytes` / `opj_write_bytes`）はJPEG 2000がビッグエンディアン固定のため、`u32::from_be_bytes` / `u32::to_be_bytes` で代替。

| C版関数                           | Rustメソッド/関数                         |
| --------------------------------- | ----------------------------------------- |
| `opj_stream_create`               | `MemoryStream::new()`                     |
| `opj_stream_read_data`            | `stream.read(buf)` → `Result<usize>`      |
| `opj_stream_write_data`           | `stream.write(buf)` → `Result<usize>`     |
| `opj_stream_skip`                 | `stream.skip(n)` → `Result<()>`           |
| `opj_stream_seek`                 | `stream.seek(pos)` → `Result<()>`         |
| `opj_stream_tell`                 | `stream.tell()` → `usize`                 |
| `opj_stream_get_number_byte_left` | `stream.bytes_left()` → `usize`           |
| `opj_stream_flush`                | `stream.flush()` → `Result<()>`           |
| `opj_write_bytes_BE/LE`           | `cio::write_bytes_be(buf, val, n)`        |
| `opj_read_bytes_BE/LE`            | `cio::read_bytes_be(buf, n)` → `u32`      |
| `opj_write_double/float`          | `cio::write_f64_be` / `cio::write_f32_be` |
| `opj_read_double/float`           | `cio::read_f64_be` / `cio::read_f32_be`   |

C版の内部バッファリング（`m_stored_data`、`m_buffer_size`）は `MemoryStream` では不要（全データがメモリ上）。Phase 600で `Stream` trait を導入する際にバッファリング戦略を再検討。

### io/event.rs

イベント（ログ）管理。C版の `opj_event_mgr_t` はコールバック関数ポインタだが、Rustではクロージャを使用。

```rust
pub struct EventManager {
    error_handler: Option<Box<dyn Fn(&str)>>,
    warning_handler: Option<Box<dyn Fn(&str)>>,
    info_handler: Option<Box<dyn Fn(&str)>>,
}
```

| C版関数                         | Rustメソッド                                            |
| ------------------------------- | ------------------------------------------------------- |
| `opj_event_msg`                 | `mgr.error(msg)` / `mgr.warning(msg)` / `mgr.info(msg)` |
| `opj_set_default_event_handler` | `EventManager::default()`                               |

C版の `EVT_ERROR` / `EVT_WARNING` / `EVT_INFO` 定数は不要（メソッドで分離）。
C版の `va_list` + `vsnprintf` は Rust の `format!` マクロで代替。

### image.rs

画像データ構造。C版 `opj_image_t` / `opj_image_comp_t` の移植。

```rust
pub struct ImageComp {
    pub dx: u32,
    pub dy: u32,
    pub w: u32,
    pub h: u32,
    pub x0: u32,
    pub y0: u32,
    pub prec: u32,
    pub sgnd: bool,
    pub resno_decoded: u32,
    pub factor: u32,
    pub data: Vec<i32>,
    pub alpha: u16,
}

pub struct Image {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
    pub numcomps: u32,
    pub color_space: ColorSpace,
    pub comps: Vec<ImageComp>,
    pub icc_profile: Vec<u8>,
}
```

| C版関数                        | Rustメソッド                           |
| ------------------------------ | -------------------------------------- |
| `opj_image_create`             | `Image::new(params, color_space)`      |
| `opj_image_tile_create`        | `Image::new_tile(params, color_space)` |
| `opj_image_destroy`            | 不要（Drop）                           |
| `opj_copy_image_header`        | `image.clone_header()`                 |
| `opj_image_comp_header_update` | Phase 400で実装（`Cp` 依存）           |

`ImageCompParam`（C版 `opj_image_cmptparm_t`）はビルダー用の入力パラメータ構造体。

```rust
pub struct ImageCompParam {
    pub dx: u32,
    pub dy: u32,
    pub w: u32,
    pub h: u32,
    pub x0: u32,
    pub y0: u32,
    pub prec: u32,
    pub sgnd: bool,
}
```

## コミット計画

TDDサイクルに従い、以下の順序でコミットする。

### 1. error + types

1. RED: `error.rs`, `types.rs` のテスト（定数値、列挙型、整数演算関数の入出力・境界値）
2. GREEN: 実装

### 2. io/bio

1. RED: `io/bio.rs` のテスト（エンコード→デコードのラウンドトリップ、0xFF後の7ビット制限）
2. GREEN: 実装

### 3. io/cio

1. RED: `io/cio.rs` のテスト（バイトオーダー変換、MemoryStreamの読み書き・seek/skip）
2. GREEN: 実装

### 4. io/event

1. RED: `io/event.rs` のテスト（コールバック呼び出し検証）
2. GREEN: 実装

### 5. image

1. RED: `image.rs` のテスト（生成、ヘッダーコピー）
2. GREEN: 実装

## lib.rs構成

```rust
pub mod error;
pub mod types;
pub mod io;
pub mod image;
```

## 依存関係

外部crate: `thiserror`（エラー型導出）

## テスト方針

- 各関数の正常系・異常系をカバー
- 整数演算: C版と同一入力で同一出力を確認（手計算値）
- `io/bio`: エンコード→デコードのラウンドトリップ、0xFF後の7ビット制限
- `io/cio`: ビッグエンディアン変換の正確性、MemoryStreamのseek/skip
- `io/event`: ハンドラの呼び出し・メッセージ内容の検証
- `image`: 生成パラメータの反映、ヘッダーコピーの独立性

## 検証コマンド

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
```
