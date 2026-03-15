// Discrete Wavelet Transform (C: dwt.c)
// Scalar-only implementation (no SIMD, no thread pool).

/// Forward 1D 5-3 lifting (in-place on interleaved data).
///
/// Data layout: `[s0, d0, s1, d1, ...]` when `cas=false` (even origin),
/// `[d0, s0, d1, s1, ...]` when `cas=true` (odd origin).
///
/// `sn`: number of low-pass samples, `dn`: number of high-pass samples.
/// `cas`: if true, high-pass starts at index 0 (odd subgrid origin).
pub fn dwt_encode_1_53(data: &mut [i32], sn: usize, dn: usize, cas: bool) {
    if !cas {
        // cas=0: s at even indices, d at odd indices
        if sn + dn <= 1 {
            return;
        }
        // Predict: d[i] -= (s_[i] + s_[i+1]) >> 1
        for i in 0..dn {
            let si = data[2 * i];
            let si1 = data[2 * (i + 1).min(sn - 1)];
            data[2 * i + 1] -= (si + si1) >> 1;
        }
        // Update: s[i] += (d_[i-1] + d_[i] + 2) >> 2
        for i in 0..sn {
            let dim1 = data[2 * (if i > 0 { i - 1 } else { 0 }) + 1];
            let di = data[2 * i.min(dn - 1) + 1];
            data[2 * i] += (dim1 + di + 2) >> 2;
        }
    } else {
        // cas=1: d at even indices (S macro), s at odd indices (D macro)
        if sn == 0 && dn == 1 {
            data[0] *= 2;
            return;
        }
        if sn + dn <= 1 {
            return;
        }
        // Predict: S(i) -= (DD_(i) + DD_(i-1)) >> 1
        for i in 0..dn {
            let dd_i = data[2 * i.min(sn - 1) + 1];
            let dd_im1 = data[2 * (if i > 0 { (i - 1).min(sn - 1) } else { 0 }) + 1];
            data[2 * i] -= (dd_i + dd_im1) >> 1;
        }
        // Update: D(i) += (SS_(i) + SS_(i+1) + 2) >> 2
        for i in 0..sn {
            let ss_i = data[2 * i.min(dn - 1)];
            let ss_ip1 = data[2 * (i + 1).min(dn - 1)];
            data[2 * i + 1] += (ss_i + ss_ip1 + 2) >> 2;
        }
    }
}

/// 9-7 lifting coefficients (ITU-T T.800 Table F.4).
/// Values match C version exactly; allow excessive_precision to preserve spec values.
#[allow(clippy::excessive_precision)]
pub const DWT_ALPHA: f32 = -1.586134342;
#[allow(clippy::excessive_precision)]
pub const DWT_BETA: f32 = -0.052980118;
#[allow(clippy::excessive_precision)]
pub const DWT_GAMMA: f32 = 0.882911075;
#[allow(clippy::excessive_precision)]
pub const DWT_DELTA: f32 = 0.443506852;
#[allow(clippy::excessive_precision)]
pub const DWT_K: f32 = 1.230174105;
#[allow(clippy::excessive_precision)]
pub const DWT_INV_K: f32 = 1.0 / 1.230174105;

/// Inverse 1D 5-3 lifting (in-place on interleaved data).
pub fn dwt_decode_1_53(data: &mut [i32], sn: usize, dn: usize, cas: bool) {
    if !cas {
        // cas=0: s at even indices, d at odd indices
        if sn + dn <= 1 {
            return;
        }
        // Undo update: s[i] -= (d_[i-1] + d_[i] + 2) >> 2
        for i in 0..sn {
            let dim1 = data[2 * (if i > 0 { i - 1 } else { 0 }) + 1];
            let di = data[2 * i.min(dn - 1) + 1];
            data[2 * i] -= (dim1 + di + 2) >> 2;
        }
        // Undo predict: d[i] += (s_[i] + s_[i+1]) >> 1
        for i in 0..dn {
            let si = data[2 * i];
            let si1 = data[2 * (i + 1).min(sn - 1)];
            data[2 * i + 1] += (si + si1) >> 1;
        }
    } else {
        // cas=1: d at even indices (S macro), s at odd indices (D macro)
        if sn == 0 && dn == 1 {
            data[0] /= 2;
            return;
        }
        if sn + dn <= 1 {
            return;
        }
        // Undo update: D(i) -= (SS_(i) + SS_(i+1) + 2) >> 2
        for i in 0..sn {
            let ss_i = data[2 * i.min(dn - 1)];
            let ss_ip1 = data[2 * (i + 1).min(dn - 1)];
            data[2 * i + 1] -= (ss_i + ss_ip1 + 2) >> 2;
        }
        // Undo predict: S(i) += (DD_(i) + DD_(i-1)) >> 1
        for i in 0..dn {
            let dd_i = data[2 * i.min(sn - 1) + 1];
            let dd_im1 = data[2 * (if i > 0 { (i - 1).min(sn - 1) } else { 0 }) + 1];
            data[2 * i] += (dd_i + dd_im1) >> 1;
        }
    }
}

/// Forward 1D 9-7 lifting (in-place on interleaved data).
pub fn dwt_encode_1_97(_data: &mut [f32], _sn: usize, _dn: usize, _cas: bool) {
    todo!()
}

/// Inverse 1D 9-7 lifting (in-place on interleaved data).
pub fn dwt_decode_1_97(_data: &mut [f32], _sn: usize, _dn: usize, _cas: bool) {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== 1D 5-3 tests ====================

    #[test]
    fn encode_1d_53_even_cas0_roundtrip() {
        // Even length, cas=0, non-linear data
        let original = vec![10, 23, 35, 41, 58, 62, 77, 80];
        let mut data = original.clone();
        let sn = 4;
        let dn = 4;
        dwt_encode_1_53(&mut data, sn, dn, false);
        assert_ne!(data, original);
        dwt_decode_1_53(&mut data, sn, dn, false);
        assert_eq!(data, original);
    }

    #[test]
    fn encode_1d_53_odd_cas0_roundtrip() {
        // Odd length, cas=0, non-linear data
        let original = vec![10, 23, 35, 41, 58, 62, 77];
        let mut data = original.clone();
        let sn = 4;
        let dn = 3;
        dwt_encode_1_53(&mut data, sn, dn, false);
        assert_ne!(data, original);
        dwt_decode_1_53(&mut data, sn, dn, false);
        assert_eq!(data, original);
    }

    #[test]
    fn encode_1d_53_cas1_roundtrip() {
        // cas=1: high-pass at even indices
        let original = vec![100, 200, 300, 400, 500, 600];
        let mut data = original.clone();
        let sn = 3;
        let dn = 3;
        dwt_encode_1_53(&mut data, sn, dn, true);
        assert_ne!(data, original);
        dwt_decode_1_53(&mut data, sn, dn, true);
        assert_eq!(data, original);
    }

    #[test]
    fn encode_1d_53_length_1() {
        // Single element: no-op for cas=0
        let mut data = vec![42];
        dwt_encode_1_53(&mut data, 1, 0, false);
        assert_eq!(data, vec![42]);
    }

    #[test]
    fn encode_1d_53_length_1_cas1() {
        // Single element, cas=1: value is doubled on encode, halved on decode
        let mut data = vec![42];
        dwt_encode_1_53(&mut data, 0, 1, true);
        assert_eq!(data[0], 84);
        dwt_decode_1_53(&mut data, 0, 1, true);
        assert_eq!(data[0], 42);
    }

    #[test]
    fn encode_1d_53_length_2_cas0_roundtrip() {
        let original = vec![100, 200];
        let mut data = original.clone();
        let sn = 1;
        let dn = 1;
        dwt_encode_1_53(&mut data, sn, dn, false);
        dwt_decode_1_53(&mut data, sn, dn, false);
        assert_eq!(data, original);
    }

    #[test]
    fn encode_1d_53_known_values_cas0() {
        // Input: [10, 20, 30, 40] (interleaved as s0=10, d0=20, s1=30, d1=40)
        // cas=0, sn=2, dn=2
        //
        // Predict: d[i] -= (s[i] + s[i+1]) >> 1
        //   d[0] = 20 - (10 + 30) >> 1 = 0
        //   d[1] = 40 - (30 + 30) >> 1 = 10  (boundary: s_[2] = s[1] = 30)
        // Update: s[i] += (d_[i-1] + d[i] + 2) >> 2
        //   s[0] = 10 + (0 + 0 + 2) >> 2 = 10  (boundary: d_[-1] = d[0] = 0)
        //   s[1] = 30 + (0 + 10 + 2) >> 2 = 33
        let mut data = vec![10, 20, 30, 40];
        dwt_encode_1_53(&mut data, 2, 2, false);
        assert_eq!(data, vec![10, 0, 33, 10]);
    }

    // ==================== 1D 9-7 tests ====================

    fn assert_f32_eq(a: &[f32], b: &[f32], tol: f32) {
        assert_eq!(a.len(), b.len());
        for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
            assert!(
                (x - y).abs() < tol,
                "index {}: {} vs {}, diff {}",
                i,
                x,
                y,
                (x - y).abs()
            );
        }
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn encode_1d_97_even_cas0_roundtrip() {
        let original = vec![10.0f32, 23.0, 35.0, 41.0, 58.0, 62.0, 77.0, 80.0];
        let mut data = original.clone();
        dwt_encode_1_97(&mut data, 4, 4, false);
        dwt_decode_1_97(&mut data, 4, 4, false);
        assert_f32_eq(&data, &original, 1e-4);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn encode_1d_97_odd_cas0_roundtrip() {
        let original = vec![10.0f32, 23.0, 35.0, 41.0, 58.0, 62.0, 77.0];
        let mut data = original.clone();
        dwt_encode_1_97(&mut data, 4, 3, false);
        dwt_decode_1_97(&mut data, 4, 3, false);
        assert_f32_eq(&data, &original, 1e-4);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn encode_1d_97_cas1_roundtrip() {
        let original = vec![100.0f32, 200.0, 300.0, 400.0, 500.0, 600.0];
        let mut data = original.clone();
        dwt_encode_1_97(&mut data, 3, 3, true);
        dwt_decode_1_97(&mut data, 3, 3, true);
        assert_f32_eq(&data, &original, 1e-4);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn encode_1d_97_length_1_noop() {
        let mut data = vec![42.0f32];
        dwt_encode_1_97(&mut data, 1, 0, false);
        assert_eq!(data[0], 42.0);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn encode_1d_97_length_2_roundtrip() {
        let original = vec![100.0f32, 200.0];
        let mut data = original.clone();
        dwt_encode_1_97(&mut data, 1, 1, false);
        dwt_decode_1_97(&mut data, 1, 1, false);
        assert_f32_eq(&data, &original, 1e-4);
    }
}
