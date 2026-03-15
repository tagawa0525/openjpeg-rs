# Phase 300: 変換・符号化レイヤー（スカラ）

Status: PLANNED

## Context

Phase 200（MQ算術コーダ・タグツリー・スパース配列）完了後の次フェーズ。JPEG 2000パイプラインのコアとなる信号変換（MCT・DWT）とTier-1符号化（T1）のスカラ実装を行う。

Phase 300が完了すると、個別のコードブロック・画像成分に対する符号化/復号の基本演算が可能になる。Phase 400（TCD）がこれらを組み合わせてタイル単位の処理を実現する。

SIMD最適化はPhase 900。64×64特化パスも後続フェーズ。

## C版対応ファイル

| C版ファイル  | Rustモジュール        | 概要                                     | C版LOC |
| ------------ | --------------------- | ---------------------------------------- | ------ |
| `t1_luts.h`  | `coding/t1_luts.rs`   | T1ルックアップテーブル                   | ~175   |
| `invert.c/h` | `transform/invert.rs` | 行列逆行列（LU分解）                     | ~360   |
| `mct.c/h`    | `transform/mct.rs`    | 多成分変換（RGB↔YCbCr）                  | ~620   |
| `dwt.c/h`    | `transform/dwt.rs`    | 離散ウェーブレット変換（スカラのみ）     | ~4,100 |
| `t1.c/h`     | `coding/t1.rs`        | Tier-1符号化（コンテキストMQ算術符号化） | ~3,010 |

C版合計: ~8,265 LOC → Rust推定: ~3,000-3,500 LOC（SIMD除外、Rustの簡潔さ）

## サブPR構成

| PR   | ブランチ                    | スコープ                                              | 推定LOC | 依存       |
| ---- | --------------------------- | ----------------------------------------------------- | ------- | ---------- |
| 300a | `feat/transform-mct-invert` | t1_luts + invert + mct + types追加 + transform/mod.rs | ~700    | Phase 200  |
| 300c | `feat/transform-dwt`        | dwt（正規化テーブル含む）                             | ~1,000  | 300a       |
| 300b | `feat/coding-t1`            | t1（t1_getwmsedec含む）                               | ~1,500  | 300a, 300c |

マージ順: 300a → 300c → 300b。300cのDWT正規化関数を300bのt1_getwmsedecが使用するため。

## 設計判断

### TCD構造体の扱い

C版のDWT・T1はTCD構造体（`opj_tcd_tilecomp_t`, `opj_tcd_cblk_enc_t`等）を引数に取る。Phase 300では**生配列ベースのAPI**を設計し、TCD統合はPhase 400で行う。

### DWTのデータ型

- 5-3 (可逆): `&mut [i32]`（整数リフティング）
- 9-7 (非可逆): `&mut [f32]`（浮動小数点リフティング）
- C版の`void*`型キャストを避け、型安全に設計
- `i32` ↔ `f32` の変換はTCD（Phase 400）の責務

### MQCの所有権

T1はMqcを所有しない。Mqc<'a>はバッファ参照を借用するため、コードブロック処理ごとにMqcインスタンスを生成する。各コードブロックは独立したバッファを持つ。

### スコープ外（延期）

| 項目                                                             | 延期先    | 理由                    |
| ---------------------------------------------------------------- | --------- | ----------------------- |
| `dwt_calc_explicit_stepsizes`                                    | Phase 500 | Tccp構造体依存          |
| `dwt_decode_partial_tile`                                        | Phase 500 | ウィンドウ復号、TCD依存 |
| TCD統合ラッパー（`dwt_encode/decode`, `t1_encode/decode_cblks`） | Phase 400 | TCD構造体依存           |
| SIMD / 64×64特化パス                                             | Phase 900 | 最適化フェーズ          |

---

## モジュール詳細

### types.rs 追加

#### 定数

```rust
// T1コンテキスト定数
pub const T1_NMSEDEC_BITS: u32 = 7;
pub const T1_NMSEDEC_FRACBITS: u32 = T1_NMSEDEC_BITS - 1;
pub const T1_NUMCTXS_ZC: usize = 9;  // Zero Context
pub const T1_NUMCTXS_SC: usize = 5;  // Sign Context
pub const T1_NUMCTXS_MAG: usize = 3; // Magnitude Context
pub const T1_NUMCTXS_AGG: usize = 1; // Aggregation Context
pub const T1_NUMCTXS_UNI: usize = 1; // Uniform Context
pub const T1_CTXNO_ZC: usize = 0;
pub const T1_CTXNO_SC: usize = 9;
pub const T1_CTXNO_MAG: usize = 14;
pub const T1_CTXNO_AGG: usize = 17;
pub const T1_CTXNO_UNI: usize = 18;
pub const T1_TYPE_MQ: u8 = 0;
pub const T1_TYPE_RAW: u8 = 1;

// T1フラグビット位置 (C: T1_SIGMA_*, T1_CHI_*, T1_MU_*, T1_PI_*)
// 32ビットワードに4行分の状態をパック
pub const T1_SIGMA_0: u32 = 1 << 0;   // ... through T1_SIGMA_17
pub const T1_CHI_0: u32 = 1 << 18;    // ... through T1_CHI_5
pub const T1_MU_0: u32 = 1 << 20;     // ...
pub const T1_PI_0: u32 = 1 << 21;     // ...
// 方向エイリアス: T1_SIGMA_NW, T1_SIGMA_N, ... T1_SIGMA_SE
// T1_SIGMA_THIS = T1_SIGMA_4, T1_CHI_THIS = T1_CHI_1 等

// コードブロックスタイルフラグ (C: J2K_CCP_CBLKSTY_*)
pub const J2K_CCP_CBLKSTY_LAZY: u32 = 0x01;
pub const J2K_CCP_CBLKSTY_RESET: u32 = 0x02;
pub const J2K_CCP_CBLKSTY_TERMALL: u32 = 0x04;
pub const J2K_CCP_CBLKSTY_VSC: u32 = 0x08;
pub const J2K_CCP_CBLKSTY_PTERM: u32 = 0x10;
pub const J2K_CCP_CBLKSTY_SEGSYM: u32 = 0x20;
```

#### 関数

```rust
/// Fixed-point multiplication for T1 NMSEDEC (C: opj_int_fix_mul_t1).
#[inline]
pub fn int_fix_mul_t1(a: i32, b: i32) -> i32;
```

---

### coding/t1_luts.rs

T1が使用する静的ルックアップテーブル。C版 `t1_luts.h` から値を転写。

| C版                | Rust                           | サイズ                        |
| ------------------ | ------------------------------ | ----------------------------- |
| `lut_ctxno_zc`     | `LUT_CTXNO_ZC: [u8; 2048]`     | Zero Context LUT              |
| `lut_ctxno_sc`     | `LUT_CTXNO_SC: [u8; 256]`      | Sign Context LUT              |
| `lut_spb`          | `LUT_SPB: [u8; 256]`           | Sign Prediction Bit LUT       |
| `lut_nmsedec_sig`  | `LUT_NMSEDEC_SIG: [i16; 128]`  | NMSEDEC significance          |
| `lut_nmsedec_sig0` | `LUT_NMSEDEC_SIG0: [i16; 128]` | NMSEDEC significance (bpno=0) |
| `lut_nmsedec_ref`  | `LUT_NMSEDEC_REF: [i16; 128]`  | NMSEDEC refinement            |
| `lut_nmsedec_ref0` | `LUT_NMSEDEC_REF0: [i16; 128]` | NMSEDEC refinement (bpno=0)   |

テスト: テーブルサイズ、代表エントリ値のスポットチェック。

---

### transform/invert.rs

LU分解による行列逆行列。カスタムMCTのパラメータ逆変換に使用。

| C版関数                  | Rust                                                                                  | 備考                  |
| ------------------------ | ------------------------------------------------------------------------------------- | --------------------- |
| `opj_matrix_inversion_f` | `pub fn matrix_inversion_f(src: &mut [f32], dst: &mut [f32], n: usize) -> Result<()>` | 特異行列はErr         |
| `opj_lupDecompose`       | `fn lup_decompose(...)` (private)                                                     | LU分解 + 部分ピボット |
| `opj_lupSolve`           | `fn lup_solve(...)` (private)                                                         | 前進・後退代入        |
| `opj_lupInvert`          | `fn lup_invert(...)` (private)                                                        | 列ごとの求解          |

テスト: 単位行列、既知2×2/3×3逆行列、A×A⁻¹≈Iラウンドトリップ、特異行列でErr。

---

### transform/mct.rs

多成分変換（MCT）。画像の色成分間の相関を除去する。

| C版関数                 | Rust                                                                              | 備考                             |
| ----------------------- | --------------------------------------------------------------------------------- | -------------------------------- |
| `opj_mct_encode`        | `pub fn mct_encode(c0: &mut [i32], c1: &mut [i32], c2: &mut [i32])`               | 可逆 RCT                         |
| `opj_mct_decode`        | `pub fn mct_decode(c0: &mut [i32], c1: &mut [i32], c2: &mut [i32])`               | 可逆 RCT逆変換                   |
| `opj_mct_encode_real`   | `pub fn mct_encode_real(c0: &mut [f32], c1: &mut [f32], c2: &mut [f32])`          | 非可逆 ICT                       |
| `opj_mct_decode_real`   | `pub fn mct_decode_real(c0: &mut [f32], c1: &mut [f32], c2: &mut [f32])`          | 非可逆 ICT逆変換                 |
| `opj_mct_getnorm`       | `pub fn mct_getnorm(compno: u32) -> f64`                                          | RCT正規化係数                    |
| `opj_mct_getnorm_real`  | `pub fn mct_getnorm_real(compno: u32) -> f64`                                     | ICT正規化係数                    |
| `opj_mct_encode_custom` | `pub fn mct_encode_custom(matrix: &[f32], data: &mut [&mut [i32]]) -> Result<()>` | カスタムMCT（可逆: i32）         |
| `opj_mct_decode_custom` | `pub fn mct_decode_custom(matrix: &[f32], data: &mut [&mut [f32]]) -> Result<()>` | カスタムMCT逆変換（非可逆: f32） |
| `opj_calculate_norms`   | `pub fn calculate_norms(norms: &mut [f64], matrix: &[f32], nb_comps: usize)`      | 列L2ノルム                       |

#### RCT計算式（可逆）

```text
符号化: Y = (R + 2G + B) >> 2, Cb = B - G, Cr = R - G
復号:   G = Y - (Cb + Cr) >> 2, R = Cr + G, B = Cb + G
```

#### 正規化係数

```rust
pub static MCT_NORMS: [f64; 3] = [1.732, 0.8292, 0.8292];      // RCT
pub static MCT_NORMS_REAL: [f64; 3] = [1.732, 1.805, 1.573];   // ICT
```

テスト: RCTラウンドトリップ（ロスレス一致）、ICTラウンドトリップ（許容誤差内）、カスタムMCT（単位行列=恒等変換）、正規化係数値検証。

---

### transform/dwt.rs

離散ウェーブレット変換（スカラのみ）。

#### 公開API

| 関数                                                                                                         | 備考          |
| ------------------------------------------------------------------------------------------------------------ | ------------- |
| `pub fn dwt_getnorm(level: u32, orient: u32) -> f64`                                                         | 5-3正規化係数 |
| `pub fn dwt_getnorm_real(level: u32, orient: u32) -> f64`                                                    | 9-7正規化係数 |
| `pub fn dwt_encode_2d_53(data: &mut [i32], w: usize, h: usize, stride: usize, num_res: usize) -> Result<()>` | 2D順方向5-3   |
| `pub fn dwt_decode_2d_53(data: &mut [i32], w: usize, h: usize, stride: usize, num_res: usize) -> Result<()>` | 2D逆方向5-3   |
| `pub fn dwt_encode_2d_97(data: &mut [f32], w: usize, h: usize, stride: usize, num_res: usize) -> Result<()>` | 2D順方向9-7   |
| `pub fn dwt_decode_2d_97(data: &mut [f32], w: usize, h: usize, stride: usize, num_res: usize) -> Result<()>` | 2D逆方向9-7   |

#### 内部関数

| 関数                                                                    | 備考             |
| ----------------------------------------------------------------------- | ---------------- |
| `fn dwt_encode_1_53(data: &mut [i32], sn: usize, dn: usize, cas: bool)` | 1D順方向5-3      |
| `fn dwt_decode_1_53(data: &mut [i32], sn: usize, dn: usize, cas: bool)` | 1D逆方向5-3      |
| `fn dwt_encode_1_97(data: &mut [f32], sn: usize, dn: usize, cas: bool)` | 1D順方向9-7      |
| `fn dwt_decode_1_97(data: &mut [f32], sn: usize, dn: usize, cas: bool)` | 1D逆方向9-7      |
| `fn deinterleave_h(...)` / `fn deinterleave_v(...)`                     | デインターリーブ |

#### リフティング係数（9-7, ITU-T T.800 Table F.4）

```rust
pub const DWT_ALPHA: f32 = -1.586134342;
pub const DWT_BETA: f32 = -0.052980118;
pub const DWT_GAMMA: f32 = 0.882911075;
pub const DWT_DELTA: f32 = 0.443506852;
pub const DWT_K: f32 = 1.230174105;
pub const DWT_INV_K: f32 = 1.0 / 1.230174105;
```

#### 正規化テーブル

`DWT_NORMS: [[f64; 10]; 4]`（5-3用）、`DWT_NORMS_REAL: [[f64; 10]; 4]`（9-7用）。orient×levelの2Dテーブル。

#### アルゴリズム

**5-3 可逆リフティング**: predict: `d[n] -= (s[n] + s[n+1]) >> 1`、update: `s[n] += (d[n-1] + d[n] + 2) >> 2`

**9-7 非可逆リフティング**: 4段（α, β, γ, δ） + スケーリング（K, 1/K）

**2D変換**: 各解像度レベルで行方向→列方向の分離可能変換。LLサブバンドに再帰適用。

テスト: 1D/2Dラウンドトリップ（5-3はロスレス、9-7は許容誤差内）、境界条件（1〜2サンプル、奇数/偶数長）、正規化テーブル値検証。

---

### coding/t1.rs

Tier-1符号化。コードブロック単位の係数をMQ算術コーダでエントロピー符号化する。

#### データ構造

```rust
/// T1ワークスペース (C: opj_t1_t)
pub struct T1 {
    data: Vec<i32>,    // 係数配列 (w * h)
    flags: Vec<u32>,   // フラグ配列 ((h/4+2) * (w+2))、1要素ボーダー付き
    w: u32,
    h: u32,
    encoder: bool,
    lut_ctxno_zc_orient_offset: usize,  // orient << 9
}

/// 符号化パス情報 (C: opj_tcd_pass_t)
pub struct TcdPass {
    pub rate: u32,
    pub distortion_decrease: f64,
    pub len: u32,
    pub term: bool,
}
```

#### 公開API

| 関数                                                                                                | 備考                                   |
| --------------------------------------------------------------------------------------------------- | -------------------------------------- |
| `T1::new(is_encoder: bool) -> T1`                                                                   | コンストラクタ                         |
| `T1::allocate_buffers(w: u32, h: u32) -> Result<()>`                                                | バッファ確保                           |
| `T1::encode_cblk_passes(buf: &mut [u8], orient: u32, bpno: i32, cblksty: u32, ...) -> Vec<TcdPass>` | コードブロック符号化（3パスループ）    |
| `T1::decode_cblk_passes(buf: &[u8], segs: &[...], orient: u32, roishift: u32, cblksty: u32)`        | コードブロック復号                     |
| `pub fn t1_getwmsedec(...) -> f64`                                                                  | 重み付きMSE減少量（DWT正規化関数使用） |

#### 内部パス関数

| 関数                                               | 備考                                   |
| -------------------------------------------------- | -------------------------------------- |
| `T1::enc_sigpass(mqc, bpno, type, cblksty) -> i32` | Significance Pass符号化（nmsedec返却） |
| `T1::dec_sigpass_mqc(mqc, bpno, cblksty)`          | Significance Pass MQ復号               |
| `T1::dec_sigpass_raw(mqc, bpno, cblksty)`          | Significance Pass RAW復号              |
| `T1::enc_refpass(mqc, bpno, type) -> i32`          | Refinement Pass符号化                  |
| `T1::dec_refpass_mqc(mqc, bpno)`                   | Refinement Pass MQ復号                 |
| `T1::dec_refpass_raw(mqc, bpno)`                   | Refinement Pass RAW復号                |
| `T1::enc_clnpass(mqc, bpno, cblksty) -> i32`       | Clean-up Pass符号化                    |
| `T1::dec_clnpass(mqc, bpno, cblksty)`              | Clean-up Pass復号                      |
| `T1::update_flags(flagsp, ci, s, stride, vsc)`     | フラグ更新                             |
| `T1::getctxno_zc(f) -> u8`                         | LUT参照                                |
| `T1::getctxno_sc(lu) -> u8`                        | LUT参照                                |
| `T1::getctxno_mag(f) -> u32`                       | インライン計算                         |
| `T1::getspb(lu) -> u8`                             | LUT参照                                |

#### 3パスアルゴリズム

各ビットプレーン（MSBからLSB）に対して:

1. **Significance Pass**: 未significant係数のうち、significant近傍を持つものを符号化。新たにsignificantになれば符号も符号化
2. **Refinement Pass**: 既significant係数の追加ビットを符号化
3. **Clean-up Pass**: 残りの係数を符号化。4サンプル集約（全未significant・近傍なしなら一括符号化）

#### フラグ配列レイアウト

32ビットワードに4行分の状態をパック:

- bits 0-17: SIGMA（significance、3×6近傍）
- bits 18-31: CHI（符号）、MU（refinement済み）、PI（significance pass済み）
- 幅+2、高さ/4+2のボーダーにより近傍アクセスが常に範囲内

テスト: バッファ確保サイズ検証、コンテキストヘルパー（既知入力→既知出力）、フラグ更新の近傍伝播、単一パスラウンドトリップ、全ビットプレーン3パスラウンドトリップ（符号化→復号で係数一致）、コードブロックスタイル（LAZY, RESET, VSC, SEGSYM）。

---

## lib.rs / mod.rs 変更

```rust
// lib.rs に追加
pub mod transform;

// coding/mod.rs に追加
pub mod t1;
pub mod t1_luts;

// transform/mod.rs (新規)
pub mod dwt;
pub mod invert;
pub mod mct;
```

## コミット計画

### PR 300a: t1_luts + invert + mct

| # | 種別  | コミットメッセージ                                       |
| - | ----- | -------------------------------------------------------- |
| 1 | RED   | `test(types): add T1 constants and int_fix_mul_t1 tests` |
| 2 | GREEN | `feat(types): add T1 constants and int_fix_mul_t1`       |
| 3 | RED   | `test(t1_luts): add lookup table verification tests`     |
| 4 | GREEN | `feat(t1_luts): add T1 lookup tables`                    |
| 5 | RED   | `test(invert): add matrix inversion tests`               |
| 6 | GREEN | `feat(invert): implement matrix inversion via LUP`       |
| 7 | RED   | `test(mct): add MCT encode/decode tests`                 |
| 8 | GREEN | `feat(mct): implement multi-component transforms`        |
| 9 | —     | `feat(transform): add transform module with mod.rs`      |

### PR 300c: dwt

| #  | 種別  | コミットメッセージ                                         |
| -- | ----- | ---------------------------------------------------------- |
| 1  | RED   | `test(dwt): add 1D 5-3 forward/inverse roundtrip tests`    |
| 2  | GREEN | `feat(dwt): implement 1D 5-3 lifting`                      |
| 3  | RED   | `test(dwt): add 1D 9-7 forward/inverse roundtrip tests`    |
| 4  | GREEN | `feat(dwt): implement 1D 9-7 lifting`                      |
| 5  | RED   | `test(dwt): add deinterleave/interleave tests`             |
| 6  | GREEN | `feat(dwt): implement deinterleave and interleave helpers` |
| 7  | RED   | `test(dwt): add 2D 5-3 forward/inverse roundtrip tests`    |
| 8  | GREEN | `feat(dwt): implement 2D 5-3 transform`                    |
| 9  | RED   | `test(dwt): add 2D 9-7 forward/inverse roundtrip tests`    |
| 10 | GREEN | `feat(dwt): implement 2D 9-7 transform`                    |
| 11 | —     | `feat(dwt): implement norm lookup functions`               |

### PR 300b: t1

| #  | 種別  | コミットメッセージ                                          |
| -- | ----- | ----------------------------------------------------------- |
| 1  | RED   | `test(t1): add T1 construction and buffer allocation tests` |
| 2  | GREEN | `feat(t1): implement T1 struct and buffer allocation`       |
| 3  | RED   | `test(t1): add context number and LUT helper tests`         |
| 4  | GREEN | `feat(t1): implement context and LUT helper functions`      |
| 5  | RED   | `test(t1): add update_flags tests`                          |
| 6  | GREEN | `feat(t1): implement update_flags`                          |
| 7  | RED   | `test(t1): add significance pass encode/decode tests`       |
| 8  | GREEN | `feat(t1): implement significance pass`                     |
| 9  | RED   | `test(t1): add refinement pass encode/decode tests`         |
| 10 | GREEN | `feat(t1): implement refinement pass`                       |
| 11 | RED   | `test(t1): add clean-up pass encode/decode tests`           |
| 12 | GREEN | `feat(t1): implement clean-up pass`                         |
| 13 | RED   | `test(t1): add multi-pass roundtrip test`                   |
| 14 | GREEN | `feat(t1): implement encode/decode cblk passes`             |
| 15 | —     | `feat(t1): implement t1_getwmsedec`                         |

## 依存関係

```text
types.rs (定数追加)
    |
    +--- coding/t1_luts.rs (純粋データ)
    |
    +--- transform/invert.rs (error.rs)
    |         |
    |         v
    +--- transform/mct.rs (invert, types::int_fix_mul_t1)
    |
    +--- transform/dwt.rs (types数学関数)
    |         |
    |         v
    +--- coding/t1.rs (coding/mqc, coding/t1_luts, types定数, transform/dwt::dwt_getnorm)
```

## 検証

各PR完了時:

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
```

## リファレンスファイル

- `reference/openjpeg/src/lib/openjp2/mct.c` — MCT実装
- `reference/openjpeg/src/lib/openjp2/invert.c` — 行列逆行列
- `reference/openjpeg/src/lib/openjp2/dwt.c` — DWT実装（スカラ部分のみ参照）
- `reference/openjpeg/src/lib/openjp2/t1.c` — T1実装
- `reference/openjpeg/src/lib/openjp2/t1_luts.h` — T1ルックアップテーブル
- `reference/openjpeg/src/lib/openjp2/t1.h` — T1フラグ定義・コンテキスト定数
