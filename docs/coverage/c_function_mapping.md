# C版→Rust版 関数対応一覧

C版 OpenJPEG（`reference/openjpeg/src/lib/openjp2/`）の各ファイル・関数が Rust版に実装されているかの対応表。

凡例:

- ✅ 実装済み
- ❌ 未実装
- 🔄 部分的に実装
- Drop: Rustの所有権システムで自動管理（明示的な実装不要）

最終更新: 2026-03-19

---

## Level 0: 基盤I/O

### bio.c → `src/io/bio.rs` — 100%

| C関数                      | Rust対応                            | 状態 | 備考                 |
| -------------------------- | ----------------------------------- | ---- | -------------------- |
| `opj_bio_create`           | `Bio::encoder()` / `Bio::decoder()` | ✅   | コンストラクタで代替 |
| `opj_bio_destroy`          | —                                   | Drop |                      |
| `opj_bio_numbytes`         | `Bio::num_bytes()`                  | ✅   |                      |
| `opj_bio_init_enc`         | `Bio::encoder()`                    | ✅   |                      |
| `opj_bio_init_dec`         | `Bio::decoder()`                    | ✅   |                      |
| `opj_bio_write`            | `Bio::write()`                      | ✅   |                      |
| `opj_bio_read`             | `Bio::read()`                       | ✅   |                      |
| `opj_bio_flush`            | `Bio::flush()`                      | ✅   |                      |
| `opj_bio_inalign`          | `Bio::inalign()`                    | ✅   |                      |
| `opj_bio_putbit` (static)  | `Bio::put_bit()`                    | ✅   | private              |
| `opj_bio_getbit` (static)  | `Bio::get_bit()`                    | ✅   | private              |
| `opj_bio_byteout` (static) | `Bio::byte_out()`                   | ✅   | private              |
| `opj_bio_bytein` (static)  | `Bio::byte_in()`                    | ✅   | private              |

### cio.c → `src/io/cio.rs` — 部分的

| C関数                             | Rust対応                     | 状態 | 備考                       |
| --------------------------------- | ---------------------------- | ---- | -------------------------- |
| `opj_write_bytes_BE`              | `write_bytes_be()`           | ✅   |                            |
| `opj_read_bytes_BE`               | `read_bytes_be()`            | ✅   |                            |
| `opj_write_double_BE`             | `write_f64_be()`             | ✅   |                            |
| `opj_read_double_BE`              | `read_f64_be()`              | ✅   |                            |
| `opj_write_float_BE`              | `write_f32_be()`             | ✅   |                            |
| `opj_read_float_BE`               | `read_f32_be()`              | ✅   |                            |
| `opj_write_bytes_LE`              | —                            | ❌   | JPEG 2000はBEが標準        |
| `opj_read_bytes_LE`               | —                            | ❌   | 同上                       |
| `opj_write_double_LE`             | —                            | ❌   | 同上                       |
| `opj_read_double_LE`              | —                            | ❌   | 同上                       |
| `opj_write_float_LE`              | —                            | ❌   | 同上                       |
| `opj_read_float_LE`               | —                            | ❌   | 同上                       |
| `opj_stream_create`               | `MemoryStream::new_output()` | ✅   |                            |
| `opj_stream_default_create`       | —                            | ❌   | コンストラクタに統合       |
| `opj_stream_destroy`              | —                            | Drop |                            |
| `opj_stream_read_data`            | `MemoryStream::read()`       | ✅   |                            |
| `opj_stream_write_data`           | `MemoryStream::write()`      | ✅   |                            |
| `opj_stream_tell`                 | `MemoryStream::tell()`       | ✅   |                            |
| `opj_stream_seek`                 | `MemoryStream::seek()`       | ✅   |                            |
| `opj_stream_get_number_byte_left` | `MemoryStream::bytes_left()` | ✅   |                            |
| `opj_stream_read_skip`            | `MemoryStream::skip()`       | ✅   |                            |
| `opj_stream_write_skip`           | `MemoryStream::skip()`       | ✅   | read/writeを統合           |
| `opj_stream_flush`                | —                            | ❌   | MemoryStreamは自動リサイズ |
| `opj_stream_set_read_function`    | —                            | ❌   | コールバック未実装         |
| `opj_stream_set_write_function`   | —                            | ❌   | 同上                       |
| `opj_stream_set_seek_function`    | —                            | ❌   | 同上                       |
| `opj_stream_set_skip_function`    | —                            | ❌   | 同上                       |
| `opj_stream_set_user_data`        | —                            | ❌   | 同上                       |
| `opj_stream_set_user_data_length` | —                            | ❌   | 同上                       |
| `opj_stream_has_seek`             | —                            | ❌   | コールバック不要           |

### event.c → `src/io/event.rs` — 部分的

| C関数                           | Rust対応                             | 状態 | 備考               |
| ------------------------------- | ------------------------------------ | ---- | ------------------ |
| `opj_set_default_event_handler` | `EventManager::new()`                | ✅   |                    |
| `opj_event_msg`                 | `EventManager::error/warning/info()` | ✅   | メソッド方式に変更 |

---

## Level 0: 符号化プリミティブ

### mqc.c → `src/coding/mqc.rs` — 100%

| C関数                            | Rust対応                        | 状態 | 備考                             |
| -------------------------------- | ------------------------------- | ---- | -------------------------------- |
| `opj_mqc_numbytes`               | `Mqc::num_bytes()`              | ✅   |                                  |
| `opj_mqc_resetstates`            | `Mqc::reset_states()`           | ✅   |                                  |
| `opj_mqc_setstate`               | `Mqc::set_state()`              | ✅   |                                  |
| `opj_mqc_setcurctx`              | `Mqc::set_curctx()`             | ✅   |                                  |
| `opj_mqc_init_enc`               | `Mqc::init_enc()`               | ✅   |                                  |
| `opj_mqc_flush`                  | `Mqc::flush()`                  | ✅   |                                  |
| `opj_mqc_bypass_init_enc`        | `Mqc::bypass_init_enc()`        | ✅   |                                  |
| `opj_mqc_bypass_get_extra_bytes` | `Mqc::bypass_get_extra_bytes()` | ✅   |                                  |
| `opj_mqc_bypass_enc`             | `Mqc::bypass_enc()`             | ✅   |                                  |
| `opj_mqc_bypass_flush_enc`       | `Mqc::bypass_flush_enc()`       | ✅   |                                  |
| `opj_mqc_reset_enc`              | `Mqc::reset_enc()`              | ✅   |                                  |
| `opj_mqc_restart_init_enc`       | `Mqc::restart_init_enc()`       | ✅   |                                  |
| `opj_mqc_erterm_enc`             | `Mqc::erterm_enc()`             | ✅   |                                  |
| `opj_mqc_segmark_enc`            | `Mqc::segmark_enc()`            | ✅   |                                  |
| `opj_mqc_init_dec`               | `Mqc::init_dec()`               | ✅   |                                  |
| `opj_mqc_raw_init_dec`           | `Mqc::raw_init_dec()`           | ✅   |                                  |
| `opj_mqc_finish_dec`             | `Mqc::finish_dec()`             | ✅   |                                  |
| `opj_mqc_restart_enc`            | —                               | ❌   | C版でも `#ifdef notdef` で無効化 |

### tgt.c → `src/coding/tgt.rs` — 100%

| C関数              | Rust対応               | 状態 | 備考           |
| ------------------ | ---------------------- | ---- | -------------- |
| `opj_tgt_create`   | `TagTree::new()`       | ✅   |                |
| `opj_tgt_init`     | —                      | ❌   | `new()` に統合 |
| `opj_tgt_destroy`  | —                      | Drop |                |
| `opj_tgt_reset`    | `TagTree::reset()`     | ✅   |                |
| `opj_tgt_setvalue` | `TagTree::set_value()` | ✅   |                |
| `opj_tgt_encode`   | `TagTree::encode()`    | ✅   |                |
| `opj_tgt_decode`   | `TagTree::decode()`    | ✅   |                |

### sparse_array.c → `src/coding/sparse_array.rs` — 100%

| C関数                              | Rust対応                         | 状態 | 備考 |
| ---------------------------------- | -------------------------------- | ---- | ---- |
| `opj_sparse_array_int32_create`    | `SparseArray::new()`             | ✅   |      |
| `opj_sparse_array_int32_free`      | —                                | Drop |      |
| `opj_sparse_array_is_region_valid` | `SparseArray::is_region_valid()` | ✅   |      |
| `opj_sparse_array_int32_read`      | `SparseArray::read_region()`     | ✅   |      |
| `opj_sparse_array_int32_write`     | `SparseArray::write_region()`    | ✅   |      |

---

## Level 1: 変換・符号化レイヤー

### mct.c → `src/transform/mct.rs` + `mct_simd.rs` — 100%

| C関数                        | Rust対応                | 状態 | 備考            |
| ---------------------------- | ----------------------- | ---- | --------------- |
| `opj_mct_encode`             | `mct_encode()`          | ✅   | +AVX2/SSE2 SIMD |
| `opj_mct_decode`             | `mct_decode()`          | ✅   | +AVX2/SSE2 SIMD |
| `opj_mct_getnorm`            | `mct_getnorm()`         | ✅   |                 |
| `opj_mct_encode_real`        | `mct_encode_real()`     | ✅   | +AVX/SSE SIMD   |
| `opj_mct_decode_real`        | `mct_decode_real()`     | ✅   | +AVX/SSE SIMD   |
| `opj_mct_getnorm_real`       | `mct_getnorm_real()`    | ✅   |                 |
| `opj_mct_encode_custom`      | `mct_encode_custom()`   | ✅   |                 |
| `opj_mct_decode_custom`      | `mct_decode_custom()`   | ✅   |                 |
| `opj_calculate_norms`        | `calculate_norms()`     | ✅   |                 |
| `opj_mct_get_mct_norms`      | `MCT_NORMS` (定数)      | ✅   |                 |
| `opj_mct_get_mct_norms_real` | `MCT_NORMS_REAL` (定数) | ✅   |                 |

SIMD追加関数（`mct_simd.rs`）: `mct_encode_avx2`, `mct_encode_sse2`, `mct_decode_avx2`, `mct_decode_sse2`, `mct_encode_real_avx`, `mct_encode_real_sse`, `mct_decode_real_avx`, `mct_decode_real_sse`

### invert.c → `src/transform/invert.rs` — 100%

| C関数                       | Rust対応               | 状態 | 備考    |
| --------------------------- | ---------------------- | ---- | ------- |
| `opj_matrix_inversion_f`    | `matrix_inversion_f()` | ✅   |         |
| `opj_lupDecompose` (static) | `lup_decompose()`      | ✅   | private |
| `opj_lupSolve` (static)     | `lup_solve()`          | ✅   | private |
| `opj_lupInvert` (static)    | `lup_invert()`         | ✅   | private |

### dwt.c → `src/transform/dwt.rs` + `dwt_simd.rs` — 67%

| C関数                             | Rust対応             | 状態 | 備考                     |
| --------------------------------- | -------------------- | ---- | ------------------------ |
| `opj_dwt_encode`                  | `dwt_encode_2d_53()` | ✅   | +AVX2/SSE2 SIMD垂直パス  |
| `opj_dwt_decode`                  | `dwt_decode_2d_53()` | ✅   | +AVX2/SSE2 SIMD垂直パス  |
| `opj_dwt_getnorm`                 | `dwt_getnorm()`      | ✅   |                          |
| `opj_dwt_encode_real`             | `dwt_encode_2d_97()` | ✅   |                          |
| `opj_dwt_decode_real`             | `dwt_decode_2d_97()` | ✅   |                          |
| `opj_dwt_getnorm_real`            | `dwt_getnorm_real()` | ✅   |                          |
| `opj_dwt_calc_explicit_stepsizes` | —                    | ❌   | 量子化ステップサイズ計算 |
| `opj_dwt_decode_partial_97`       | —                    | ❌   | 部分復号（ROI用）        |
| `opj_dwt_decode_tile_97`          | —                    | ❌   | タイルレベル抽象         |

Rust独自の1D関数: `dwt_encode_1_53`, `dwt_decode_1_53`, `dwt_encode_1_97`, `dwt_decode_1_97`
Rust独自のインターリーブ関数: `deinterleave_h`, `interleave_h`, `deinterleave_v`, `interleave_v`
SIMD追加関数（`dwt_simd.rs`）: `dwt_encode_vert_53_avx2`, `dwt_encode_vert_53_sse2`, `dwt_decode_vert_53_avx2`, `dwt_decode_vert_53_sse2`

### t1.c → `src/coding/t1.rs` — 85%

| C関数                                | Rust対応                     | 状態 | 備考                   |
| ------------------------------------ | ---------------------------- | ---- | ---------------------- |
| `opj_t1_create`                      | `T1::new()`                  | ✅   |                        |
| `opj_t1_destroy`                     | —                            | Drop |                        |
| `opj_t1_encode_cblks`                | `t1_encode_cblks()`          | ✅   |                        |
| `opj_t1_decode_cblks`                | `t1_decode_cblks()`          | ✅   |                        |
| `opj_t1_ht_decode_cblk`              | `T1::decode_cblk_ht()`       | ✅   |                        |
| `opj_t1_allocate_buffers`            | `T1::allocate_buffers()`     | ✅   |                        |
| `opj_t1_enc_sigpass`                 | `T1::enc_sigpass()`          | ✅   |                        |
| `opj_t1_enc_refpass`                 | `T1::enc_refpass()`          | ✅   |                        |
| `opj_t1_enc_clnpass`                 | `T1::enc_clnpass()`          | ✅   |                        |
| `opj_t1_dec_sigpass_mqc`             | `T1::dec_sigpass_mqc()`      | ✅   |                        |
| `opj_t1_dec_sigpass_raw`             | `T1::dec_sigpass_raw()`      | ✅   |                        |
| `opj_t1_dec_refpass_mqc`             | `T1::dec_refpass_mqc()`      | ✅   |                        |
| `opj_t1_dec_refpass_raw`             | `T1::dec_refpass_raw()`      | ✅   |                        |
| `opj_t1_dec_clnpass`                 | `T1::dec_clnpass()`          | ✅   |                        |
| `opj_t1_encode_cblk`                 | `T1::encode_cblk()`          | ✅   |                        |
| `opj_t1_decode_cblk`                 | `T1::decode_cblk()`          | ✅   |                        |
| `opj_t1_enc_sigpass_step`            | `T1::enc_sigpass_step()`     | ✅   | private                |
| `opj_t1_enc_refpass_step`            | `T1::enc_refpass_step()`     | ✅   | private                |
| `opj_t1_enc_clnpass_step`            | `T1::enc_clnpass_step()`     | ✅   | private                |
| `opj_t1_dec_sigpass_step_mqc`        | `T1::dec_sigpass_step_mqc()` | ✅   | private                |
| `opj_t1_dec_sigpass_step_raw`        | `T1::dec_sigpass_step_raw()` | ✅   | private                |
| `opj_t1_dec_refpass_step_mqc`        | `T1::dec_refpass_step_mqc()` | ✅   | private                |
| `opj_t1_dec_refpass_step_raw`        | `T1::dec_refpass_step_raw()` | ✅   | private                |
| `opj_t1_dec_clnpass_step`            | `T1::dec_clnpass_step()`     | ✅   | private                |
| `opj_t1_enc_is_term_pass`            | `T1::is_term_pass()`         | ✅   | private                |
| `opj_t1_getctxno_zc`                 | `getctxno_zc()`              | ✅   |                        |
| `opj_t1_getctxno_sc`                 | `getctxno_sc()`              | ✅   |                        |
| `opj_t1_getctxno_mag`                | `getctxno_mag()`             | ✅   |                        |
| `opj_t1_getspb`                      | `getspb()`                   | ✅   |                        |
| `opj_t1_getnmsedec_sig`              | `getnmsedec_sig()`           | ✅   |                        |
| `opj_t1_getnmsedec_ref`              | `getnmsedec_ref()`           | ✅   |                        |
| `opj_t1_getwmsedec`                  | `t1_getwmsedec()`            | ✅   |                        |
| `opj_t1_update_flags`                | `update_flags()`             | ✅   |                        |
| `opj_t1_dec_sigpass_mqc_64x64_novsc` | —                            | ❌   | 64x64 SIMD最適化       |
| `opj_t1_dec_sigpass_mqc_64x64_vsc`   | —                            | ❌   | 64x64 SIMD最適化       |
| `opj_t1_dec_clnpass_64x64_novsc`     | —                            | ❌   | 64x64 SIMD最適化       |
| `opj_t1_dec_clnpass_64x64_vsc`       | —                            | ❌   | 64x64 SIMD最適化       |
| `opj_t1_dec_clnpass_check_segsym`    | —                            | ❌   | セグメントシンボル検証 |

### t1_luts.h → `src/coding/t1_luts.rs` — 100%

| Cテーブル          | Rustテーブル       | 状態 |
| ------------------ | ------------------ | ---- |
| `lut_ctxno_zc`     | `LUT_CTXNO_ZC`     | ✅   |
| `lut_ctxno_sc`     | `LUT_CTXNO_SC`     | ✅   |
| `lut_spb`          | `LUT_SPB`          | ✅   |
| `lut_nmsedec_sig`  | `LUT_NMSEDEC_SIG`  | ✅   |
| `lut_nmsedec_sig0` | `LUT_NMSEDEC_SIG0` | ✅   |
| `lut_nmsedec_ref`  | `LUT_NMSEDEC_REF`  | ✅   |
| `lut_nmsedec_ref0` | `LUT_NMSEDEC_REF0` | ✅   |

### ht_dec.c → `src/coding/ht_dec.rs` — 100%

| C関数                   | Rust対応           | 状態 | 備考 |
| ----------------------- | ------------------ | ---- | ---- |
| `opj_t1_ht_decode_cblk` | `ht_decode_cblk()` | ✅   |      |

Rust独自構造: `MelDecoder`, `RevReader`, `FrwdReader`

---

## Level 2: パケット管理

### pi.c → `src/tier2/pi.rs` — 部分的

| C関数                               | Rust対応                      | 状態 | 備考             |
| ----------------------------------- | ----------------------------- | ---- | ---------------- |
| `opj_pi_next`                       | `pi_next()`                   | ✅   |                  |
| `opj_get_encoding_packet_count`     | `get_encoding_packet_count()` | ✅   |                  |
| `opj_pi_initialise_encode`          | —                             | ❌   | エンコーダ初期化 |
| `opj_pi_update_encoding_parameters` | —                             | ❌   | パラメータ更新   |
| `opj_pi_create_encode`              | —                             | ❌   | エンコーダ作成   |
| `opj_pi_create_decode`              | —                             | ❌   | デコーダ作成     |
| `opj_pi_destroy`                    | —                             | Drop |                  |

5種のプログレッション順序は全て実装済み: `pi_next_lrcp`, `pi_next_rlcp`, `pi_next_rpcl`, `pi_next_pcrl`, `pi_next_cprl`

### t2.c → `src/tier2/t2.rs` — ユーティリティのみ

| C関数                          | Rust対応            | 状態 | 備考                 |
| ------------------------------ | ------------------- | ---- | -------------------- |
| `opj_t2_encode_packets`        | —                   | ❌   | **コアencode未実装** |
| `opj_t2_decode_packets`        | —                   | ❌   | **コアdecode未実装** |
| `opj_t2_create`                | —                   | ❌   |                      |
| `opj_t2_destroy`               | —                   | Drop |                      |
| `opj_t2_putcommacode` (helper) | `t2_putcommacode()` | ✅   |                      |
| `opj_t2_getcommacode` (helper) | `t2_getcommacode()` | ✅   |                      |
| `opj_t2_putnumpasses` (helper) | `t2_putnumpasses()` | ✅   |                      |
| `opj_t2_getnumpasses` (helper) | `t2_getnumpasses()` | ✅   |                      |
| `opj_t2_init_seg` (helper)     | `t2_init_seg()`     | ✅   |                      |
| `opj_t2_getpassbits` (helper)  | `t2_getpassbits()`  | ✅   |                      |

---

## Level 3: タイル処理

### tcd.c → `src/tcd.rs` — 初期段階

| C関数                                   | Rust対応                       | 状態 | 備考                         |
| --------------------------------------- | ------------------------------ | ---- | ---------------------------- |
| `opj_tcd_create`                        | `Tcd::new()`                   | ✅   |                              |
| `opj_tcd_destroy`                       | —                              | Drop |                              |
| `opj_tcd_init`                          | `Tcd::init_tile()`             | ✅   |                              |
| `opj_tcd_is_band_empty`                 | `TcdBand::is_empty()`          | ✅   |                              |
| `opj_tcd_dc_level_shift_encode`         | `Tcd::dc_level_shift_encode()` | ✅   |                              |
| `opj_tcd_dc_level_shift_decode`         | `Tcd::dc_level_shift_decode()` | ✅   |                              |
| `opj_tcd_init_decode_tile`              | —                              | ❌   |                              |
| `opj_tcd_init_encode_tile`              | —                              | ❌   |                              |
| `opj_tcd_encode_tile`                   | —                              | ❌   | **タイル符号化パイプライン** |
| `opj_tcd_decode_tile`                   | —                              | ❌   | **タイル復号パイプライン**   |
| `opj_tcd_update_tile_data`              | —                              | ❌   |                              |
| `opj_tcd_get_decoded_tile_size`         | —                              | ❌   |                              |
| `opj_tcd_get_encoder_input_buffer_size` | —                              | ❌   |                              |
| `opj_tcd_copy_tile_data`                | —                              | ❌   |                              |
| `opj_tcd_rateallocate`                  | —                              | ❌   | **レート割り当て**           |
| `opj_tcd_rateallocate_fixed`            | —                              | ❌   |                              |
| `opj_tcd_makelayer`                     | —                              | ❌   | **レイヤー構築**             |
| `opj_tcd_makelayer_fixed`               | —                              | ❌   |                              |
| `opj_tcd_reinit_segment`                | —                              | ❌   |                              |
| `opj_tcd_is_subband_area_of_interest`   | —                              | ❌   | ROI用                        |
| `opj_tcd_marker_info_create`            | —                              | ❌   |                              |

---

## Level 4: J2Kコードストリーム

### j2k.c → `src/j2k/` — マーカー読み書き実装済み

#### マーカー読み込み（`markers.rs`）

| C関数              | Rust対応     | 状態 |
| ------------------ | ------------ | ---- |
| `opj_j2k_read_siz` | `read_siz()` | ✅   |
| `opj_j2k_read_cod` | `read_cod()` | ✅   |
| `opj_j2k_read_coc` | `read_coc()` | ✅   |
| `opj_j2k_read_qcd` | `read_qcd()` | ✅   |
| `opj_j2k_read_qcc` | `read_qcc()` | ✅   |
| `opj_j2k_read_poc` | `read_poc()` | ✅   |
| `opj_j2k_read_rgn` | `read_rgn()` | ✅   |
| `opj_j2k_read_com` | `read_com()` | ✅   |
| `opj_j2k_read_sot` | `read_sot()` | ✅   |

#### マーカー書き込み（`markers.rs`）

| C関数               | Rust対応      | 状態 | 備考                     |
| ------------------- | ------------- | ---- | ------------------------ |
| `opj_j2k_write_soc` | `write_soc()` | ✅   |                          |
| `opj_j2k_write_siz` | `write_siz()` | ✅   |                          |
| `opj_j2k_write_cod` | `write_cod()` | ✅   |                          |
| `opj_j2k_write_qcd` | `write_qcd()` | ✅   |                          |
| `opj_j2k_write_sot` | `write_sot()` | ✅   |                          |
| `opj_j2k_write_sod` | `write_sod()` | ✅   |                          |
| `opj_j2k_write_eoc` | `write_eoc()` | ✅   |                          |
| `opj_j2k_write_coc` | —             | ❌   | 成分別COD                |
| `opj_j2k_write_qcc` | —             | ❌   | 成分別QCD                |
| `opj_j2k_write_poc` | —             | ❌   | プログレッション順序変更 |
| `opj_j2k_write_rgn` | —             | ❌   | ROI                      |
| `opj_j2k_write_com` | —             | ❌   | コメント                 |

#### 高レベルデコーダ/エンコーダ API（`read.rs` / `write.rs`）

| C関数                                   | Rust対応                       | 状態 | 備考                         |
| --------------------------------------- | ------------------------------ | ---- | ---------------------------- |
| `opj_j2k_read_header`                   | `J2kDecoder::read_header()`    | ✅   |                              |
| `opj_j2k_read_tile_header`              | `J2kDecoder::read_tile_part()` | 🔄   | 部分的                       |
| `opj_j2k_decode`                        | —                              | ❌   | **完全デコードパイプライン** |
| `opj_j2k_decode_tile`                   | —                              | ❌   | タイル個別デコード           |
| `opj_j2k_end_decompress`                | —                              | ❌   |                              |
| `opj_j2k_set_decode_area`               | —                              | ❌   | 領域指定デコード             |
| `opj_j2k_set_decoded_components`        | —                              | ❌   | 成分指定デコード             |
| `opj_j2k_set_decoded_resolution_factor` | —                              | ❌   | 解像度削減                   |
| `opj_j2k_setup_decoder`                 | —                              | ❌   |                              |
| `opj_j2k_setup_encoder`                 | —                              | ❌   |                              |
| `opj_j2k_create_compress`               | —                              | ❌   |                              |
| `opj_j2k_create_decompress`             | —                              | ❌   |                              |
| `opj_j2k_encoder_set_extra_options`     | —                              | ❌   |                              |
| `opj_j2k_set_threads`                   | —                              | ❌   |                              |
| `j2k_dump`                              | —                              | ❌   | デバッグ出力                 |
| `j2k_get_cstr_info`                     | —                              | ❌   |                              |
| `j2k_get_cstr_index`                    | —                              | ❌   |                              |

---

## Level 5: JP2ファイルフォーマット

### jp2.c → `src/jp2/` — ヘッダ読み書き実装済み

| C関数                                   | Rust対応                                  | 状態 | 備考                 |
| --------------------------------------- | ----------------------------------------- | ---- | -------------------- |
| `opj_jp2_read_header`                   | `Jp2Decoder::read_header()`               | ✅   |                      |
| `opj_jp2_decode`                        | `Jp2Decoder::read_codestream()`           | ✅   |                      |
| `opj_jp2_encode`                        | `Jp2Encoder::write_codestream()`          | ✅   |                      |
| `opj_jp2_start_compress`                | `Jp2Encoder::write_header()`              | ✅   |                      |
| `opj_jp2_end_compress`                  | `Jp2Encoder::finalize()`                  | ✅   |                      |
| `opj_jp2_create`                        | `Jp2Decoder::new()` / `Jp2Encoder::new()` | ✅   | 分割                 |
| `opj_jp2_destroy`                       | —                                         | Drop |                      |
| `opj_jp2_setup_decoder`                 | `Jp2Decoder::new()`                       | 🔄   | コンストラクタに統合 |
| `opj_jp2_setup_encoder`                 | `Jp2Encoder::new()`                       | 🔄   | コンストラクタに統合 |
| `opj_jp2_decode_tile`                   | —                                         | ❌   | タイル個別デコード   |
| `opj_jp2_read_tile_header`              | —                                         | ❌   |                      |
| `opj_jp2_set_decode_area`               | —                                         | ❌   | 領域指定デコード     |
| `opj_jp2_set_decoded_components`        | —                                         | ❌   | 成分指定             |
| `opj_jp2_set_decoded_resolution_factor` | —                                         | ❌   | 解像度削減           |
| `opj_jp2_set_threads`                   | —                                         | ❌   |                      |
| `opj_jp2_decoder_set_strict_mode`       | —                                         | ❌   |                      |
| `opj_jp2_encoder_set_extra_options`     | —                                         | ❌   |                      |
| `opj_jp2_get_tile`                      | —                                         | ❌   |                      |
| `opj_jp2_write_tile`                    | —                                         | ❌   |                      |
| `opj_jp2_end_decompress`                | —                                         | ❌   |                      |

---

## Level 6: 公開API

### openjpeg.c → `src/api.rs` — 高レベルAPI

| C関数                                | Rust対応                      | 状態 | 備考                   |
| ------------------------------------ | ----------------------------- | ---- | ---------------------- |
| `opj_decode`                         | `decode()` / `decode_owned()` | ✅   |                        |
| `opj_encode`                         | `encode()`                    | ✅   |                        |
| `opj_image_create`                   | `Image::new()`                | ✅   |                        |
| `opj_image_tile_create`              | `Image::new_tile()`           | ✅   |                        |
| `opj_version`                        | —                             | ❌   |                        |
| `opj_create_decompress`              | —                             | ❌   | Rust APIは構造が異なる |
| `opj_create_compress`                | —                             | ❌   | 同上                   |
| `opj_destroy_codec`                  | —                             | Drop |                        |
| `opj_setup_decoder`                  | —                             | ❌   | decode()に統合         |
| `opj_setup_encoder`                  | —                             | ❌   | encode()に統合         |
| `opj_set_default_decoder_parameters` | —                             | ❌   | Default traitで代替可  |
| `opj_set_default_encoder_parameters` | —                             | ❌   | 同上                   |
| `opj_read_header`                    | 内部で使用                    | 🔄   |                        |
| `opj_end_decompress`                 | —                             | ❌   | decode()に統合         |
| `opj_end_compress`                   | —                             | ❌   | encode()に統合         |
| `opj_read_tile_header`               | —                             | ❌   | タイルAPI未公開        |
| `opj_decode_tile_data`               | —                             | ❌   | 同上                   |
| `opj_write_tile`                     | —                             | ❌   | 同上                   |
| `opj_get_decoded_tile`               | —                             | ❌   | 同上                   |
| `opj_set_decode_area`                | —                             | ❌   | 領域デコード未実装     |
| `opj_set_decoded_components`         | —                             | ❌   | 成分指定未実装         |
| `opj_set_decoded_resolution_factor`  | —                             | ❌   | 解像度削減未実装       |
| `opj_decoder_set_strict_mode`        | —                             | ❌   |                        |
| `opj_set_info_handler`               | —                             | ❌   | イベントコールバック   |
| `opj_set_warning_handler`            | —                             | ❌   | 同上                   |
| `opj_set_error_handler`              | —                             | ❌   | 同上                   |
| `opj_stream_*` (12関数)              | —                             | ❌   | MemoryStream内部化     |
| `opj_codec_set_threads`              | —                             | ❌   |                        |
| `opj_dump_codec`                     | —                             | ❌   |                        |
| `opj_get_cstr_info`                  | —                             | ❌   |                        |
| `opj_get_cstr_index`                 | —                             | ❌   |                        |

### image.c → `src/image.rs`

| C関数                          | Rust対応                | 状態 | 備考               |
| ------------------------------ | ----------------------- | ---- | ------------------ |
| `opj_image_create`             | `Image::new()`          | ✅   |                    |
| `opj_image_tile_create`        | `Image::new_tile()`     | ✅   |                    |
| `opj_copy_image_header`        | `Image::clone_header()` | ✅   |                    |
| `opj_image_create0`            | —                       | ❌   | `new_tile()`で代替 |
| `opj_image_comp_header_update` | —                       | ❌   |                    |

---

## 不要なCファイル（Rustの標準機能で代替）

| Cファイル           | 理由                     |
| ------------------- | ------------------------ |
| `opj_malloc.c/h`    | Rustの所有権・メモリ管理 |
| `opj_clock.c/h`     | `std::time`              |
| `function_list.c/h` | `match`式・クロージャ    |
| `thread.c/h`        | `rayon` クレート         |
| `cidx_manager.c`    | JPIP（スコープ外）       |
| `phix_manager.c`    | JPIP（スコープ外）       |
| `ppix_manager.c`    | JPIP（スコープ外）       |
| `thix_manager.c`    | JPIP（スコープ外）       |
| `tpix_manager.c`    | JPIP（スコープ外）       |

---

## 総合サマリー

| Cファイル      | 実装済み | 合計 | カバー率 | 備考                        |
| -------------- | -------- | ---- | -------- | --------------------------- |
| bio.c          | 13       | 13   | **100%** | 完全                        |
| mqc.c          | 17       | 17   | **100%** | 完全                        |
| tgt.c          | 6        | 6    | **100%** | 完全                        |
| sparse_array.c | 5        | 5    | **100%** | 完全                        |
| mct.c          | 11       | 11   | **100%** | 完全 + SIMD                 |
| invert.c       | 4        | 4    | **100%** | 完全                        |
| ht_dec.c       | 1        | 1    | **100%** | 完全                        |
| t1_luts.h      | 7        | 7    | **100%** | 全テーブル                  |
| image.c        | 3        | 5    | 60%      |                             |
| dwt.c          | 6        | 9    | 67%      | 部分復号・量子化未実装      |
| t1.c           | 28       | 33   | 85%      | 64x64 SIMD・segsym未実装    |
| event.c        | 2        | 2    | 100%     | メソッド方式                |
| cio.c          | 12       | 30   | 40%      | LE・コールバック未実装      |
| pi.c           | 7        | 7    | 100%     | 5プログレッション実装済み   |
| t2.c           | 6        | 10   | 60%      | **コアencode/decode未実装** |
| tcd.c          | 5        | 21   | 24%      | **パイプライン未実装**      |
| j2k.c          | 18       | 35+  | ~50%     | マーカー読み書き済み        |
| jp2.c          | 8        | 20   | 40%      | ヘッダ読み書き済み          |
| openjpeg.c     | 4        | 52   | 8%       | Rust APIは構造が異なる      |

### 主な未実装領域

1. **T2パケット符号化/復号** — `t2_encode_packets` / `t2_decode_packets`
2. **TCDタイル処理パイプライン** — `tcd_encode_tile` / `tcd_decode_tile` / レート割り当て
3. **J2K完全デコードパイプライン** — `j2k_decode` / タイルデコード / 領域デコード
4. **公開APIの多くの機能** — ストリームコールバック、コーデック設定、ダンプ機能
5. **CIOリトルエンディアン関数** — JPEG 2000標準はBEだが一部拡張で使用
6. **T1 64x64 SIMD最適化パス** — ブロック単位の最適化
7. **DWT部分復号** — ROI（Region of Interest）用
