// Matrix inversion via LUP decomposition (C: invert.c)

use crate::error::Result;

/// Invert an n×n matrix using LUP decomposition (C: opj_matrix_inversion_f).
///
/// `src` is modified in-place during decomposition.
/// `dst` receives the inverse matrix.
/// Returns `Err` if the matrix is singular.
pub fn matrix_inversion_f(src: &mut [f32], dst: &mut [f32], n: usize) -> Result<()> {
    let _ = (src, dst, n);
    todo!("matrix_inversion_f not yet implemented")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "not yet implemented"]
    fn identity_2x2() {
        let mut src = [1.0f32, 0.0, 0.0, 1.0];
        let mut dst = [0.0f32; 4];
        matrix_inversion_f(&mut src, &mut dst, 2).unwrap();
        assert!((dst[0] - 1.0).abs() < 1e-6);
        assert!((dst[1]).abs() < 1e-6);
        assert!((dst[2]).abs() < 1e-6);
        assert!((dst[3] - 1.0).abs() < 1e-6);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn known_2x2_inverse() {
        // [[4, 7], [2, 6]] → inverse = [[0.6, -0.7], [-0.2, 0.4]]
        let mut src = [4.0f32, 7.0, 2.0, 6.0];
        let mut dst = [0.0f32; 4];
        matrix_inversion_f(&mut src, &mut dst, 2).unwrap();
        assert!((dst[0] - 0.6).abs() < 1e-5);
        assert!((dst[1] - (-0.7)).abs() < 1e-5);
        assert!((dst[2] - (-0.2)).abs() < 1e-5);
        assert!((dst[3] - 0.4).abs() < 1e-5);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn known_3x3_inverse() {
        // [[1, 2, 3], [0, 1, 4], [5, 6, 0]]
        let mut src = [1.0f32, 2.0, 3.0, 0.0, 1.0, 4.0, 5.0, 6.0, 0.0];
        let mut dst = [0.0f32; 9];
        matrix_inversion_f(&mut src, &mut dst, 3).unwrap();
        // A * A^-1 ≈ I — verified in roundtrip test below
        let expected = [-24.0f32, 18.0, 5.0, 20.0, -15.0, -4.0, -5.0, 4.0, 1.0];
        for i in 0..9 {
            assert!(
                (dst[i] - expected[i]).abs() < 1e-4,
                "dst[{}] = {}, expected {}",
                i,
                dst[i],
                expected[i]
            );
        }
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn roundtrip_a_times_a_inv_is_identity() {
        let original = [3.0f32, 1.0, 2.0, 1.0, 4.0, 3.0, 2.0, 5.0, 7.0];
        let mut src = original;
        let mut dst = [0.0f32; 9];
        matrix_inversion_f(&mut src, &mut dst, 3).unwrap();
        // Multiply original * dst
        for i in 0..3 {
            for j in 0..3 {
                let mut sum = 0.0f32;
                for k in 0..3 {
                    sum += original[i * 3 + k] * dst[k * 3 + j];
                }
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!(
                    (sum - expected).abs() < 1e-4,
                    "product[{}][{}] = {}, expected {}",
                    i,
                    j,
                    sum,
                    expected
                );
            }
        }
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn singular_matrix_returns_err() {
        let mut src = [1.0f32, 2.0, 2.0, 4.0]; // det = 0
        let mut dst = [0.0f32; 4];
        assert!(matrix_inversion_f(&mut src, &mut dst, 2).is_err());
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn identity_1x1() {
        let mut src = [5.0f32];
        let mut dst = [0.0f32];
        matrix_inversion_f(&mut src, &mut dst, 1).unwrap();
        assert!((dst[0] - 0.2).abs() < 1e-6);
    }
}
