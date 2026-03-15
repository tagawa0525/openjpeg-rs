// Discrete Wavelet Transform (C: dwt.c)
// Scalar-only implementation (no SIMD, no thread pool).

use crate::error::Result;

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

/// One lifting step on interleaved f32 data.
///
/// For each i in 0..count:
///   data[target_off + 2*i] += (left + right) * c
/// where left  = data[nbr_off + 2*clamp(i + left_delta, 0, nbr_count-1)]
///       right = data[nbr_off + 2*clamp(i + right_delta, 0, nbr_count-1)]
#[inline]
#[allow(clippy::too_many_arguments)]
fn lift_step_97(
    data: &mut [f32],
    target_off: usize,
    count: usize,
    nbr_off: usize,
    nbr_count: usize,
    left_delta: isize,
    right_delta: isize,
    c: f32,
) {
    let max_idx = nbr_count as isize - 1;
    for i in 0..count {
        let li = (i as isize + left_delta).clamp(0, max_idx) as usize;
        let ri = (i as isize + right_delta).clamp(0, max_idx) as usize;
        let left = data[nbr_off + 2 * li];
        let right = data[nbr_off + 2 * ri];
        data[target_off + 2 * i] += (left + right) * c;
    }
}

/// Forward 1D 9-7 lifting (in-place on interleaved data).
pub fn dwt_encode_1_97(data: &mut [f32], sn: usize, dn: usize, cas: bool) {
    if sn + dn <= 1 {
        return;
    }
    if !cas {
        // cas=0: s at even (off=0), d at odd (off=1)
        lift_step_97(data, 1, dn, 0, sn, 0, 1, DWT_ALPHA);
        lift_step_97(data, 0, sn, 1, dn, -1, 0, DWT_BETA);
        lift_step_97(data, 1, dn, 0, sn, 0, 1, DWT_GAMMA);
        lift_step_97(data, 0, sn, 1, dn, -1, 0, DWT_DELTA);
        for i in 0..sn {
            data[2 * i] *= DWT_INV_K;
        }
        for i in 0..dn {
            data[2 * i + 1] *= DWT_K;
        }
    } else {
        // cas=1: d at even (off=0), s at odd (off=1)
        lift_step_97(data, 0, dn, 1, sn, -1, 0, DWT_ALPHA);
        lift_step_97(data, 1, sn, 0, dn, 0, 1, DWT_BETA);
        lift_step_97(data, 0, dn, 1, sn, -1, 0, DWT_GAMMA);
        lift_step_97(data, 1, sn, 0, dn, 0, 1, DWT_DELTA);
        for i in 0..dn {
            data[2 * i] *= DWT_K;
        }
        for i in 0..sn {
            data[2 * i + 1] *= DWT_INV_K;
        }
    }
}

/// Inverse 1D 9-7 lifting (in-place on interleaved data).
pub fn dwt_decode_1_97(data: &mut [f32], sn: usize, dn: usize, cas: bool) {
    if sn + dn <= 1 {
        return;
    }
    if !cas {
        // cas=0: s at even (off=0), d at odd (off=1)
        for i in 0..sn {
            data[2 * i] *= DWT_K;
        }
        for i in 0..dn {
            data[2 * i + 1] *= DWT_INV_K;
        }
        lift_step_97(data, 0, sn, 1, dn, -1, 0, -DWT_DELTA);
        lift_step_97(data, 1, dn, 0, sn, 0, 1, -DWT_GAMMA);
        lift_step_97(data, 0, sn, 1, dn, -1, 0, -DWT_BETA);
        lift_step_97(data, 1, dn, 0, sn, 0, 1, -DWT_ALPHA);
    } else {
        // cas=1: d at even (off=0), s at odd (off=1)
        for i in 0..dn {
            data[2 * i] *= DWT_INV_K;
        }
        for i in 0..sn {
            data[2 * i + 1] *= DWT_K;
        }
        lift_step_97(data, 1, sn, 0, dn, 0, 1, -DWT_DELTA);
        lift_step_97(data, 0, dn, 1, sn, -1, 0, -DWT_GAMMA);
        lift_step_97(data, 1, sn, 0, dn, 0, 1, -DWT_BETA);
        lift_step_97(data, 0, dn, 1, sn, -1, 0, -DWT_ALPHA);
    }
}

/// Horizontal deinterleave: split interleaved `[s0, d0, s1, d1, ...]` into
/// `[s0, s1, ..., d0, d1, ...]` (separated low/high subbands).
///
/// `src`: interleaved input, `dst`: separated output.
/// `sn`: low-pass count, `dn`: high-pass count, `cas`: subgrid parity.
pub fn deinterleave_h<T: Copy>(src: &[T], dst: &mut [T], sn: usize, dn: usize, cas: bool) {
    let s_start = if cas { 1 } else { 0 }; // low-pass starts at this src offset
    let d_start = 1 - s_start; // high-pass starts at this src offset
    for i in 0..sn {
        dst[i] = src[s_start + 2 * i];
    }
    for i in 0..dn {
        dst[sn + i] = src[d_start + 2 * i];
    }
}

/// Horizontal interleave: merge separated `[s0, s1, ..., d0, d1, ...]` back
/// into interleaved `[s0, d0, s1, d1, ...]`.
pub fn interleave_h<T: Copy>(src: &[T], dst: &mut [T], sn: usize, dn: usize, cas: bool) {
    let s_start = if cas { 1 } else { 0 };
    let d_start = 1 - s_start;
    for i in 0..sn {
        dst[s_start + 2 * i] = src[i];
    }
    for i in 0..dn {
        dst[d_start + 2 * i] = src[sn + i];
    }
}

/// Vertical deinterleave: split interleaved column data (with stride) into
/// separated low/high subbands.
pub fn deinterleave_v<T: Copy>(
    src: &[T],
    dst: &mut [T],
    sn: usize,
    dn: usize,
    cas: bool,
    stride: usize,
) {
    let s_row_start = if cas { stride } else { 0 };
    let d_row_start = if cas { 0 } else { stride };
    for i in 0..sn {
        dst[i] = src[s_row_start + 2 * i * stride];
    }
    for i in 0..dn {
        dst[sn + i] = src[d_row_start + 2 * i * stride];
    }
}

/// Vertical interleave: merge separated subbands back into strided column data.
pub fn interleave_v<T: Copy>(
    src: &[T],
    dst: &mut [T],
    sn: usize,
    dn: usize,
    cas: bool,
    stride: usize,
) {
    let s_row_start = if cas { stride } else { 0 };
    let d_row_start = if cas { 0 } else { stride };
    for i in 0..sn {
        dst[s_row_start + 2 * i * stride] = src[i];
    }
    for i in 0..dn {
        dst[d_row_start + 2 * i * stride] = src[sn + i];
    }
}

/// Forward 2D 5-3 DWT (C: opj_dwt_encode).
///
/// `data`: row-major tile data, `w`×`h` pixels with row stride `stride`.
/// `num_res`: number of resolution levels (num_res-1 decomposition levels).
/// Processes from finest to coarsest: each level applies vertical then horizontal
/// transform on the current LL subband, producing LL/LH/HL/HH subbands.
pub fn dwt_encode_2d_53(
    _data: &mut [i32],
    _w: usize,
    _h: usize,
    _stride: usize,
    _num_res: usize,
) -> Result<()> {
    todo!()
}

/// Inverse 2D 5-3 DWT (C: opj_dwt_decode).
pub fn dwt_decode_2d_53(
    _data: &mut [i32],
    _w: usize,
    _h: usize,
    _stride: usize,
    _num_res: usize,
) -> Result<()> {
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
    fn encode_1d_97_even_cas0_roundtrip() {
        let original = vec![10.0f32, 23.0, 35.0, 41.0, 58.0, 62.0, 77.0, 80.0];
        let mut data = original.clone();
        dwt_encode_1_97(&mut data, 4, 4, false);
        dwt_decode_1_97(&mut data, 4, 4, false);
        assert_f32_eq(&data, &original, 1e-4);
    }

    #[test]
    fn encode_1d_97_odd_cas0_roundtrip() {
        let original = vec![10.0f32, 23.0, 35.0, 41.0, 58.0, 62.0, 77.0];
        let mut data = original.clone();
        dwt_encode_1_97(&mut data, 4, 3, false);
        dwt_decode_1_97(&mut data, 4, 3, false);
        assert_f32_eq(&data, &original, 1e-4);
    }

    #[test]
    fn encode_1d_97_cas1_roundtrip() {
        let original = vec![100.0f32, 200.0, 300.0, 400.0, 500.0, 600.0];
        let mut data = original.clone();
        dwt_encode_1_97(&mut data, 3, 3, true);
        dwt_decode_1_97(&mut data, 3, 3, true);
        assert_f32_eq(&data, &original, 1e-4);
    }

    #[test]
    fn encode_1d_97_length_1_noop() {
        let mut data = vec![42.0f32];
        dwt_encode_1_97(&mut data, 1, 0, false);
        assert_eq!(data[0], 42.0);
    }

    #[test]
    fn encode_1d_97_length_2_roundtrip() {
        let original = vec![100.0f32, 200.0];
        let mut data = original.clone();
        dwt_encode_1_97(&mut data, 1, 1, false);
        dwt_decode_1_97(&mut data, 1, 1, false);
        assert_f32_eq(&data, &original, 1e-4);
    }

    // ==================== Deinterleave / Interleave tests ====================

    #[test]
    fn deinterleave_h_cas0() {
        // [s0, d0, s1, d1, s2, d2] → [s0, s1, s2, d0, d1, d2]
        let src = [10, 20, 30, 40, 50, 60];
        let mut dst = [0i32; 6];
        deinterleave_h(&src, &mut dst, 3, 3, false);
        assert_eq!(dst, [10, 30, 50, 20, 40, 60]);
    }

    #[test]
    fn deinterleave_h_cas1() {
        // cas=1: [d0, s0, d1, s1, d2, s2] → [s0, s1, s2, d0, d1, d2]
        let src = [10, 20, 30, 40, 50, 60];
        let mut dst = [0i32; 6];
        deinterleave_h(&src, &mut dst, 3, 3, true);
        assert_eq!(dst, [20, 40, 60, 10, 30, 50]);
    }

    #[test]
    fn interleave_h_cas0() {
        // [s0, s1, s2, d0, d1, d2] → [s0, d0, s1, d1, s2, d2]
        let src = [10, 30, 50, 20, 40, 60];
        let mut dst = [0i32; 6];
        interleave_h(&src, &mut dst, 3, 3, false);
        assert_eq!(dst, [10, 20, 30, 40, 50, 60]);
    }

    #[test]
    fn deinterleave_interleave_h_roundtrip() {
        let original = [10, 20, 30, 40, 50, 60, 70];
        let mut separated = [0i32; 7];
        let mut restored = [0i32; 7];
        deinterleave_h(&original, &mut separated, 4, 3, false);
        interleave_h(&separated, &mut restored, 4, 3, false);
        assert_eq!(restored, original);
    }

    #[test]
    fn deinterleave_v_cas0() {
        // Column data with stride=4:
        // row 0: [s0, ...]  (even row → low-pass)
        // row 1: [d0, ...]  (odd row → high-pass)
        // row 2: [s1, ...]
        // row 3: [d1, ...]
        // Output: [s0, s1, d0, d1]
        let src = [10, 0, 0, 0, 20, 0, 0, 0, 30, 0, 0, 0, 40, 0, 0, 0];
        let mut dst = [0i32; 4];
        deinterleave_v(&src, &mut dst, 2, 2, false, 4);
        assert_eq!(dst, [10, 30, 20, 40]);
    }

    #[test]
    fn interleave_v_cas0() {
        // [s0, s1, d0, d1] → column with stride=4
        let src = [10, 30, 20, 40];
        let mut dst = [0i32; 16];
        interleave_v(&src, &mut dst, 2, 2, false, 4);
        assert_eq!(dst[0], 10);
        assert_eq!(dst[4], 20);
        assert_eq!(dst[8], 30);
        assert_eq!(dst[12], 40);
    }

    // ==================== 2D 5-3 tests ====================

    #[test]
    #[ignore = "not yet implemented"]
    fn encode_2d_53_4x4_roundtrip() {
        // 4×4 non-linear data, 2 resolution levels (1 decomposition)
        #[rustfmt::skip]
        let original = vec![
            10, 23, 35, 41,
            58, 62, 77, 80,
            15, 28, 42, 53,
            67, 71, 88, 95,
        ];
        let mut data = original.clone();
        dwt_encode_2d_53(&mut data, 4, 4, 4, 2).unwrap();
        assert_ne!(data, original);
        dwt_decode_2d_53(&mut data, 4, 4, 4, 2).unwrap();
        assert_eq!(data, original);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn encode_2d_53_8x8_multi_level_roundtrip() {
        // 8×8 data, 3 resolution levels (2 decompositions)
        let mut original = vec![0i32; 64];
        for (i, v) in original.iter_mut().enumerate() {
            *v = (i as i32 * 7 + 13) % 256;
        }
        let mut data = original.clone();
        dwt_encode_2d_53(&mut data, 8, 8, 8, 3).unwrap();
        assert_ne!(data, original);
        dwt_decode_2d_53(&mut data, 8, 8, 8, 3).unwrap();
        assert_eq!(data, original);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn encode_2d_53_odd_size_roundtrip() {
        // 5×3 data, 2 resolution levels
        #[rustfmt::skip]
        let original = vec![
            10, 20, 30, 40, 50,
            60, 70, 80, 90, 100,
            110, 120, 130, 140, 150,
        ];
        let mut data = original.clone();
        dwt_encode_2d_53(&mut data, 5, 3, 5, 2).unwrap();
        dwt_decode_2d_53(&mut data, 5, 3, 5, 2).unwrap();
        assert_eq!(data, original);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn encode_2d_53_single_res_noop() {
        // num_res=1 means no decomposition, should be a no-op
        let original = vec![10, 20, 30, 40];
        let mut data = original.clone();
        dwt_encode_2d_53(&mut data, 2, 2, 2, 1).unwrap();
        assert_eq!(data, original);
    }
}
