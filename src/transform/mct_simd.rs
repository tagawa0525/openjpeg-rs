// MCT SIMD acceleration (SSE2/AVX2 for i32, SSE/AVX for f32).
//
// Each public function processes as many samples as possible via SIMD,
// then delegates the remainder to a scalar tail loop.
// All functions are gated on x86/x86_64 target and require runtime CPU
// feature detection before calling.

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod inner {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    use crate::transform::mct::{
        ICT_CBB, ICT_CBG, ICT_CBR, ICT_CRB, ICT_CRG, ICT_CRR, ICT_VBU, ICT_VGU, ICT_VGV, ICT_VRV,
        ICT_YB, ICT_YG, ICT_YR, mct_decode_real_scalar, mct_decode_scalar, mct_encode_real_scalar,
        mct_encode_scalar,
    };

    // -----------------------------------------------------------------------
    // RCT (i32): Y = (R + 2G + B) >> 2,  Cb = B - G,  Cr = R - G
    // -----------------------------------------------------------------------

    /// Forward RCT using AVX2 (8×i32 per iteration).
    ///
    /// # Safety
    ///
    /// Caller must ensure the CPU supports AVX2 (`is_x86_feature_detected!("avx2")`).
    #[target_feature(enable = "avx2")]
    pub unsafe fn mct_encode_avx2(c0: &mut [i32], c1: &mut [i32], c2: &mut [i32]) {
        let n = c0.len().min(c1.len()).min(c2.len());
        let chunks = n / 8;
        unsafe {
            for i in 0..chunks {
                let off = i * 8;
                let r = _mm256_loadu_si256(c0[off..].as_ptr() as *const __m256i);
                let g = _mm256_loadu_si256(c1[off..].as_ptr() as *const __m256i);
                let b = _mm256_loadu_si256(c2[off..].as_ptr() as *const __m256i);
                let y = _mm256_srai_epi32(
                    _mm256_add_epi32(_mm256_add_epi32(r, _mm256_slli_epi32(g, 1)), b),
                    2,
                );
                let cb = _mm256_sub_epi32(b, g);
                let cr = _mm256_sub_epi32(r, g);
                _mm256_storeu_si256(c0[off..].as_mut_ptr() as *mut __m256i, y);
                _mm256_storeu_si256(c1[off..].as_mut_ptr() as *mut __m256i, cb);
                _mm256_storeu_si256(c2[off..].as_mut_ptr() as *mut __m256i, cr);
            }
        }
        let processed = chunks * 8;
        mct_encode_scalar(
            &mut c0[processed..],
            &mut c1[processed..],
            &mut c2[processed..],
        );
    }

    /// Forward RCT using SSE2 (4×i32 per iteration).
    ///
    /// # Safety
    ///
    /// Caller must ensure the CPU supports SSE2 (`is_x86_feature_detected!("sse2")`).
    #[target_feature(enable = "sse2")]
    pub unsafe fn mct_encode_sse2(c0: &mut [i32], c1: &mut [i32], c2: &mut [i32]) {
        let n = c0.len().min(c1.len()).min(c2.len());
        let chunks = n / 4;
        unsafe {
            for i in 0..chunks {
                let off = i * 4;
                let r = _mm_loadu_si128(c0[off..].as_ptr() as *const __m128i);
                let g = _mm_loadu_si128(c1[off..].as_ptr() as *const __m128i);
                let b = _mm_loadu_si128(c2[off..].as_ptr() as *const __m128i);
                let y = _mm_srai_epi32(_mm_add_epi32(_mm_add_epi32(r, _mm_slli_epi32(g, 1)), b), 2);
                let cb = _mm_sub_epi32(b, g);
                let cr = _mm_sub_epi32(r, g);
                _mm_storeu_si128(c0[off..].as_mut_ptr() as *mut __m128i, y);
                _mm_storeu_si128(c1[off..].as_mut_ptr() as *mut __m128i, cb);
                _mm_storeu_si128(c2[off..].as_mut_ptr() as *mut __m128i, cr);
            }
        }
        let processed = chunks * 4;
        mct_encode_scalar(
            &mut c0[processed..],
            &mut c1[processed..],
            &mut c2[processed..],
        );
    }

    // -----------------------------------------------------------------------
    // Inverse RCT: G = Y - (Cb + Cr) >> 2,  R = Cr + G,  B = Cb + G
    // -----------------------------------------------------------------------

    /// Inverse RCT using AVX2 (8×i32 per iteration).
    ///
    /// # Safety
    ///
    /// Caller must ensure the CPU supports AVX2 (`is_x86_feature_detected!("avx2")`).
    #[target_feature(enable = "avx2")]
    pub unsafe fn mct_decode_avx2(c0: &mut [i32], c1: &mut [i32], c2: &mut [i32]) {
        let n = c0.len().min(c1.len()).min(c2.len());
        let chunks = n / 8;
        unsafe {
            for i in 0..chunks {
                let off = i * 8;
                let y = _mm256_loadu_si256(c0[off..].as_ptr() as *const __m256i);
                let u = _mm256_loadu_si256(c1[off..].as_ptr() as *const __m256i);
                let v = _mm256_loadu_si256(c2[off..].as_ptr() as *const __m256i);
                let g = _mm256_sub_epi32(y, _mm256_srai_epi32(_mm256_add_epi32(u, v), 2));
                let r = _mm256_add_epi32(v, g);
                let b = _mm256_add_epi32(u, g);
                _mm256_storeu_si256(c0[off..].as_mut_ptr() as *mut __m256i, r);
                _mm256_storeu_si256(c1[off..].as_mut_ptr() as *mut __m256i, g);
                _mm256_storeu_si256(c2[off..].as_mut_ptr() as *mut __m256i, b);
            }
        }
        let processed = chunks * 8;
        mct_decode_scalar(
            &mut c0[processed..],
            &mut c1[processed..],
            &mut c2[processed..],
        );
    }

    /// Inverse RCT using SSE2 (4×i32 per iteration).
    ///
    /// # Safety
    ///
    /// Caller must ensure the CPU supports SSE2 (`is_x86_feature_detected!("sse2")`).
    #[target_feature(enable = "sse2")]
    pub unsafe fn mct_decode_sse2(c0: &mut [i32], c1: &mut [i32], c2: &mut [i32]) {
        let n = c0.len().min(c1.len()).min(c2.len());
        let chunks = n / 4;
        unsafe {
            for i in 0..chunks {
                let off = i * 4;
                let y = _mm_loadu_si128(c0[off..].as_ptr() as *const __m128i);
                let u = _mm_loadu_si128(c1[off..].as_ptr() as *const __m128i);
                let v = _mm_loadu_si128(c2[off..].as_ptr() as *const __m128i);
                let g = _mm_sub_epi32(y, _mm_srai_epi32(_mm_add_epi32(u, v), 2));
                let r = _mm_add_epi32(v, g);
                let b = _mm_add_epi32(u, g);
                _mm_storeu_si128(c0[off..].as_mut_ptr() as *mut __m128i, r);
                _mm_storeu_si128(c1[off..].as_mut_ptr() as *mut __m128i, g);
                _mm_storeu_si128(c2[off..].as_mut_ptr() as *mut __m128i, b);
            }
        }
        let processed = chunks * 4;
        mct_decode_scalar(
            &mut c0[processed..],
            &mut c1[processed..],
            &mut c2[processed..],
        );
    }

    // -----------------------------------------------------------------------
    // ICT forward (f32): Y/Cb/Cr from R/G/B using fixed coefficients
    // -----------------------------------------------------------------------

    /// Forward ICT using AVX (8×f32 per iteration).
    ///
    /// # Safety
    ///
    /// Caller must ensure the CPU supports AVX (`is_x86_feature_detected!("avx")`).
    #[target_feature(enable = "avx")]
    pub unsafe fn mct_encode_real_avx(c0: &mut [f32], c1: &mut [f32], c2: &mut [f32]) {
        let n = c0.len().min(c1.len()).min(c2.len());
        let chunks = n / 8;
        unsafe {
            let yr = _mm256_set1_ps(ICT_YR);
            let yg = _mm256_set1_ps(ICT_YG);
            let yb = _mm256_set1_ps(ICT_YB);
            let cbr = _mm256_set1_ps(ICT_CBR);
            let cbg = _mm256_set1_ps(ICT_CBG);
            let cbb = _mm256_set1_ps(ICT_CBB);
            let crr = _mm256_set1_ps(ICT_CRR);
            let crg = _mm256_set1_ps(ICT_CRG);
            let crb = _mm256_set1_ps(ICT_CRB);

            for i in 0..chunks {
                let off = i * 8;
                let r = _mm256_loadu_ps(c0[off..].as_ptr());
                let g = _mm256_loadu_ps(c1[off..].as_ptr());
                let b = _mm256_loadu_ps(c2[off..].as_ptr());

                let y = _mm256_add_ps(
                    _mm256_add_ps(_mm256_mul_ps(yr, r), _mm256_mul_ps(yg, g)),
                    _mm256_mul_ps(yb, b),
                );
                let cb = _mm256_add_ps(
                    _mm256_add_ps(_mm256_mul_ps(cbr, r), _mm256_mul_ps(cbg, g)),
                    _mm256_mul_ps(cbb, b),
                );
                let cr = _mm256_add_ps(
                    _mm256_add_ps(_mm256_mul_ps(crr, r), _mm256_mul_ps(crg, g)),
                    _mm256_mul_ps(crb, b),
                );

                _mm256_storeu_ps(c0[off..].as_mut_ptr(), y);
                _mm256_storeu_ps(c1[off..].as_mut_ptr(), cb);
                _mm256_storeu_ps(c2[off..].as_mut_ptr(), cr);
            }
        }
        let processed = chunks * 8;
        mct_encode_real_scalar(
            &mut c0[processed..],
            &mut c1[processed..],
            &mut c2[processed..],
        );
    }

    /// Forward ICT using SSE (4×f32 per iteration).
    ///
    /// # Safety
    ///
    /// Caller must ensure the CPU supports SSE (`is_x86_feature_detected!("sse")`).
    #[target_feature(enable = "sse")]
    pub unsafe fn mct_encode_real_sse(c0: &mut [f32], c1: &mut [f32], c2: &mut [f32]) {
        let n = c0.len().min(c1.len()).min(c2.len());
        let chunks = n / 4;
        unsafe {
            let yr = _mm_set1_ps(ICT_YR);
            let yg = _mm_set1_ps(ICT_YG);
            let yb = _mm_set1_ps(ICT_YB);
            let cbr = _mm_set1_ps(ICT_CBR);
            let cbg = _mm_set1_ps(ICT_CBG);
            let cbb = _mm_set1_ps(ICT_CBB);
            let crr = _mm_set1_ps(ICT_CRR);
            let crg = _mm_set1_ps(ICT_CRG);
            let crb = _mm_set1_ps(ICT_CRB);

            for i in 0..chunks {
                let off = i * 4;
                let r = _mm_loadu_ps(c0[off..].as_ptr());
                let g = _mm_loadu_ps(c1[off..].as_ptr());
                let b = _mm_loadu_ps(c2[off..].as_ptr());

                let y = _mm_add_ps(
                    _mm_add_ps(_mm_mul_ps(yr, r), _mm_mul_ps(yg, g)),
                    _mm_mul_ps(yb, b),
                );
                let cb = _mm_add_ps(
                    _mm_add_ps(_mm_mul_ps(cbr, r), _mm_mul_ps(cbg, g)),
                    _mm_mul_ps(cbb, b),
                );
                let cr = _mm_add_ps(
                    _mm_add_ps(_mm_mul_ps(crr, r), _mm_mul_ps(crg, g)),
                    _mm_mul_ps(crb, b),
                );

                _mm_storeu_ps(c0[off..].as_mut_ptr(), y);
                _mm_storeu_ps(c1[off..].as_mut_ptr(), cb);
                _mm_storeu_ps(c2[off..].as_mut_ptr(), cr);
            }
        }
        let processed = chunks * 4;
        mct_encode_real_scalar(
            &mut c0[processed..],
            &mut c1[processed..],
            &mut c2[processed..],
        );
    }

    // -----------------------------------------------------------------------
    // ICT inverse (f32): R/G/B from Y/Cb/Cr
    // -----------------------------------------------------------------------

    /// Inverse ICT using AVX (8×f32 per iteration).
    ///
    /// # Safety
    ///
    /// Caller must ensure the CPU supports AVX (`is_x86_feature_detected!("avx")`).
    #[target_feature(enable = "avx")]
    pub unsafe fn mct_decode_real_avx(c0: &mut [f32], c1: &mut [f32], c2: &mut [f32]) {
        let n = c0.len().min(c1.len()).min(c2.len());
        let chunks = n / 8;
        unsafe {
            let vrv = _mm256_set1_ps(ICT_VRV);
            let vgu = _mm256_set1_ps(ICT_VGU);
            let vgv = _mm256_set1_ps(ICT_VGV);
            let vbu = _mm256_set1_ps(ICT_VBU);

            for i in 0..chunks {
                let off = i * 8;
                let y = _mm256_loadu_ps(c0[off..].as_ptr());
                let u = _mm256_loadu_ps(c1[off..].as_ptr());
                let v = _mm256_loadu_ps(c2[off..].as_ptr());

                let r = _mm256_add_ps(y, _mm256_mul_ps(vrv, v));
                let g = _mm256_sub_ps(
                    _mm256_sub_ps(y, _mm256_mul_ps(vgu, u)),
                    _mm256_mul_ps(vgv, v),
                );
                let b = _mm256_add_ps(y, _mm256_mul_ps(vbu, u));

                _mm256_storeu_ps(c0[off..].as_mut_ptr(), r);
                _mm256_storeu_ps(c1[off..].as_mut_ptr(), g);
                _mm256_storeu_ps(c2[off..].as_mut_ptr(), b);
            }
        }
        let processed = chunks * 8;
        mct_decode_real_scalar(
            &mut c0[processed..],
            &mut c1[processed..],
            &mut c2[processed..],
        );
    }

    /// Inverse ICT using SSE (4×f32 per iteration).
    ///
    /// # Safety
    ///
    /// Caller must ensure the CPU supports SSE (`is_x86_feature_detected!("sse")`).
    #[target_feature(enable = "sse")]
    pub unsafe fn mct_decode_real_sse(c0: &mut [f32], c1: &mut [f32], c2: &mut [f32]) {
        let n = c0.len().min(c1.len()).min(c2.len());
        let chunks = n / 4;
        unsafe {
            let vrv = _mm_set1_ps(ICT_VRV);
            let vgu = _mm_set1_ps(ICT_VGU);
            let vgv = _mm_set1_ps(ICT_VGV);
            let vbu = _mm_set1_ps(ICT_VBU);

            for i in 0..chunks {
                let off = i * 4;
                let y = _mm_loadu_ps(c0[off..].as_ptr());
                let u = _mm_loadu_ps(c1[off..].as_ptr());
                let v = _mm_loadu_ps(c2[off..].as_ptr());

                let r = _mm_add_ps(y, _mm_mul_ps(vrv, v));
                let g = _mm_sub_ps(_mm_sub_ps(y, _mm_mul_ps(vgu, u)), _mm_mul_ps(vgv, v));
                let b = _mm_add_ps(y, _mm_mul_ps(vbu, u));

                _mm_storeu_ps(c0[off..].as_mut_ptr(), r);
                _mm_storeu_ps(c1[off..].as_mut_ptr(), g);
                _mm_storeu_ps(c2[off..].as_mut_ptr(), b);
            }
        }
        let processed = chunks * 4;
        mct_decode_real_scalar(
            &mut c0[processed..],
            &mut c1[processed..],
            &mut c2[processed..],
        );
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub use inner::*;

#[cfg(test)]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod tests {
    use super::*;
    use crate::transform::mct;

    fn gen_i32(n: usize, seed: i32) -> Vec<i32> {
        (0..n)
            .map(|i| ((i as i32) * seed + 17) % 500 - 250)
            .collect()
    }
    fn gen_f32(n: usize, seed: i32) -> Vec<f32> {
        (0..n)
            .map(|i| (((i as i32) * seed + 17) % 500 - 250) as f32)
            .collect()
    }

    // --- RCT encode ---

    #[test]
    fn rct_encode_avx2_matches_scalar() {
        if !is_x86_feature_detected!("avx2") {
            return;
        }
        let n = 35;
        let (mut c0s, mut c1s, mut c2s) = (gen_i32(n, 3), gen_i32(n, 7), gen_i32(n, 11));
        let (mut c0v, mut c1v, mut c2v) = (c0s.clone(), c1s.clone(), c2s.clone());
        mct::mct_encode_scalar(&mut c0s, &mut c1s, &mut c2s);
        unsafe { mct_encode_avx2(&mut c0v, &mut c1v, &mut c2v) };
        assert_eq!(c0s, c0v, "c0 mismatch");
        assert_eq!(c1s, c1v, "c1 mismatch");
        assert_eq!(c2s, c2v, "c2 mismatch");
    }

    #[test]
    fn rct_encode_sse2_matches_scalar() {
        if !is_x86_feature_detected!("sse2") {
            return;
        }
        let n = 19;
        let (mut c0s, mut c1s, mut c2s) = (gen_i32(n, 3), gen_i32(n, 7), gen_i32(n, 11));
        let (mut c0v, mut c1v, mut c2v) = (c0s.clone(), c1s.clone(), c2s.clone());
        mct::mct_encode_scalar(&mut c0s, &mut c1s, &mut c2s);
        unsafe { mct_encode_sse2(&mut c0v, &mut c1v, &mut c2v) };
        assert_eq!(c0s, c0v, "c0 mismatch");
        assert_eq!(c1s, c1v, "c1 mismatch");
        assert_eq!(c2s, c2v, "c2 mismatch");
    }

    // --- RCT decode ---

    #[test]
    fn rct_decode_avx2_matches_scalar() {
        if !is_x86_feature_detected!("avx2") {
            return;
        }
        let n = 35;
        let (mut c0s, mut c1s, mut c2s) = (gen_i32(n, 5), gen_i32(n, 9), gen_i32(n, 13));
        let (mut c0v, mut c1v, mut c2v) = (c0s.clone(), c1s.clone(), c2s.clone());
        mct::mct_decode_scalar(&mut c0s, &mut c1s, &mut c2s);
        unsafe { mct_decode_avx2(&mut c0v, &mut c1v, &mut c2v) };
        assert_eq!(c0s, c0v, "c0 mismatch");
        assert_eq!(c1s, c1v, "c1 mismatch");
        assert_eq!(c2s, c2v, "c2 mismatch");
    }

    #[test]
    fn rct_decode_sse2_matches_scalar() {
        if !is_x86_feature_detected!("sse2") {
            return;
        }
        let n = 19;
        let (mut c0s, mut c1s, mut c2s) = (gen_i32(n, 5), gen_i32(n, 9), gen_i32(n, 13));
        let (mut c0v, mut c1v, mut c2v) = (c0s.clone(), c1s.clone(), c2s.clone());
        mct::mct_decode_scalar(&mut c0s, &mut c1s, &mut c2s);
        unsafe { mct_decode_sse2(&mut c0v, &mut c1v, &mut c2v) };
        assert_eq!(c0s, c0v, "c0 mismatch");
        assert_eq!(c1s, c1v, "c1 mismatch");
        assert_eq!(c2s, c2v, "c2 mismatch");
    }

    // --- ICT encode ---

    #[test]
    fn ict_encode_avx_matches_scalar() {
        if !is_x86_feature_detected!("avx") {
            return;
        }
        let n = 35;
        let (mut c0s, mut c1s, mut c2s) = (gen_f32(n, 3), gen_f32(n, 7), gen_f32(n, 11));
        let (mut c0v, mut c1v, mut c2v) = (c0s.clone(), c1s.clone(), c2s.clone());
        mct::mct_encode_real_scalar(&mut c0s, &mut c1s, &mut c2s);
        unsafe { mct_encode_real_avx(&mut c0v, &mut c1v, &mut c2v) };
        for i in 0..n {
            assert!((c0s[i] - c0v[i]).abs() < 1e-4, "c0[{i}]");
            assert!((c1s[i] - c1v[i]).abs() < 1e-4, "c1[{i}]");
            assert!((c2s[i] - c2v[i]).abs() < 1e-4, "c2[{i}]");
        }
    }

    #[test]
    fn ict_encode_sse_matches_scalar() {
        if !is_x86_feature_detected!("sse") {
            return;
        }
        let n = 19;
        let (mut c0s, mut c1s, mut c2s) = (gen_f32(n, 3), gen_f32(n, 7), gen_f32(n, 11));
        let (mut c0v, mut c1v, mut c2v) = (c0s.clone(), c1s.clone(), c2s.clone());
        mct::mct_encode_real_scalar(&mut c0s, &mut c1s, &mut c2s);
        unsafe { mct_encode_real_sse(&mut c0v, &mut c1v, &mut c2v) };
        for i in 0..n {
            assert!((c0s[i] - c0v[i]).abs() < 1e-4, "c0[{i}]");
            assert!((c1s[i] - c1v[i]).abs() < 1e-4, "c1[{i}]");
            assert!((c2s[i] - c2v[i]).abs() < 1e-4, "c2[{i}]");
        }
    }

    // --- ICT decode ---

    #[test]
    fn ict_decode_avx_matches_scalar() {
        if !is_x86_feature_detected!("avx") {
            return;
        }
        let n = 35;
        let (mut c0s, mut c1s, mut c2s) = (gen_f32(n, 5), gen_f32(n, 9), gen_f32(n, 13));
        let (mut c0v, mut c1v, mut c2v) = (c0s.clone(), c1s.clone(), c2s.clone());
        mct::mct_decode_real_scalar(&mut c0s, &mut c1s, &mut c2s);
        unsafe { mct_decode_real_avx(&mut c0v, &mut c1v, &mut c2v) };
        for i in 0..n {
            assert!((c0s[i] - c0v[i]).abs() < 1e-4, "c0[{i}]");
            assert!((c1s[i] - c1v[i]).abs() < 1e-4, "c1[{i}]");
            assert!((c2s[i] - c2v[i]).abs() < 1e-4, "c2[{i}]");
        }
    }

    #[test]
    fn ict_decode_sse_matches_scalar() {
        if !is_x86_feature_detected!("sse") {
            return;
        }
        let n = 19;
        let (mut c0s, mut c1s, mut c2s) = (gen_f32(n, 5), gen_f32(n, 9), gen_f32(n, 13));
        let (mut c0v, mut c1v, mut c2v) = (c0s.clone(), c1s.clone(), c2s.clone());
        mct::mct_decode_real_scalar(&mut c0s, &mut c1s, &mut c2s);
        unsafe { mct_decode_real_sse(&mut c0v, &mut c1v, &mut c2v) };
        for i in 0..n {
            assert!((c0s[i] - c0v[i]).abs() < 1e-4, "c0[{i}]");
            assert!((c1s[i] - c1v[i]).abs() < 1e-4, "c1[{i}]");
            assert!((c2s[i] - c2v[i]).abs() < 1e-4, "c2[{i}]");
        }
    }
}
