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

    /// Set orient for ZC context lookup (C: mqc->lut_ctxno_zc_orient).
    #[inline]
    pub fn set_orient(&mut self, orient: u32) {
        self.lut_ctxno_zc_orient_offset = (orient as usize) << 9;
    }
}

// --- Context helper functions ---

use crate::coding::t1_luts::*;

/// Zero Coding context number (C: opj_t1_getctxno_zc).
#[inline]
pub fn getctxno_zc(orient_offset: usize, f: u32) -> u8 {
    LUT_CTXNO_ZC[orient_offset + (f & T1_SIGMA_NEIGHBOURS) as usize]
}

/// Sign context / SPB index (C: opj_t1_getctxtno_sc_or_spb_index).
///
/// Computes an 8-bit lookup index from the current flags word (fX),
/// the previous (west) neighbour flags (pfX), and the next (east) neighbour
/// flags (nfX) for sub-row ci.
#[inline]
pub fn getctxtno_sc_or_spb_index(fx: u32, pfx: u32, nfx: u32, ci: u32) -> u32 {
    let mut lu = (fx >> (ci * 3)) & (T1_SIGMA_1 | T1_SIGMA_3 | T1_SIGMA_5 | T1_SIGMA_7);

    lu |= (pfx >> (T1_CHI_1_I + ci * 3)) & (1); // W sign
    lu |= (nfx >> (T1_CHI_1_I - 2 + ci * 3)) & (1 << 2); // E sign
    if ci == 0 {
        lu |= (fx >> (T1_CHI_0_I - 4)) & (1 << 4); // N sign
    } else {
        lu |= (fx >> (T1_CHI_1_I - 4 + (ci - 1) * 3)) & (1 << 4);
    }
    lu |= (fx >> (T1_CHI_2_I - 6 + ci * 3)) & (1 << 6); // S sign
    lu
}

/// Sign Coding context number (C: opj_t1_getctxno_sc).
#[inline]
pub fn getctxno_sc(lu: u32) -> u8 {
    LUT_CTXNO_SC[lu as usize]
}

/// Magnitude context number (C: opj_t1_getctxno_mag).
#[inline]
pub fn getctxno_mag(f: u32) -> u32 {
    if (f & T1_MU_0) != 0 {
        T1_CTXNO_MAG as u32 + 2
    } else if (f & T1_SIGMA_NEIGHBOURS) != 0 {
        T1_CTXNO_MAG as u32 + 1
    } else {
        T1_CTXNO_MAG as u32
    }
}

/// Sign Prediction Bit (C: opj_t1_getspb).
#[inline]
pub fn getspb(lu: u32) -> u8 {
    LUT_SPB[lu as usize]
}

/// NMSEDEC for significance pass (C: opj_t1_getnmsedec_sig).
#[inline]
pub fn getnmsedec_sig(x: u32, bitpos: u32) -> i16 {
    if bitpos > 0 {
        LUT_NMSEDEC_SIG[(x >> bitpos) as usize & ((1 << T1_NMSEDEC_BITS) - 1)]
    } else {
        LUT_NMSEDEC_SIG0[x as usize & ((1 << T1_NMSEDEC_BITS) - 1)]
    }
}

/// NMSEDEC for refinement pass (C: opj_t1_getnmsedec_ref).
#[inline]
pub fn getnmsedec_ref(x: u32, bitpos: u32) -> i16 {
    if bitpos > 0 {
        LUT_NMSEDEC_REF[(x >> bitpos) as usize & ((1 << T1_NMSEDEC_BITS) - 1)]
    } else {
        LUT_NMSEDEC_REF0[x as usize & ((1 << T1_NMSEDEC_BITS) - 1)]
    }
}

// --- Signed Magnitude Representation helpers (C: opj_smr_abs, opj_smr_sign, opj_to_smr) ---

/// Absolute value from signed magnitude representation.
#[inline]
pub fn smr_abs(x: i32) -> u32 {
    (x as u32) & 0x7FFF_FFFF
}

/// Sign bit from signed magnitude representation (0 = positive, 1 = negative).
#[inline]
pub fn smr_sign(x: i32) -> u32 {
    (x as u32) >> 31
}

/// Convert two's complement to signed magnitude representation.
#[inline]
pub fn to_smr(x: i32) -> i32 {
    if x >= 0 {
        x
    } else {
        ((-x) as u32 | 0x8000_0000) as i32
    }
}

/// Update flags after a coefficient becomes significant (C: opj_t1_update_flags).
///
/// Sets SIGMA_THIS and CHI (sign) for the current data point, then propagates
/// significance to all 8 neighbours. `ci` is the sub-row index (0..3).
/// `vsc` disables north propagation for the top row of a VSC stripe.
#[inline]
pub fn update_flags(
    _flags: &mut [u32],
    _flagsp: usize,
    _ci: u32,
    _s: u32,
    _stride: usize,
    _vsc: bool,
) {
    todo!()
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

    // --- Context helper tests ---

    #[test]
    fn getctxno_zc_no_neighbours() {
        // No significant neighbours -> context 0 for all orients
        for orient in 0..4u32 {
            assert_eq!(getctxno_zc((orient as usize) << 9, 0), 0);
        }
    }

    #[test]
    fn getctxno_zc_known_values() {
        // Orient 0 (LL/LH), north significant (T1_SIGMA_N = bit 1)
        let f = T1_SIGMA_N; // = 0x02
        let ctx = getctxno_zc(0, f);
        assert_eq!(ctx, LUT_CTXNO_ZC[T1_SIGMA_N as usize]);

        // Orient 2 (HH), all 8 neighbours significant
        let f_all = T1_SIGMA_NEIGHBOURS;
        let ctx = getctxno_zc(2 << 9, f_all);
        assert_eq!(ctx, LUT_CTXNO_ZC[(2 << 9) + f_all as usize]);
    }

    #[test]
    fn getctxno_mag_no_neighbours_not_refined() {
        // No neighbours, no MU -> base MAG context
        assert_eq!(getctxno_mag(0), T1_CTXNO_MAG as u32);
    }

    #[test]
    fn getctxno_mag_with_neighbours() {
        // Has significant neighbour, no MU -> MAG + 1
        assert_eq!(getctxno_mag(T1_SIGMA_N), T1_CTXNO_MAG as u32 + 1);
    }

    #[test]
    fn getctxno_mag_already_refined() {
        // MU_0 set -> MAG + 2 regardless of neighbours
        assert_eq!(getctxno_mag(T1_MU_0), T1_CTXNO_MAG as u32 + 2);
        assert_eq!(getctxno_mag(T1_MU_0 | T1_SIGMA_N), T1_CTXNO_MAG as u32 + 2);
    }

    #[test]
    fn getctxno_sc_from_lut() {
        // Verify getctxno_sc delegates to LUT_CTXNO_SC
        assert_eq!(getctxno_sc(0), LUT_CTXNO_SC[0]);
        assert_eq!(getctxno_sc(0xFF), LUT_CTXNO_SC[0xFF]);
    }

    #[test]
    fn getspb_from_lut() {
        // Verify getspb delegates to LUT_SPB
        assert_eq!(getspb(0), LUT_SPB[0]);
        assert_eq!(getspb(0xFF), LUT_SPB[0xFF]);
    }

    #[test]
    fn getnmsedec_sig_bitpos_zero() {
        // bitpos=0 uses LUT_NMSEDEC_SIG0
        assert_eq!(getnmsedec_sig(0, 0), LUT_NMSEDEC_SIG0[0]);
        assert_eq!(getnmsedec_sig(42, 0), LUT_NMSEDEC_SIG0[42]);
    }

    #[test]
    fn getnmsedec_sig_bitpos_nonzero() {
        // bitpos>0 uses LUT_NMSEDEC_SIG with shifted index
        let x: u32 = 0b1010_0110;
        let bitpos: u32 = 2;
        let idx = ((x >> bitpos) as usize) & 0x7F;
        assert_eq!(getnmsedec_sig(x, bitpos), LUT_NMSEDEC_SIG[idx]);
    }

    #[test]
    fn getnmsedec_ref_bitpos_zero() {
        assert_eq!(getnmsedec_ref(0, 0), LUT_NMSEDEC_REF0[0]);
        assert_eq!(getnmsedec_ref(42, 0), LUT_NMSEDEC_REF0[42]);
    }

    #[test]
    fn getnmsedec_ref_bitpos_nonzero() {
        let x: u32 = 0b1010_0110;
        let bitpos: u32 = 2;
        let idx = ((x >> bitpos) as usize) & 0x7F;
        assert_eq!(getnmsedec_ref(x, bitpos), LUT_NMSEDEC_REF[idx]);
    }

    #[test]
    fn smr_roundtrip() {
        // Positive
        let v = to_smr(42);
        assert_eq!(smr_abs(v), 42);
        assert_eq!(smr_sign(v), 0);

        // Negative
        let v = to_smr(-42);
        assert_eq!(smr_abs(v), 42);
        assert_eq!(smr_sign(v), 1);

        // Zero
        let v = to_smr(0);
        assert_eq!(smr_abs(v), 0);
        assert_eq!(smr_sign(v), 0);
    }

    #[test]
    fn set_orient_offset() {
        let mut t1 = T1::new(true);
        t1.set_orient(2);
        assert_eq!(t1.lut_ctxno_zc_orient_offset, 2 << 9);
    }

    #[test]
    fn getctxtno_sc_or_spb_index_zero_flags() {
        // All flags zero -> lu should be 0
        assert_eq!(getctxtno_sc_or_spb_index(0, 0, 0, 0), 0);
    }

    // --- update_flags tests ---

    /// Helper: create a T1 with given dimensions, return (flags, flagsp, stride)
    fn setup_flags(w: u32, h: u32) -> (Vec<u32>, usize, usize) {
        let mut t1 = T1::new(true);
        t1.allocate_buffers(w, h).unwrap();
        let stride = t1.flags_stride();
        let flagsp = t1.flags_index(2, 0); // column 2, row 0
        (t1.flags, flagsp, stride)
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn update_flags_sets_sigma_this() {
        let (mut flags, fp, stride) = setup_flags(8, 8);
        update_flags(&mut flags, fp, 0, 0, stride, false);
        // T1_SIGMA_THIS (T1_SIGMA_4) should be set for ci=0
        assert_ne!(flags[fp] & (T1_SIGMA_4), 0);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn update_flags_sets_chi_sign() {
        let (mut flags, fp, stride) = setup_flags(8, 8);
        // s=1 (negative sign)
        update_flags(&mut flags, fp, 0, 1, stride, false);
        // CHI_1 should be set (sign=1 for ci=0)
        assert_ne!(flags[fp] & (T1_CHI_1), 0);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn update_flags_propagates_east_west() {
        let (mut flags, fp, stride) = setup_flags(8, 8);
        update_flags(&mut flags, fp, 0, 0, stride, false);
        // West neighbour (flagsp[-1]) should have T1_SIGMA_E (= T1_SIGMA_5) set
        assert_ne!(flags[fp - 1] & (T1_SIGMA_5), 0);
        // East neighbour (flagsp[+1]) should have T1_SIGMA_W (= T1_SIGMA_3) set
        assert_ne!(flags[fp + 1] & (T1_SIGMA_3), 0);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn update_flags_propagates_north() {
        let (mut flags, fp, stride) = setup_flags(8, 8);
        // ci=0, vsc=false: should propagate north
        update_flags(&mut flags, fp, 0, 0, stride, false);
        let north = fp - stride;
        // T1_SIGMA_16 (south significance in north neighbour's row)
        assert_ne!(flags[north] & T1_SIGMA_16, 0);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn update_flags_vsc_blocks_north() {
        let (mut flags, fp, stride) = setup_flags(8, 8);
        // ci=0, vsc=true: should NOT propagate north
        update_flags(&mut flags, fp, 0, 0, stride, true);
        let north = fp - stride;
        assert_eq!(flags[north] & T1_SIGMA_16, 0);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn update_flags_propagates_south() {
        let (mut flags, fp, stride) = setup_flags(8, 8);
        // ci=3: should propagate south
        update_flags(&mut flags, fp, 3, 0, stride, false);
        let south = fp + stride;
        // T1_SIGMA_1 (north significance in south neighbour's row)
        assert_ne!(flags[south] & T1_SIGMA_1, 0);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn update_flags_ci1_no_north_south() {
        let (mut flags, fp, stride) = setup_flags(8, 8);
        // ci=1: should NOT propagate to north or south neighbour rows
        update_flags(&mut flags, fp, 1, 0, stride, false);
        let north = fp - stride;
        let south = fp + stride;
        // North should be unchanged
        assert_eq!(flags[north] & T1_SIGMA_16, 0);
        // South should be unchanged
        assert_eq!(flags[south] & T1_SIGMA_1, 0);
    }
}
