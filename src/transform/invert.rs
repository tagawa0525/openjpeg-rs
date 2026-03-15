// Matrix inversion via LUP decomposition (C: invert.c)

use crate::error::{Error, Result};

/// Invert an n×n matrix using LUP decomposition (C: opj_matrix_inversion_f).
///
/// `src` is modified in-place during decomposition.
/// `dst` receives the inverse matrix.
/// Returns `Err` if the matrix is singular.
pub fn matrix_inversion_f(src: &mut [f32], dst: &mut [f32], n: usize) -> Result<()> {
    let mut permutations = vec![0u32; n];
    if !lup_decompose(src, &mut permutations, n) {
        return Err(Error::InvalidInput("singular matrix".into()));
    }
    lup_invert(src, dst, n, &permutations);
    Ok(())
}

/// LUP decomposition with partial pivoting (C: opj_lupDecompose).
/// Returns false if the matrix is singular.
fn lup_decompose(matrix: &mut [f32], permutations: &mut [u32], n: usize) -> bool {
    // Initialize permutations to identity
    for (i, perm) in permutations.iter_mut().enumerate() {
        *perm = i as u32;
    }

    for k in 0..n.saturating_sub(1) {
        // Find pivot (largest absolute value in column k, rows k..n)
        let mut p = 0.0f32;
        let mut k2 = k;
        for i in k..n {
            let val = matrix[i * n + k].abs();
            if val > p {
                p = val;
                k2 = i;
            }
        }

        if p == 0.0 {
            return false;
        }

        // Swap rows k and k2
        if k2 != k {
            permutations.swap(k, k2);
            for j in 0..n {
                let (a, b) = (k * n + j, k2 * n + j);
                matrix.swap(a, b);
            }
        }

        // Elimination
        let pivot = matrix[k * n + k];
        for i in (k + 1)..n {
            matrix[i * n + k] /= pivot;
            let factor = matrix[i * n + k];
            for j in (k + 1)..n {
                matrix[i * n + j] -= factor * matrix[k * n + j];
            }
        }
    }

    // Check last diagonal element (not covered by the loop)
    if n > 0 && matrix[(n - 1) * n + (n - 1)] == 0.0 {
        return false;
    }

    true
}

/// Solve Ly=Pb (forward) then Ux=y (backward) (C: opj_lupSolve).
fn lup_solve(result: &mut [f32], matrix: &[f32], vector: &[f32], permutations: &[u32], n: usize) {
    let mut intermediate = vec![0.0f32; n];

    // Forward substitution: Ly = Pb
    for i in 0..n {
        let mut sum = 0.0f32;
        for j in 0..i {
            sum += matrix[i * n + j] * intermediate[j];
        }
        intermediate[i] = vector[permutations[i] as usize] - sum;
    }

    // Backward substitution: Ux = y
    for k in (0..n).rev() {
        let mut sum = 0.0f32;
        for j in (k + 1)..n {
            sum += matrix[k * n + j] * result[j];
        }
        result[k] = (intermediate[k] - sum) / matrix[k * n + k];
    }
}

/// Compute inverse by solving for each column (C: opj_lupInvert).
fn lup_invert(src_matrix: &[f32], dst_matrix: &mut [f32], n: usize, permutations: &[u32]) {
    let mut unit_col = vec![0.0f32; n];
    let mut result_col = vec![0.0f32; n];

    for j in 0..n {
        // Set up unit vector for column j
        unit_col.fill(0.0);
        unit_col[j] = 1.0;

        result_col.fill(0.0);
        lup_solve(&mut result_col, src_matrix, &unit_col, permutations, n);

        // Copy result into column j of destination (row-major)
        for i in 0..n {
            dst_matrix[i * n + j] = result_col[i];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
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
    fn known_3x3_inverse() {
        // [[1, 2, 3], [0, 1, 4], [5, 6, 0]]
        let mut src = [1.0f32, 2.0, 3.0, 0.0, 1.0, 4.0, 5.0, 6.0, 0.0];
        let mut dst = [0.0f32; 9];
        matrix_inversion_f(&mut src, &mut dst, 3).unwrap();
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
    fn roundtrip_a_times_a_inv_is_identity() {
        let original = [3.0f32, 1.0, 2.0, 1.0, 4.0, 3.0, 2.0, 5.0, 7.0];
        let mut src = original;
        let mut dst = [0.0f32; 9];
        matrix_inversion_f(&mut src, &mut dst, 3).unwrap();
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
    fn singular_matrix_returns_err() {
        let mut src = [1.0f32, 2.0, 2.0, 4.0]; // det = 0
        let mut dst = [0.0f32; 4];
        assert!(matrix_inversion_f(&mut src, &mut dst, 2).is_err());
    }

    #[test]
    fn identity_1x1() {
        let mut src = [5.0f32];
        let mut dst = [0.0f32];
        matrix_inversion_f(&mut src, &mut dst, 1).unwrap();
        assert!((dst[0] - 0.2).abs() < 1e-6);
    }
}
