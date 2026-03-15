// Multi-component transform (C: mct.c)

use crate::error::{Error, Result};
use crate::types::int_fix_mul;

/// RCT normalization coefficients (C: opj_mct_norms).
pub static MCT_NORMS: [f64; 3] = [1.732, 0.8292, 0.8292];

/// ICT normalization coefficients (C: opj_mct_norms_real).
pub static MCT_NORMS_REAL: [f64; 3] = [1.732, 1.805, 1.573];

/// Forward reversible MCT (RCT) (C: opj_mct_encode).
/// Y = (R + 2G + B) >> 2, Cb = B - G, Cr = R - G
pub fn mct_encode(c0: &mut [i32], c1: &mut [i32], c2: &mut [i32]) {
    debug_assert_eq!(c0.len(), c1.len());
    debug_assert_eq!(c1.len(), c2.len());
    let n = c0.len().min(c1.len()).min(c2.len());
    for i in 0..n {
        let r = c0[i];
        let g = c1[i];
        let b = c2[i];
        c0[i] = (r + (g * 2) + b) >> 2;
        c1[i] = b - g;
        c2[i] = r - g;
    }
}

/// Inverse reversible MCT (RCT) (C: opj_mct_decode).
/// G = Y - (Cb + Cr) >> 2, R = Cr + G, B = Cb + G
pub fn mct_decode(c0: &mut [i32], c1: &mut [i32], c2: &mut [i32]) {
    debug_assert_eq!(c0.len(), c1.len());
    debug_assert_eq!(c1.len(), c2.len());
    let n = c0.len().min(c1.len()).min(c2.len());
    for i in 0..n {
        let y = c0[i];
        let u = c1[i];
        let v = c2[i];
        let g = y - ((u + v) >> 2);
        c0[i] = v + g;
        c1[i] = g;
        c2[i] = u + g;
    }
}

/// Forward irreversible MCT (ICT) (C: opj_mct_encode_real).
pub fn mct_encode_real(c0: &mut [f32], c1: &mut [f32], c2: &mut [f32]) {
    debug_assert_eq!(c0.len(), c1.len());
    debug_assert_eq!(c1.len(), c2.len());
    let n = c0.len().min(c1.len()).min(c2.len());
    for i in 0..n {
        let r = c0[i];
        let g = c1[i];
        let b = c2[i];
        c0[i] = 0.299f32 * r + 0.587f32 * g + 0.114f32 * b;
        c1[i] = -0.16875f32 * r - 0.331260f32 * g + 0.5f32 * b;
        c2[i] = 0.5f32 * r - 0.41869f32 * g - 0.08131f32 * b;
    }
}

/// Inverse irreversible MCT (ICT) (C: opj_mct_decode_real).
pub fn mct_decode_real(c0: &mut [f32], c1: &mut [f32], c2: &mut [f32]) {
    debug_assert_eq!(c0.len(), c1.len());
    debug_assert_eq!(c1.len(), c2.len());
    let n = c0.len().min(c1.len()).min(c2.len());
    for i in 0..n {
        let y = c0[i];
        let u = c1[i];
        let v = c2[i];
        c0[i] = y + v * 1.402f32;
        c1[i] = y - u * 0.34413f32 - v * 0.71414f32;
        c2[i] = y + u * 1.772f32;
    }
}

/// Get RCT normalization coefficient (C: opj_mct_getnorm).
pub fn mct_getnorm(compno: u32) -> f64 {
    debug_assert!((compno as usize) < MCT_NORMS.len());
    MCT_NORMS[compno as usize]
}

/// Get ICT normalization coefficient (C: opj_mct_getnorm_real).
pub fn mct_getnorm_real(compno: u32) -> f64 {
    debug_assert!((compno as usize) < MCT_NORMS_REAL.len());
    MCT_NORMS_REAL[compno as usize]
}

/// Forward custom MCT (C: opj_mct_encode_custom).
/// Matrix is nb_comps×nb_comps in row-major order, applied as fixed-point multiply.
#[allow(clippy::needless_range_loop)]
pub fn mct_encode_custom(matrix: &[f32], data: &mut [&mut [i32]], n: usize) -> Result<()> {
    let nb_comps = data.len();
    if matrix.len() < nb_comps * nb_comps {
        return Err(Error::InvalidInput("matrix too small".into()));
    }
    for comp in data.iter() {
        if comp.len() < n {
            return Err(Error::InvalidInput("component slice too short".into()));
        }
    }
    let multiplier = 1 << 13;
    let int_matrix: Vec<i32> = matrix
        .iter()
        .map(|&v| (v * multiplier as f32) as i32)
        .collect();
    let mut current = vec![0i32; nb_comps];

    for i in 0..n {
        for j in 0..nb_comps {
            current[j] = data[j][i];
        }
        for j in 0..nb_comps {
            let mut sum = 0i32;
            for k in 0..nb_comps {
                sum += int_fix_mul(int_matrix[j * nb_comps + k], current[k]);
            }
            data[j][i] = sum;
        }
    }
    Ok(())
}

/// Inverse custom MCT (C: opj_mct_decode_custom).
/// Matrix is nb_comps×nb_comps, applied as floating-point multiply.
#[allow(clippy::needless_range_loop)]
pub fn mct_decode_custom(matrix: &[f32], data: &mut [&mut [f32]], n: usize) -> Result<()> {
    let nb_comps = data.len();
    if matrix.len() < nb_comps * nb_comps {
        return Err(Error::InvalidInput("matrix too small".into()));
    }
    for comp in data.iter() {
        if comp.len() < n {
            return Err(Error::InvalidInput("component slice too short".into()));
        }
    }
    let mut current = vec![0.0f32; nb_comps];

    for i in 0..n {
        for j in 0..nb_comps {
            current[j] = data[j][i];
        }
        for j in 0..nb_comps {
            let mut sum = 0.0f32;
            for k in 0..nb_comps {
                sum += matrix[j * nb_comps + k] * current[k];
            }
            data[j][i] = sum;
        }
    }
    Ok(())
}

/// Calculate column L2 norms of a matrix (C: opj_calculate_norms).
pub fn calculate_norms(norms: &mut [f64], matrix: &[f32], nb_comps: usize) {
    debug_assert!(norms.len() >= nb_comps);
    debug_assert!(matrix.len() >= nb_comps * nb_comps);
    for i in 0..nb_comps {
        let mut sum = 0.0f64;
        for j in 0..nb_comps {
            let val = matrix[j * nb_comps + i] as f64;
            sum += val * val;
        }
        norms[i] = sum.sqrt();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn norms_values() {
        assert!((MCT_NORMS[0] - 1.732).abs() < 1e-10);
        assert!((MCT_NORMS[1] - 0.8292).abs() < 1e-10);
        assert!((MCT_NORMS[2] - 0.8292).abs() < 1e-10);
        assert!((MCT_NORMS_REAL[0] - 1.732).abs() < 1e-10);
        assert!((MCT_NORMS_REAL[1] - 1.805).abs() < 1e-10);
        assert!((MCT_NORMS_REAL[2] - 1.573).abs() < 1e-10);
    }

    #[test]
    fn getnorm_returns_correct_values() {
        assert_eq!(mct_getnorm(0), 1.732);
        assert_eq!(mct_getnorm(1), 0.8292);
        assert_eq!(mct_getnorm_real(0), 1.732);
        assert_eq!(mct_getnorm_real(2), 1.573);
    }

    #[test]
    fn rct_roundtrip_lossless() {
        let mut c0 = vec![100i32, 200, 50, 255];
        let mut c1 = vec![150, 100, 200, 128];
        let mut c2 = vec![80, 50, 180, 64];
        let orig0 = c0.clone();
        let orig1 = c1.clone();
        let orig2 = c2.clone();
        mct_encode(&mut c0, &mut c1, &mut c2);
        mct_decode(&mut c0, &mut c1, &mut c2);
        assert_eq!(c0, orig0);
        assert_eq!(c1, orig1);
        assert_eq!(c2, orig2);
    }

    #[test]
    fn rct_encode_known_values() {
        // R=100, G=150, B=80
        // Y = (100 + 300 + 80) >> 2 = 480 >> 2 = 120
        // Cb = 80 - 150 = -70
        // Cr = 100 - 150 = -50
        let mut c0 = vec![100i32];
        let mut c1 = vec![150];
        let mut c2 = vec![80];
        mct_encode(&mut c0, &mut c1, &mut c2);
        assert_eq!(c0[0], 120);
        assert_eq!(c1[0], -70);
        assert_eq!(c2[0], -50);
    }

    #[test]
    fn ict_roundtrip_within_tolerance() {
        let mut c0 = vec![100.0f32, 200.0, 50.0];
        let mut c1 = vec![150.0, 100.0, 200.0];
        let mut c2 = vec![80.0, 50.0, 180.0];
        let orig0 = c0.clone();
        let orig1 = c1.clone();
        let orig2 = c2.clone();
        mct_encode_real(&mut c0, &mut c1, &mut c2);
        mct_decode_real(&mut c0, &mut c1, &mut c2);
        for i in 0..3 {
            assert!(
                (c0[i] - orig0[i]).abs() < 0.01,
                "c0[{}]: {} vs {}",
                i,
                c0[i],
                orig0[i]
            );
            assert!(
                (c1[i] - orig1[i]).abs() < 0.01,
                "c1[{}]: {} vs {}",
                i,
                c1[i],
                orig1[i]
            );
            assert!(
                (c2[i] - orig2[i]).abs() < 0.01,
                "c2[{}]: {} vs {}",
                i,
                c2[i],
                orig2[i]
            );
        }
    }

    #[test]
    fn custom_mct_identity_is_noop() {
        #[rustfmt::skip]
        let identity = [
            1.0f32, 0.0, 0.0,
            0.0, 1.0, 0.0,
            0.0, 0.0, 1.0,
        ];
        let mut d0 = vec![10i32, 20, 30];
        let mut d1 = vec![40, 50, 60];
        let mut d2 = vec![70, 80, 90];
        let orig0 = d0.clone();
        let orig1 = d1.clone();
        let orig2 = d2.clone();
        {
            let mut data: Vec<&mut [i32]> = vec![&mut d0, &mut d1, &mut d2];
            mct_encode_custom(&identity, &mut data, 3).unwrap();
        }
        assert_eq!(d0, orig0);
        assert_eq!(d1, orig1);
        assert_eq!(d2, orig2);
    }

    #[test]
    fn calculate_norms_identity() {
        #[rustfmt::skip]
        let matrix = [
            1.0f32, 0.0, 0.0,
            0.0, 1.0, 0.0,
            0.0, 0.0, 1.0,
        ];
        let mut norms = [0.0f64; 3];
        calculate_norms(&mut norms, &matrix, 3);
        for n in &norms {
            assert!((*n - 1.0).abs() < 1e-10);
        }
    }
}
