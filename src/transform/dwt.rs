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
}
