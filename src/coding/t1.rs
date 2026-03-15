// Phase 300b: Tier-1 coding (C: opj_t1_t)
//
// Encodes/decodes code-block coefficients using context-based MQ arithmetic coding.
// Three coding passes per bitplane: Significance, Refinement, Clean-up.

use crate::error::Result;
use crate::types::*;

/// T1 workspace (C: opj_t1_t).
///
/// Holds coefficient data and significance flags for one code-block.
/// Encoder data is in "zigzag" layout (4-row strips, column-first).
/// Decoder data is in row-major layout.
pub struct T1 {
    /// Coefficient data (w * h elements).
    pub data: Vec<i32>,
    /// Significance/sign/refinement/PI flags.
    /// Layout: (flags_height + 2) rows × flags_stride columns, with 1-element border.
    pub flags: Vec<u32>,
    pub w: u32,
    pub h: u32,
    pub encoder: bool,
    /// Orient-specific offset into LUT_CTXNO_ZC: orient << 9.
    pub lut_ctxno_zc_orient_offset: usize,
}

impl T1 {
    /// Create a new T1 workspace (C: opj_t1_create).
    pub fn new(is_encoder: bool) -> Self {
        Self {
            data: Vec::new(),
            flags: Vec::new(),
            w: 0,
            h: 0,
            encoder: is_encoder,
            lut_ctxno_zc_orient_offset: 0,
        }
    }

    /// Allocate/reallocate data and flags buffers (C: opj_t1_allocate_buffers).
    ///
    /// Code-block dimensions are limited to 1024×1024 with w*h ≤ 4096.
    /// Data is zeroed. Flags array includes 1-element border rows (top/bottom)
    /// with PI bits set to prevent passes from processing border entries.
    /// Partial strips (when h is not a multiple of 4) have PI bits set for
    /// unused sub-rows.
    pub fn allocate_buffers(&mut self, w: u32, h: u32) -> Result<()> {
        debug_assert!(w <= 1024 && h <= 1024 && w * h <= 4096);

        let datasize = (w * h) as usize;
        let flags_stride = w as usize + 2;
        let flags_height = h.div_ceil(4) as usize;
        let flagssize = (flags_height + 2) * flags_stride;

        // Allocate data
        self.data.clear();
        self.data.resize(datasize, 0);

        // Allocate and zero flags
        self.flags.clear();
        self.flags.resize(flagssize, 0);

        let pi_all = T1_PI_0 | T1_PI_1 | T1_PI_2 | T1_PI_3;

        // Top border row: set PI bits to block all passes
        for x in 0..flags_stride {
            self.flags[x] = pi_all;
        }

        // Bottom border row
        let bottom_start = (flags_height + 1) * flags_stride;
        for x in 0..flags_stride {
            self.flags[bottom_start + x] = pi_all;
        }

        // Partial strip: set PI bits for unused sub-rows
        if !h.is_multiple_of(4) {
            let v = match h % 4 {
                1 => T1_PI_1 | T1_PI_2 | T1_PI_3,
                2 => T1_PI_2 | T1_PI_3,
                3 => T1_PI_3,
                _ => unreachable!(),
            };
            let partial_start = flags_height * flags_stride;
            for x in 0..flags_stride {
                self.flags[partial_start + x] = v;
            }
        }

        self.w = w;
        self.h = h;

        Ok(())
    }

    /// Flags array stride: w + 2 (1-element border on each side).
    #[inline]
    pub fn flags_stride(&self) -> usize {
        self.w as usize + 2
    }

    /// Flags index for column x, row y (C: T1_FLAGS(x, y)).
    #[inline]
    pub fn flags_index(&self, x: u32, y: u32) -> usize {
        x as usize + 1 + ((y as usize / 4) + 1) * self.flags_stride()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_encoder() {
        let t1 = T1::new(true);
        assert!(t1.encoder);
        assert_eq!(t1.w, 0);
        assert_eq!(t1.h, 0);
        assert!(t1.data.is_empty());
        assert!(t1.flags.is_empty());
        assert_eq!(t1.lut_ctxno_zc_orient_offset, 0);
    }

    #[test]
    fn new_decoder() {
        let t1 = T1::new(false);
        assert!(!t1.encoder);
    }

    #[test]
    fn allocate_4x4() {
        let mut t1 = T1::new(true);
        t1.allocate_buffers(4, 4).unwrap();
        assert_eq!(t1.w, 4);
        assert_eq!(t1.h, 4);
        assert_eq!(t1.data.len(), 16);
        // flags_stride = 4+2 = 6
        // flags_height = (4+3)/4 = 1
        // flagssize = (1+2) * 6 = 18
        assert_eq!(t1.flags.len(), 18);
    }

    #[test]
    fn allocate_8x8() {
        let mut t1 = T1::new(false);
        t1.allocate_buffers(8, 8).unwrap();
        assert_eq!(t1.w, 8);
        assert_eq!(t1.h, 8);
        assert_eq!(t1.data.len(), 64);
        // flags_stride = 10, flags_height = 2, flagssize = 4 * 10 = 40
        assert_eq!(t1.flags.len(), 40);
    }

    #[test]
    fn allocate_clears_data() {
        let mut t1 = T1::new(false);
        t1.allocate_buffers(8, 8).unwrap();
        assert!(t1.data.iter().all(|&x| x == 0));
    }

    #[test]
    fn allocate_border_flags_top_bottom() {
        let mut t1 = T1::new(true);
        t1.allocate_buffers(4, 8).unwrap();
        let stride = t1.flags_stride();
        let pi_all = T1_PI_0 | T1_PI_1 | T1_PI_2 | T1_PI_3;

        // Top border row (row 0 in flags array)
        for x in 0..stride {
            assert_eq!(t1.flags[x], pi_all, "top border at x={x}");
        }

        // Bottom border row: flags_height = (8+3)/4 = 2, bottom row index = 2+1 = 3
        let flags_height = 2usize;
        let bottom_start = (flags_height + 1) * stride;
        for x in 0..stride {
            assert_eq!(t1.flags[bottom_start + x], pi_all, "bottom border at x={x}");
        }
    }

    #[test]
    fn allocate_interior_flags_cleared() {
        let mut t1 = T1::new(true);
        t1.allocate_buffers(4, 8).unwrap();
        let stride = t1.flags_stride();
        // Interior data rows (row 1 and 2) should be zero
        for row in 1..=2 {
            for x in 0..stride {
                assert_eq!(t1.flags[row * stride + x], 0, "row={row} x={x}");
            }
        }
    }

    #[test]
    fn allocate_partial_strip_h5() {
        // h=5: 1 full strip (rows 0-3), 1 partial strip (row 4 only)
        let mut t1 = T1::new(true);
        t1.allocate_buffers(4, 5).unwrap();
        let stride = t1.flags_stride();
        // flags_height = (5+3)/4 = 2
        // Partial strip (row index 2 in flags): only row 0 of 4 is valid
        // PI_1, PI_2, PI_3 should be set to mark unused sub-rows
        let partial_start = 2 * stride;
        let pi_unused = T1_PI_1 | T1_PI_2 | T1_PI_3;
        for x in 0..stride {
            assert_eq!(
                t1.flags[partial_start + x] & pi_unused,
                pi_unused,
                "partial strip at x={x}"
            );
        }
    }

    #[test]
    fn allocate_partial_strip_h6() {
        // h=6: 1 full strip (rows 0-3), 1 partial strip (rows 4-5)
        let mut t1 = T1::new(true);
        t1.allocate_buffers(4, 6).unwrap();
        let stride = t1.flags_stride();
        let partial_start = 2 * stride;
        // 2 valid rows, PI_2 and PI_3 should be set
        let pi_unused = T1_PI_2 | T1_PI_3;
        for x in 0..stride {
            assert_eq!(
                t1.flags[partial_start + x] & pi_unused,
                pi_unused,
                "partial strip h=6 at x={x}"
            );
            // PI_0 and PI_1 should NOT be set (valid rows)
            assert_eq!(
                t1.flags[partial_start + x] & (T1_PI_0 | T1_PI_1),
                0,
                "partial strip h=6 PI_0/PI_1 at x={x}"
            );
        }
    }

    #[test]
    fn allocate_partial_strip_h7() {
        // h=7: 1 full strip, 1 partial with 3 valid rows -> PI_3 set
        let mut t1 = T1::new(true);
        t1.allocate_buffers(4, 7).unwrap();
        let stride = t1.flags_stride();
        let partial_start = 2 * stride;
        for x in 0..stride {
            assert_eq!(
                t1.flags[partial_start + x] & T1_PI_3,
                T1_PI_3,
                "partial strip h=7 at x={x}"
            );
            assert_eq!(
                t1.flags[partial_start + x] & (T1_PI_0 | T1_PI_1 | T1_PI_2),
                0,
                "partial strip h=7 valid rows at x={x}"
            );
        }
    }

    #[test]
    fn flags_index_matches_c_macro() {
        // C: T1_FLAGS(x, y) = flags[x + 1 + ((y/4) + 1) * (w+2)]
        let mut t1 = T1::new(true);
        t1.allocate_buffers(8, 8).unwrap();
        // w=8, flags_stride = 10
        // T1_FLAGS(0, 0) = 0 + 1 + 1*10 = 11
        assert_eq!(t1.flags_index(0, 0), 11);
        // T1_FLAGS(3, 0) = 3 + 1 + 1*10 = 14
        assert_eq!(t1.flags_index(3, 0), 14);
        // T1_FLAGS(0, 4) = 0 + 1 + 2*10 = 21
        assert_eq!(t1.flags_index(0, 4), 21);
        // T1_FLAGS(7, 7) = 7 + 1 + 2*10 = 28
        assert_eq!(t1.flags_index(7, 7), 28);
    }

    #[test]
    fn allocate_reuse_larger() {
        // Second allocation with same or smaller size should reuse
        let mut t1 = T1::new(true);
        t1.allocate_buffers(8, 8).unwrap();
        // Re-allocate with smaller size
        t1.allocate_buffers(4, 4).unwrap();
        assert_eq!(t1.w, 4);
        assert_eq!(t1.h, 4);
        assert_eq!(t1.data.len(), 16);
    }
}
