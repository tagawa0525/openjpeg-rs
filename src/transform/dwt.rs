// Discrete Wavelet Transform (C: dwt.c)
// Scalar-only implementation (no SIMD, no thread pool).

/// Forward 1D 5-3 lifting (in-place on interleaved data).
/// `sn`: number of low-pass samples, `dn`: number of high-pass samples.
/// `cas`: if true, high-pass starts at index 0 (odd subgrid origin).
pub fn dwt_encode_1_53(_data: &mut [i32], _sn: usize, _dn: usize, _cas: bool) {
    todo!()
}

/// Inverse 1D 5-3 lifting (in-place on interleaved data).
pub fn dwt_decode_1_53(_data: &mut [i32], _sn: usize, _dn: usize, _cas: bool) {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== 1D 5-3 tests ====================

    #[test]
    #[ignore = "not yet implemented"]
    fn encode_1d_53_even_cas0_roundtrip() {
        // Even length, cas=0: low-pass at even indices
        let original = vec![10, 20, 30, 40, 50, 60, 70, 80];
        let mut data = original.clone();
        let sn = 4; // (8 + 1) >> 1 = 4 low-pass
        let dn = 4; // 8 - 4 = 4 high-pass
        dwt_encode_1_53(&mut data, sn, dn, false);
        // After encode, data should be different from original
        assert_ne!(data, original);
        dwt_decode_1_53(&mut data, sn, dn, false);
        assert_eq!(data, original);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn encode_1d_53_odd_cas0_roundtrip() {
        // Odd length, cas=0
        let original = vec![10, 20, 30, 40, 50, 60, 70];
        let mut data = original.clone();
        let sn = 4; // (7 + 1) >> 1 = 4
        let dn = 3; // 7 - 4 = 3
        dwt_encode_1_53(&mut data, sn, dn, false);
        assert_ne!(data, original);
        dwt_decode_1_53(&mut data, sn, dn, false);
        assert_eq!(data, original);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn encode_1d_53_cas1_roundtrip() {
        // cas=1: high-pass at even indices
        let original = vec![100, 200, 300, 400, 500, 600];
        let mut data = original.clone();
        let sn = 3; // 6 >> 1 = 3
        let dn = 3; // 6 - 3 = 3
        dwt_encode_1_53(&mut data, sn, dn, true);
        assert_ne!(data, original);
        dwt_decode_1_53(&mut data, sn, dn, true);
        assert_eq!(data, original);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn encode_1d_53_length_1() {
        // Single element: no-op for cas=0
        let mut data = vec![42];
        dwt_encode_1_53(&mut data, 1, 0, false);
        assert_eq!(data, vec![42]);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn encode_1d_53_length_1_cas1() {
        // Single element, cas=1: value is doubled on encode, halved on decode
        let mut data = vec![42];
        dwt_encode_1_53(&mut data, 0, 1, true);
        assert_eq!(data[0], 84);
        dwt_decode_1_53(&mut data, 0, 1, true);
        assert_eq!(data[0], 42);
    }

    #[test]
    #[ignore = "not yet implemented"]
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
    #[ignore = "not yet implemented"]
    fn encode_1d_53_known_values_cas0() {
        // Verify the 5-3 predict/update steps produce expected values
        // Input: [10, 20, 30, 40] (interleaved as s0=10, d0=20, s1=30, d1=40)
        // cas=0, sn=2, dn=2
        //
        // Forward:
        //   Predict: d[i] -= (s[i] + s[i+1]) >> 1
        //     d[0] = 20 - (10 + 30) >> 1 = 20 - 20 = 0
        //     d[1] = 40 - (30 + 30) >> 1 = 40 - 30 = 10  (boundary: s[2] clamped to s[1]=30)
        //   Update: s[i] += (d[i-1] + d[i] + 2) >> 2
        //     s[0] = 10 + (0 + 0 + 2) >> 2 = 10 + 0 = 10  (boundary: d[-1] clamped to d[0]=0)
        //     s[1] = 30 + (0 + 10 + 2) >> 2 = 30 + 3 = 33
        //
        // Result (interleaved): [10, 0, 33, 10]
        let mut data = vec![10, 20, 30, 40];
        dwt_encode_1_53(&mut data, 2, 2, false);
        assert_eq!(data, vec![10, 0, 33, 10]);
    }
}
