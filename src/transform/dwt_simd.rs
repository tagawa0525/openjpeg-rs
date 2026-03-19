// DWT SIMD acceleration: vertical multi-column processing.
//
// Instead of processing one column at a time (gather → 1D DWT → scatter),
// processes 8 columns (AVX2) or 4 columns (SSE2) simultaneously by loading
// contiguous row segments and applying lifting steps across SIMD lanes.
//
// This improves both instruction-level parallelism and cache locality since
// adjacent columns in a row are contiguous in memory.

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod inner {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    // -----------------------------------------------------------------------
    // 5-3 vertical lifting (i32) — multi-column SSE2/AVX2
    // -----------------------------------------------------------------------
    //
    // Forward 5-3 lifting on interleaved vertical data for W columns at once.
    // Equivalent to calling dwt_encode_1_53 on each column independently,
    // but processes W columns in parallel using SIMD registers.
    //
    // The vertical data for each column is interleaved: [s0, d0, s1, d1, ...]
    // stored across rows of the tile. We load row segments of W i32 values,
    // apply predict and update lifting steps, then deinterleave and store back.

    /// Forward 5-3 vertical pass for 8 columns using AVX2.
    ///
    /// Processes columns `col_base..col_base+8` of the tile.
    /// `data` is row-major with `stride` elements per row.
    /// `rh` rows, `sn` low-pass count, `dn` high-pass count.
    ///
    /// # Safety
    ///
    /// Caller must ensure AVX2 is available and `col_base + 8 <= stride`.
    #[target_feature(enable = "avx2")]
    pub unsafe fn dwt_encode_vert_53_avx2(
        data: &mut [i32],
        stride: usize,
        rh: usize,
        sn: usize,
        dn: usize,
        col_base: usize,
    ) {
        if sn + dn <= 1 || (dn > 0 && sn == 0) {
            return;
        }

        unsafe {
            let two = _mm256_set1_epi32(2);

            // Predict: d[i] -= (s[i] + s[i+1]) >> 1
            for i in 0..dn {
                let si_row = 2 * i;
                let si1_row = 2 * (i + 1).min(sn - 1);
                let di_row = 2 * i + 1;

                let si = _mm256_loadu_si256(
                    data[si_row * stride + col_base..].as_ptr() as *const __m256i
                );
                let si1 = _mm256_loadu_si256(
                    data[si1_row * stride + col_base..].as_ptr() as *const __m256i
                );
                let d = _mm256_loadu_si256(
                    data[di_row * stride + col_base..].as_ptr() as *const __m256i
                );

                let pred = _mm256_srai_epi32(_mm256_add_epi32(si, si1), 1);
                let d_new = _mm256_sub_epi32(d, pred);

                _mm256_storeu_si256(
                    data[di_row * stride + col_base..].as_mut_ptr() as *mut __m256i,
                    d_new,
                );
            }

            // Update: s[i] += (d[i-1] + d[i] + 2) >> 2
            for i in 0..sn {
                let si_row = 2 * i;
                let dim1_row = 2 * (if i > 0 { i - 1 } else { 0 }) + 1;
                let di_row = if dn > 0 { 2 * i.min(dn - 1) + 1 } else { 0 };

                let s = _mm256_loadu_si256(
                    data[si_row * stride + col_base..].as_ptr() as *const __m256i
                );
                let dim1 = _mm256_loadu_si256(
                    data[dim1_row * stride + col_base..].as_ptr() as *const __m256i
                );
                let di = if dn > 0 {
                    _mm256_loadu_si256(data[di_row * stride + col_base..].as_ptr() as *const __m256i)
                } else {
                    _mm256_setzero_si256()
                };

                let upd = _mm256_srai_epi32(_mm256_add_epi32(_mm256_add_epi32(dim1, di), two), 2);
                let s_new = _mm256_add_epi32(s, upd);

                _mm256_storeu_si256(
                    data[si_row * stride + col_base..].as_mut_ptr() as *mut __m256i,
                    s_new,
                );
            }

            // Deinterleave: move even rows (s) to top, odd rows (d) to bottom.
            // We need a temporary buffer for the rearrangement.
            // Use a stack buffer for up to 64 rows, heap-allocate for larger.
            let mut buf = vec![[0i32; 8]; rh];

            // Read all rows
            for row in 0..rh {
                _mm256_storeu_si256(
                    buf[row].as_mut_ptr() as *mut __m256i,
                    _mm256_loadu_si256(data[row * stride + col_base..].as_ptr() as *const __m256i),
                );
            }

            // Write back deinterleaved: s0, s1, ..., s_{sn-1}, d0, d1, ..., d_{dn-1}
            for i in 0..sn {
                _mm256_storeu_si256(
                    data[i * stride + col_base..].as_mut_ptr() as *mut __m256i,
                    _mm256_loadu_si256(buf[2 * i].as_ptr() as *const __m256i),
                );
            }
            for i in 0..dn {
                _mm256_storeu_si256(
                    data[(sn + i) * stride + col_base..].as_mut_ptr() as *mut __m256i,
                    _mm256_loadu_si256(buf[2 * i + 1].as_ptr() as *const __m256i),
                );
            }
        }
    }

    /// Forward 5-3 vertical pass for 4 columns using SSE2.
    ///
    /// # Safety
    ///
    /// Caller must ensure SSE2 is available and `col_base + 4 <= stride`.
    #[target_feature(enable = "sse2")]
    pub unsafe fn dwt_encode_vert_53_sse2(
        data: &mut [i32],
        stride: usize,
        rh: usize,
        sn: usize,
        dn: usize,
        col_base: usize,
    ) {
        if sn + dn <= 1 || (dn > 0 && sn == 0) {
            return;
        }

        unsafe {
            let two = _mm_set1_epi32(2);

            // Predict
            for i in 0..dn {
                let si_row = 2 * i;
                let si1_row = 2 * (i + 1).min(sn - 1);
                let di_row = 2 * i + 1;

                let si =
                    _mm_loadu_si128(data[si_row * stride + col_base..].as_ptr() as *const __m128i);
                let si1 =
                    _mm_loadu_si128(data[si1_row * stride + col_base..].as_ptr() as *const __m128i);
                let d =
                    _mm_loadu_si128(data[di_row * stride + col_base..].as_ptr() as *const __m128i);

                let pred = _mm_srai_epi32(_mm_add_epi32(si, si1), 1);
                let d_new = _mm_sub_epi32(d, pred);

                _mm_storeu_si128(
                    data[di_row * stride + col_base..].as_mut_ptr() as *mut __m128i,
                    d_new,
                );
            }

            // Update
            for i in 0..sn {
                let si_row = 2 * i;
                let dim1_row = 2 * (if i > 0 { i - 1 } else { 0 }) + 1;
                let di_row = if dn > 0 { 2 * i.min(dn - 1) + 1 } else { 0 };

                let s =
                    _mm_loadu_si128(data[si_row * stride + col_base..].as_ptr() as *const __m128i);
                let dim1 = _mm_loadu_si128(
                    data[dim1_row * stride + col_base..].as_ptr() as *const __m128i
                );
                let di = if dn > 0 {
                    _mm_loadu_si128(data[di_row * stride + col_base..].as_ptr() as *const __m128i)
                } else {
                    _mm_setzero_si128()
                };

                let upd = _mm_srai_epi32(_mm_add_epi32(_mm_add_epi32(dim1, di), two), 2);
                let s_new = _mm_add_epi32(s, upd);

                _mm_storeu_si128(
                    data[si_row * stride + col_base..].as_mut_ptr() as *mut __m128i,
                    s_new,
                );
            }

            // Deinterleave
            let mut buf = vec![[0i32; 4]; rh];
            for row in 0..rh {
                _mm_storeu_si128(
                    buf[row].as_mut_ptr() as *mut __m128i,
                    _mm_loadu_si128(data[row * stride + col_base..].as_ptr() as *const __m128i),
                );
            }
            for i in 0..sn {
                _mm_storeu_si128(
                    data[i * stride + col_base..].as_mut_ptr() as *mut __m128i,
                    _mm_loadu_si128(buf[2 * i].as_ptr() as *const __m128i),
                );
            }
            for i in 0..dn {
                _mm_storeu_si128(
                    data[(sn + i) * stride + col_base..].as_mut_ptr() as *mut __m128i,
                    _mm_loadu_si128(buf[2 * i + 1].as_ptr() as *const __m128i),
                );
            }
        }
    }

    /// Inverse 5-3 vertical pass for 8 columns using AVX2.
    ///
    /// # Safety
    ///
    /// Caller must ensure AVX2 is available and `col_base + 8 <= stride`.
    #[target_feature(enable = "avx2")]
    pub unsafe fn dwt_decode_vert_53_avx2(
        data: &mut [i32],
        stride: usize,
        rh: usize,
        sn: usize,
        dn: usize,
        col_base: usize,
    ) {
        if sn + dn <= 1 {
            return;
        }

        unsafe {
            let two = _mm256_set1_epi32(2);

            // Interleave: s0..s_{sn-1}, d0..d_{dn-1} → [s0, d0, s1, d1, ...]
            let mut buf = vec![[0i32; 8]; rh];
            for i in 0..sn {
                _mm256_storeu_si256(
                    buf[2 * i].as_mut_ptr() as *mut __m256i,
                    _mm256_loadu_si256(data[i * stride + col_base..].as_ptr() as *const __m256i),
                );
            }
            for i in 0..dn {
                _mm256_storeu_si256(
                    buf[2 * i + 1].as_mut_ptr() as *mut __m256i,
                    _mm256_loadu_si256(
                        data[(sn + i) * stride + col_base..].as_ptr() as *const __m256i
                    ),
                );
            }
            for row in 0..rh {
                _mm256_storeu_si256(
                    data[row * stride + col_base..].as_mut_ptr() as *mut __m256i,
                    _mm256_loadu_si256(buf[row].as_ptr() as *const __m256i),
                );
            }

            // Inverse update: s[i] -= (d[i-1] + d[i] + 2) >> 2
            for i in 0..sn {
                let si_row = 2 * i;
                let dim1_row = 2 * (if i > 0 { i - 1 } else { 0 }) + 1;
                let di_row = if dn > 0 { 2 * i.min(dn - 1) + 1 } else { 0 };

                let s = _mm256_loadu_si256(
                    data[si_row * stride + col_base..].as_ptr() as *const __m256i
                );
                let dim1 = _mm256_loadu_si256(
                    data[dim1_row * stride + col_base..].as_ptr() as *const __m256i
                );
                let di = if dn > 0 {
                    _mm256_loadu_si256(data[di_row * stride + col_base..].as_ptr() as *const __m256i)
                } else {
                    _mm256_setzero_si256()
                };

                let upd = _mm256_srai_epi32(_mm256_add_epi32(_mm256_add_epi32(dim1, di), two), 2);
                let s_new = _mm256_sub_epi32(s, upd);

                _mm256_storeu_si256(
                    data[si_row * stride + col_base..].as_mut_ptr() as *mut __m256i,
                    s_new,
                );
            }

            // Inverse predict: d[i] += (s[i] + s[i+1]) >> 1
            for i in 0..dn {
                let si_row = 2 * i;
                let si1_row = 2 * (i + 1).min(sn - 1);
                let di_row = 2 * i + 1;

                let si = _mm256_loadu_si256(
                    data[si_row * stride + col_base..].as_ptr() as *const __m256i
                );
                let si1 = _mm256_loadu_si256(
                    data[si1_row * stride + col_base..].as_ptr() as *const __m256i
                );
                let d = _mm256_loadu_si256(
                    data[di_row * stride + col_base..].as_ptr() as *const __m256i
                );

                let pred = _mm256_srai_epi32(_mm256_add_epi32(si, si1), 1);
                let d_new = _mm256_add_epi32(d, pred);

                _mm256_storeu_si256(
                    data[di_row * stride + col_base..].as_mut_ptr() as *mut __m256i,
                    d_new,
                );
            }
        }
    }

    /// Inverse 5-3 vertical pass for 4 columns using SSE2.
    ///
    /// # Safety
    ///
    /// Caller must ensure SSE2 is available and `col_base + 4 <= stride`.
    #[target_feature(enable = "sse2")]
    pub unsafe fn dwt_decode_vert_53_sse2(
        data: &mut [i32],
        stride: usize,
        rh: usize,
        sn: usize,
        dn: usize,
        col_base: usize,
    ) {
        if sn + dn <= 1 {
            return;
        }

        unsafe {
            let two = _mm_set1_epi32(2);

            // Interleave
            let mut buf = vec![[0i32; 4]; rh];
            for i in 0..sn {
                _mm_storeu_si128(
                    buf[2 * i].as_mut_ptr() as *mut __m128i,
                    _mm_loadu_si128(data[i * stride + col_base..].as_ptr() as *const __m128i),
                );
            }
            for i in 0..dn {
                _mm_storeu_si128(
                    buf[2 * i + 1].as_mut_ptr() as *mut __m128i,
                    _mm_loadu_si128(data[(sn + i) * stride + col_base..].as_ptr() as *const __m128i),
                );
            }
            for row in 0..rh {
                _mm_storeu_si128(
                    data[row * stride + col_base..].as_mut_ptr() as *mut __m128i,
                    _mm_loadu_si128(buf[row].as_ptr() as *const __m128i),
                );
            }

            // Inverse update
            for i in 0..sn {
                let si_row = 2 * i;
                let dim1_row = 2 * (if i > 0 { i - 1 } else { 0 }) + 1;
                let di_row = if dn > 0 { 2 * i.min(dn - 1) + 1 } else { 0 };

                let s =
                    _mm_loadu_si128(data[si_row * stride + col_base..].as_ptr() as *const __m128i);
                let dim1 = _mm_loadu_si128(
                    data[dim1_row * stride + col_base..].as_ptr() as *const __m128i
                );
                let di = if dn > 0 {
                    _mm_loadu_si128(data[di_row * stride + col_base..].as_ptr() as *const __m128i)
                } else {
                    _mm_setzero_si128()
                };

                let upd = _mm_srai_epi32(_mm_add_epi32(_mm_add_epi32(dim1, di), two), 2);
                let s_new = _mm_sub_epi32(s, upd);

                _mm_storeu_si128(
                    data[si_row * stride + col_base..].as_mut_ptr() as *mut __m128i,
                    s_new,
                );
            }

            // Inverse predict
            for i in 0..dn {
                let si_row = 2 * i;
                let si1_row = 2 * (i + 1).min(sn - 1);
                let di_row = 2 * i + 1;

                let si =
                    _mm_loadu_si128(data[si_row * stride + col_base..].as_ptr() as *const __m128i);
                let si1 =
                    _mm_loadu_si128(data[si1_row * stride + col_base..].as_ptr() as *const __m128i);
                let d =
                    _mm_loadu_si128(data[di_row * stride + col_base..].as_ptr() as *const __m128i);

                let pred = _mm_srai_epi32(_mm_add_epi32(si, si1), 1);
                let d_new = _mm_add_epi32(d, pred);

                _mm_storeu_si128(
                    data[di_row * stride + col_base..].as_mut_ptr() as *mut __m128i,
                    d_new,
                );
            }
        }
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub use inner::*;

#[cfg(test)]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod tests {
    use super::*;
    use crate::transform::dwt::{dwt_decode_2d_53, dwt_encode_2d_53};

    /// Verify SIMD vertical pass produces same result as scalar 2D DWT.
    #[test]
    fn encode_decode_53_roundtrip_with_simd() {
        // The 2D functions internally dispatch to SIMD for vertical passes.
        // Verify full encode→decode roundtrip is lossless.
        let w = 32;
        let h = 32;
        let stride = w;
        let num_res = 4;
        let original: Vec<i32> = (0..w * h).map(|i| (i as i32 * 7 + 13) % 256).collect();
        let mut data = original.clone();

        dwt_encode_2d_53(&mut data, w, h, stride, num_res).unwrap();
        assert_ne!(data, original);
        dwt_decode_2d_53(&mut data, w, h, stride, num_res).unwrap();
        assert_eq!(data, original);
    }

    /// Verify with non-SIMD-aligned width (not multiple of 8).
    #[test]
    fn encode_decode_53_non_aligned_width() {
        let w = 35;
        let h = 27;
        let stride = w;
        let num_res = 3;
        let original: Vec<i32> = (0..w * h)
            .map(|i| (i as i32 * 11 + 5) % 512 - 256)
            .collect();
        let mut data = original.clone();

        dwt_encode_2d_53(&mut data, w, h, stride, num_res).unwrap();
        dwt_decode_2d_53(&mut data, w, h, stride, num_res).unwrap();
        assert_eq!(data, original);
    }

    /// Verify SIMD vertical pass functions directly match scalar.
    #[test]
    fn encode_vert_53_avx2_matches_scalar() {
        if !is_x86_feature_detected!("avx2") {
            return;
        }
        let w = 16;
        let h = 10;
        let stride = w;
        let sn = 5;
        let dn = 5;

        // Create test data
        let original: Vec<i32> = (0..w * h).map(|i| (i as i32 * 3 + 7) % 200 - 100).collect();

        // Scalar: use existing 2D encode which processes one column at a time
        let mut scalar = original.clone();
        {
            let mut tmp = vec![0i32; h];
            let mut sep = vec![0i32; h];
            for j in 0..8 {
                for i in 0..h {
                    tmp[i] = scalar[i * stride + j];
                }
                crate::transform::dwt::dwt_encode_1_53(&mut tmp[..h], sn, dn, false);
                crate::transform::dwt::deinterleave_h(&tmp[..h], &mut sep[..h], sn, dn, false);
                for i in 0..h {
                    scalar[i * stride + j] = sep[i];
                }
            }
        }

        // SIMD: process 8 columns at once
        let mut simd = original.clone();
        unsafe { dwt_encode_vert_53_avx2(&mut simd, stride, h, sn, dn, 0) };

        // Compare first 8 columns
        for row in 0..h {
            for col in 0..8 {
                assert_eq!(
                    scalar[row * stride + col],
                    simd[row * stride + col],
                    "mismatch at row={row}, col={col}"
                );
            }
        }
    }

    /// Verify SSE2 variant matches scalar.
    #[test]
    fn encode_vert_53_sse2_matches_scalar() {
        if !is_x86_feature_detected!("sse2") {
            return;
        }
        let w = 16;
        let h = 10;
        let stride = w;
        let sn = 5;
        let dn = 5;

        let original: Vec<i32> = (0..w * h).map(|i| (i as i32 * 3 + 7) % 200 - 100).collect();

        let mut scalar = original.clone();
        {
            let mut tmp = vec![0i32; h];
            let mut sep = vec![0i32; h];
            for j in 0..4 {
                for i in 0..h {
                    tmp[i] = scalar[i * stride + j];
                }
                crate::transform::dwt::dwt_encode_1_53(&mut tmp[..h], sn, dn, false);
                crate::transform::dwt::deinterleave_h(&tmp[..h], &mut sep[..h], sn, dn, false);
                for i in 0..h {
                    scalar[i * stride + j] = sep[i];
                }
            }
        }

        let mut simd = original.clone();
        unsafe { dwt_encode_vert_53_sse2(&mut simd, stride, h, sn, dn, 0) };

        for row in 0..h {
            for col in 0..4 {
                assert_eq!(
                    scalar[row * stride + col],
                    simd[row * stride + col],
                    "mismatch at row={row}, col={col}"
                );
            }
        }
    }

    /// Verify decode AVX2 vertical pass matches scalar.
    #[test]
    fn decode_vert_53_avx2_matches_scalar() {
        if !is_x86_feature_detected!("avx2") {
            return;
        }
        let w = 16;
        let h = 10;
        let stride = w;
        let sn = 5;
        let dn = 5;

        // Create already-encoded data (separated: s0..s4 in top rows, d0..d4 in bottom)
        let encoded: Vec<i32> = (0..w * h)
            .map(|i| (i as i32 * 13 + 3) % 200 - 100)
            .collect();

        // Scalar decode
        let mut scalar = encoded.clone();
        {
            let mut tmp = vec![0i32; h];
            let mut sep = vec![0i32; h];
            for j in 0..8 {
                for i in 0..h {
                    sep[i] = scalar[i * stride + j];
                }
                crate::transform::dwt::interleave_h(&sep[..h], &mut tmp[..h], sn, dn, false);
                crate::transform::dwt::dwt_decode_1_53(&mut tmp[..h], sn, dn, false);
                for i in 0..h {
                    scalar[i * stride + j] = tmp[i];
                }
            }
        }

        // SIMD decode
        let mut simd = encoded.clone();
        unsafe { dwt_decode_vert_53_avx2(&mut simd, stride, h, sn, dn, 0) };

        for row in 0..h {
            for col in 0..8 {
                assert_eq!(
                    scalar[row * stride + col],
                    simd[row * stride + col],
                    "mismatch at row={row}, col={col}"
                );
            }
        }
    }
}
