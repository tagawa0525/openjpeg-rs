// Phase 100: Common types, constants, and integer math

// --- Constants ---

/// Maximum number of resolution levels (C: OPJ_J2K_MAXRLVLS).
pub const J2K_MAXRLVLS: usize = 33;
/// Maximum number of sub-bands (C: OPJ_J2K_MAXBANDS).
pub const J2K_MAXBANDS: usize = 3 * J2K_MAXRLVLS - 2;
/// Default number of segments (C: OPJ_J2K_DEFAULT_NB_SEGS).
pub const J2K_DEFAULT_NB_SEGS: u32 = 10;
/// Default stream chunk size: 1 MB (C: OPJ_J2K_STREAM_CHUNK_SIZE).
pub const J2K_STREAM_CHUNK_SIZE: usize = 0x100000;
/// Maximum path length (C: OPJ_PATH_LEN).
pub const PATH_LEN: usize = 4096;
/// Basic image information flag (C: OPJ_IMG_INFO).
pub const IMG_INFO: u32 = 1;
/// Main header info flag (C: OPJ_J2K_MH_INFO).
pub const J2K_MH_INFO: u32 = 2;
/// Tile header info flag (C: OPJ_J2K_TH_INFO).
pub const J2K_TH_INFO: u32 = 4;
/// Tile/component info flag (C: OPJ_J2K_TCH_INFO).
pub const J2K_TCH_INFO: u32 = 8;
/// Main header index flag (C: OPJ_J2K_MH_IND).
pub const J2K_MH_IND: u32 = 16;
/// Tile header index flag (C: OPJ_J2K_TH_IND).
pub const J2K_TH_IND: u32 = 32;
/// JP2 info flag (C: OPJ_JP2_INFO).
pub const JP2_INFO: u32 = 128;
/// JP2 index flag (C: OPJ_JP2_IND).
pub const JP2_IND: u32 = 256;
/// Margin for fake FFFF marker (C: OPJ_COMMON_CBLK_DATA_EXTRA).
pub const COMMON_CBLK_DATA_EXTRA: usize = 2;
/// Default code block width (C: OPJ_COMP_PARAM_DEFAULT_CBLOCKW).
pub const COMP_PARAM_DEFAULT_CBLOCKW: u32 = 64;
/// Default code block height (C: OPJ_COMP_PARAM_DEFAULT_CBLOCKH).
pub const COMP_PARAM_DEFAULT_CBLOCKH: u32 = 64;
/// Default number of resolution levels (C: OPJ_COMP_PARAM_DEFAULT_NUMRESOLUTION).
pub const COMP_PARAM_DEFAULT_NUMRESOLUTION: u32 = 6;

// --- Enums ---

/// Progression order (C: OPJ_PROG_ORDER).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ProgressionOrder {
    Lrcp = 0,
    Rlcp = 1,
    Rpcl = 2,
    Pcrl = 3,
    Cprl = 4,
}

/// Color space (C: OPJ_COLOR_SPACE).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ColorSpace {
    Unknown = -1,
    Unspecified = 0,
    Srgb = 1,
    Gray = 2,
    Sycc = 3,
    Eycc = 4,
    Cmyk = 5,
}

/// Codec format (C: OPJ_CODEC_FORMAT).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum CodecFormat {
    Unknown = -1,
    J2k = 0,
    Jpt = 1,
    Jp2 = 2,
}

// --- Integer math ---

/// Ceiling division for signed integers (C: opj_int_ceildiv).
#[inline]
pub fn int_ceildiv(a: i32, b: i32) -> i32 {
    debug_assert!(b != 0, "int_ceildiv: divisor must not be zero");
    ((a as i64 + b as i64 - 1) / b as i64) as i32
}

/// Ceiling division for unsigned integers (C: opj_uint_ceildiv).
#[inline]
pub fn uint_ceildiv(a: u32, b: u32) -> u32 {
    debug_assert!(b != 0, "uint_ceildiv: divisor must not be zero");
    (a as u64).div_ceil(b as u64) as u32
}

/// Ceiling division of u64 returning u32 (C: opj_uint64_ceildiv_res_uint32).
#[inline]
pub fn uint64_ceildiv_as_u32(a: u64, b: u64) -> u32 {
    debug_assert!(b != 0, "uint64_ceildiv_as_u32: divisor must not be zero");
    a.div_ceil(b) as u32
}

/// Ceiling division by 2^b for signed integers (C: opj_int_ceildivpow2).
#[inline]
pub fn int_ceildivpow2(a: i32, b: i32) -> i32 {
    debug_assert!(
        (0..31).contains(&b),
        "int_ceildivpow2: shift must be in 0..31"
    );
    ((a as i64 + (1i64 << b) - 1) >> b) as i32
}

/// Ceiling division by 2^b for i64 (C: opj_int64_ceildivpow2).
#[inline]
pub fn int64_ceildivpow2(a: i64, b: i32) -> i32 {
    debug_assert!(
        (0..63).contains(&b),
        "int64_ceildivpow2: shift must be in 0..63"
    );
    ((a + (1i64 << b) - 1) >> b) as i32
}

/// Ceiling division by 2^b for unsigned integers (C: opj_uint_ceildivpow2).
#[inline]
pub fn uint_ceildivpow2(a: u32, b: u32) -> u32 {
    debug_assert!(b < 32, "uint_ceildivpow2: shift must be in 0..32");
    ((a as u64 + (1u64 << b) - 1) >> b) as u32
}

/// Floor division by 2^b for signed integers (C: opj_int_floordivpow2).
#[inline]
pub fn int_floordivpow2(a: i32, b: i32) -> i32 {
    debug_assert!(
        (0..32).contains(&b),
        "int_floordivpow2: shift must be in 0..32"
    );
    a >> b
}

/// Floor of log2 for signed integers (C: opj_int_floorlog2).
#[inline]
pub fn int_floorlog2(a: i32) -> i32 {
    debug_assert!(a > 0);
    31 - (a as u32).leading_zeros() as i32
}

/// Floor of log2 for unsigned integers (C: opj_uint_floorlog2).
#[inline]
pub fn uint_floorlog2(a: u32) -> u32 {
    debug_assert!(a > 0);
    31 - a.leading_zeros()
}

/// Fixed-point multiplication with 13-bit shift (C: opj_int_fix_mul).
#[inline]
pub fn int_fix_mul(a: i32, b: i32) -> i32 {
    let temp = a as i64 * b as i64 + 4096;
    (temp >> 13) as i32
}

// --- T1 constants ---

/// NMSEDEC precision bits (C: T1_NMSEDEC_BITS).
pub const T1_NMSEDEC_BITS: u32 = 7;
/// NMSEDEC fractional bits (C: T1_NMSEDEC_FRACBITS).
pub const T1_NMSEDEC_FRACBITS: u32 = T1_NMSEDEC_BITS - 1;

/// Number of Zero Coding contexts (C: T1_NUMCTXS_ZC).
pub const T1_NUMCTXS_ZC: usize = 9;
/// Number of Sign Coding contexts (C: T1_NUMCTXS_SC).
pub const T1_NUMCTXS_SC: usize = 5;
/// Number of Magnitude contexts (C: T1_NUMCTXS_MAG).
pub const T1_NUMCTXS_MAG: usize = 3;
/// Number of Aggregation contexts (C: T1_NUMCTXS_AGG).
pub const T1_NUMCTXS_AGG: usize = 1;
/// Number of Uniform contexts (C: T1_NUMCTXS_UNI).
pub const T1_NUMCTXS_UNI: usize = 1;

/// Context offset: Zero Coding (C: T1_CTXNO_ZC).
pub const T1_CTXNO_ZC: usize = 0;
/// Context offset: Sign Coding (C: T1_CTXNO_SC).
pub const T1_CTXNO_SC: usize = T1_CTXNO_ZC + T1_NUMCTXS_ZC;
/// Context offset: Magnitude (C: T1_CTXNO_MAG).
pub const T1_CTXNO_MAG: usize = T1_CTXNO_SC + T1_NUMCTXS_SC;
/// Context offset: Aggregation (C: T1_CTXNO_AGG).
pub const T1_CTXNO_AGG: usize = T1_CTXNO_MAG + T1_NUMCTXS_MAG;
/// Context offset: Uniform (C: T1_CTXNO_UNI).
pub const T1_CTXNO_UNI: usize = T1_CTXNO_AGG + T1_NUMCTXS_AGG;
/// Total number of T1 contexts (C: T1_NUMCTXS).
pub const T1_NUMCTXS: usize = T1_CTXNO_UNI + T1_NUMCTXS_UNI;

/// Normal MQ coding mode (C: T1_TYPE_MQ).
pub const T1_TYPE_MQ: u8 = 0;
/// Raw (bypass) coding mode (C: T1_TYPE_RAW).
pub const T1_TYPE_RAW: u8 = 1;

// T1 flag bit positions — 32-bit word packs state for 4 data points in a column.
// SIGMA bits (0–17): significance for a 3-wide × 6-high neighbourhood window
//   (4 data points + 1 above + 1 below neighbours, each with W/THIS/E columns).
// Shift the word right by 3 bits to advance from one data point to the next.
pub const T1_SIGMA_0: u32 = 1 << 0;
pub const T1_SIGMA_1: u32 = 1 << 1;
pub const T1_SIGMA_2: u32 = 1 << 2;
pub const T1_SIGMA_3: u32 = 1 << 3;
pub const T1_SIGMA_4: u32 = 1 << 4;
pub const T1_SIGMA_5: u32 = 1 << 5;
pub const T1_SIGMA_6: u32 = 1 << 6;
pub const T1_SIGMA_7: u32 = 1 << 7;
pub const T1_SIGMA_8: u32 = 1 << 8;
pub const T1_SIGMA_9: u32 = 1 << 9;
pub const T1_SIGMA_10: u32 = 1 << 10;
pub const T1_SIGMA_11: u32 = 1 << 11;
pub const T1_SIGMA_12: u32 = 1 << 12;
pub const T1_SIGMA_13: u32 = 1 << 13;
pub const T1_SIGMA_14: u32 = 1 << 14;
pub const T1_SIGMA_15: u32 = 1 << 15;
pub const T1_SIGMA_16: u32 = 1 << 16;
pub const T1_SIGMA_17: u32 = 1 << 17;

// CHI: sign state (with bit index _I variants for shift computation)
pub const T1_CHI_0: u32 = 1 << 18;
pub const T1_CHI_0_I: u32 = 18;
pub const T1_CHI_1: u32 = 1 << 19;
pub const T1_CHI_1_I: u32 = 19;
pub const T1_MU_0: u32 = 1 << 20;
pub const T1_PI_0: u32 = 1 << 21;
pub const T1_CHI_2: u32 = 1 << 22;
pub const T1_CHI_2_I: u32 = 22;
pub const T1_MU_1: u32 = 1 << 23;
pub const T1_PI_1: u32 = 1 << 24;
pub const T1_CHI_3: u32 = 1 << 25;
pub const T1_MU_2: u32 = 1 << 26;
pub const T1_PI_2: u32 = 1 << 27;
pub const T1_CHI_4: u32 = 1 << 28;
pub const T1_MU_3: u32 = 1 << 29;
pub const T1_PI_3: u32 = 1 << 30;
pub const T1_CHI_5: u32 = 1 << 31;
pub const T1_CHI_5_I: u32 = 31;

// Direction aliases for data point 0 (shift by 3 bits per row).
pub const T1_SIGMA_NW: u32 = T1_SIGMA_0;
pub const T1_SIGMA_N: u32 = T1_SIGMA_1;
pub const T1_SIGMA_NE: u32 = T1_SIGMA_2;
pub const T1_SIGMA_W: u32 = T1_SIGMA_3;
pub const T1_SIGMA_THIS: u32 = T1_SIGMA_4;
pub const T1_SIGMA_E: u32 = T1_SIGMA_5;
pub const T1_SIGMA_SW: u32 = T1_SIGMA_6;
pub const T1_SIGMA_S: u32 = T1_SIGMA_7;
pub const T1_SIGMA_SE: u32 = T1_SIGMA_8;
pub const T1_SIGMA_NEIGHBOURS: u32 = T1_SIGMA_NW
    | T1_SIGMA_N
    | T1_SIGMA_NE
    | T1_SIGMA_W
    | T1_SIGMA_E
    | T1_SIGMA_SW
    | T1_SIGMA_S
    | T1_SIGMA_SE;

pub const T1_CHI_THIS: u32 = T1_CHI_1;
pub const T1_MU_THIS: u32 = T1_MU_0;
pub const T1_PI_THIS: u32 = T1_PI_0;
pub const T1_CHI_S: u32 = T1_CHI_2;

// LUT index bits for sign context (C: T1_LUT_SGN_W, etc.)
pub const T1_LUT_SGN_W: u32 = 1 << 0;
pub const T1_LUT_SIG_N: u32 = 1 << 1;
pub const T1_LUT_SGN_E: u32 = 1 << 2;
pub const T1_LUT_SIG_W: u32 = 1 << 3;
pub const T1_LUT_SGN_N: u32 = 1 << 4;
pub const T1_LUT_SIG_E: u32 = 1 << 5;
pub const T1_LUT_SGN_S: u32 = 1 << 6;
pub const T1_LUT_SIG_S: u32 = 1 << 7;

// Codeblock style flags (C: J2K_CCP_CBLKSTY_*)
pub const J2K_CCP_CBLKSTY_LAZY: u32 = 0x01;
pub const J2K_CCP_CBLKSTY_RESET: u32 = 0x02;
pub const J2K_CCP_CBLKSTY_TERMALL: u32 = 0x04;
pub const J2K_CCP_CBLKSTY_VSC: u32 = 0x08;
pub const J2K_CCP_CBLKSTY_PTERM: u32 = 0x10;
pub const J2K_CCP_CBLKSTY_SEGSYM: u32 = 0x20;

// --- J2K coding parameter constants ---

/// Maximum number of POC entries (C: J2K_MAX_POCS).
pub const J2K_MAX_POCS: usize = 32;

/// Coding style: precinct size defined (C: J2K_CP_CSTY_PRT).
pub const J2K_CP_CSTY_PRT: u32 = 0x01;
/// Coding style: SOP marker present (C: J2K_CP_CSTY_SOP).
pub const J2K_CP_CSTY_SOP: u32 = 0x02;
/// Coding style: EPH marker present (C: J2K_CP_CSTY_EPH).
pub const J2K_CP_CSTY_EPH: u32 = 0x04;
/// Component coding style: precinct size defined (C: J2K_CCP_CSTY_PRT).
pub const J2K_CCP_CSTY_PRT: u32 = 0x01;

/// Quantization: no quantization (C: J2K_CCP_QNTSTY_NOQNT).
pub const J2K_CCP_QNTSTY_NOQNT: u32 = 0;
/// Quantization: scalar implicit (C: J2K_CCP_QNTSTY_SIQNT).
pub const J2K_CCP_QNTSTY_SIQNT: u32 = 1;
/// Quantization: scalar explicit (C: J2K_CCP_QNTSTY_SEQNT).
pub const J2K_CCP_QNTSTY_SEQNT: u32 = 2;

/// Codeblock style: HT (high throughput) (C: J2K_CCP_CBLKSTY_HT).
pub const J2K_CCP_CBLKSTY_HT: u32 = 0x40;
/// Codeblock style: HT mixed mode (C: J2K_CCP_CBLKSTY_HTMIXED).
pub const J2K_CCP_CBLKSTY_HTMIXED: u32 = 0x80;

/// Fixed-point multiplication for T1 NMSEDEC (C: opj_int_fix_mul_t1).
/// Uses round-to-nearest instead of biased rounding.
#[inline]
pub fn int_fix_mul_t1(a: i32, b: i32) -> i32 {
    let temp = a as i64 * b as i64;
    ((temp + (temp & 4096)) >> 13) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Constants ---

    #[test]
    fn constants_values() {
        assert_eq!(J2K_MAXRLVLS, 33);
        assert_eq!(J2K_MAXBANDS, 3 * J2K_MAXRLVLS - 2);
        assert_eq!(J2K_DEFAULT_NB_SEGS, 10);
        assert_eq!(J2K_STREAM_CHUNK_SIZE, 0x100000);
        assert_eq!(PATH_LEN, 4096);
        assert_eq!(IMG_INFO, 1);
        assert_eq!(J2K_MH_INFO, 2);
        assert_eq!(J2K_TH_INFO, 4);
        assert_eq!(J2K_TCH_INFO, 8);
        assert_eq!(J2K_MH_IND, 16);
        assert_eq!(J2K_TH_IND, 32);
        assert_eq!(JP2_INFO, 128);
        assert_eq!(JP2_IND, 256);
        assert_eq!(COMMON_CBLK_DATA_EXTRA, 2);
        assert_eq!(COMP_PARAM_DEFAULT_CBLOCKW, 64);
        assert_eq!(COMP_PARAM_DEFAULT_CBLOCKH, 64);
        assert_eq!(COMP_PARAM_DEFAULT_NUMRESOLUTION, 6);
    }

    // --- Enums ---

    #[test]
    fn progression_order_variants() {
        assert_eq!(ProgressionOrder::Lrcp as i32, 0);
        assert_eq!(ProgressionOrder::Rlcp as i32, 1);
        assert_eq!(ProgressionOrder::Rpcl as i32, 2);
        assert_eq!(ProgressionOrder::Pcrl as i32, 3);
        assert_eq!(ProgressionOrder::Cprl as i32, 4);
    }

    #[test]
    fn color_space_variants() {
        assert_eq!(ColorSpace::Unknown as i32, -1);
        assert_eq!(ColorSpace::Unspecified as i32, 0);
        assert_eq!(ColorSpace::Srgb as i32, 1);
        assert_eq!(ColorSpace::Gray as i32, 2);
        assert_eq!(ColorSpace::Sycc as i32, 3);
        assert_eq!(ColorSpace::Eycc as i32, 4);
        assert_eq!(ColorSpace::Cmyk as i32, 5);
    }

    #[test]
    fn codec_format_variants() {
        assert_eq!(CodecFormat::Unknown as i32, -1);
        assert_eq!(CodecFormat::J2k as i32, 0);
        assert_eq!(CodecFormat::Jpt as i32, 1);
        assert_eq!(CodecFormat::Jp2 as i32, 2);
    }

    #[test]
    fn enum_derive_traits() {
        let a = ProgressionOrder::Lrcp;
        let b = a;
        assert_eq!(a, b);
        let _ = format!("{:?}", ColorSpace::Srgb);
        assert_ne!(CodecFormat::J2k, CodecFormat::Jp2);
    }

    // --- Integer math ---

    #[test]
    fn int_ceildiv_basic() {
        assert_eq!(int_ceildiv(10, 3), 4);
        assert_eq!(int_ceildiv(9, 3), 3);
        assert_eq!(int_ceildiv(1, 1), 1);
        assert_eq!(int_ceildiv(0, 5), 0);
    }

    #[test]
    fn int_ceildiv_negative() {
        // C formula: (a + b - 1) / b with truncation toward zero
        // (-10 + 3 - 1) / 3 = -8 / 3 = -2
        assert_eq!(int_ceildiv(-10, 3), -2);
        // (-9 + 3 - 1) / 3 = -7 / 3 = -2
        assert_eq!(int_ceildiv(-9, 3), -2);
    }

    #[test]
    fn uint_ceildiv_basic() {
        assert_eq!(uint_ceildiv(10, 3), 4);
        assert_eq!(uint_ceildiv(9, 3), 3);
        assert_eq!(uint_ceildiv(0, 5), 0);
        assert_eq!(uint_ceildiv(1, 1), 1);
        assert_eq!(uint_ceildiv(u32::MAX, 2), (u32::MAX / 2) + 1);
    }

    #[test]
    fn uint64_ceildiv_as_u32_basic() {
        assert_eq!(uint64_ceildiv_as_u32(10, 3), 4);
        assert_eq!(uint64_ceildiv_as_u32(9, 3), 3);
        assert_eq!(uint64_ceildiv_as_u32(0, 5), 0);
    }

    #[test]
    fn int_ceildivpow2_basic() {
        assert_eq!(int_ceildivpow2(10, 2), 3);
        assert_eq!(int_ceildivpow2(8, 2), 2);
        assert_eq!(int_ceildivpow2(0, 3), 0);
        assert_eq!(int_ceildivpow2(1, 0), 1);
    }

    #[test]
    fn int_ceildivpow2_negative() {
        assert_eq!(int_ceildivpow2(-8, 2), -2);
        assert_eq!(int_ceildivpow2(-7, 2), -1);
    }

    #[test]
    fn int64_ceildivpow2_basic() {
        assert_eq!(int64_ceildivpow2(10, 2), 3);
        assert_eq!(int64_ceildivpow2(-8, 2), -2);
    }

    #[test]
    fn uint_ceildivpow2_basic() {
        assert_eq!(uint_ceildivpow2(10, 2), 3);
        assert_eq!(uint_ceildivpow2(8, 2), 2);
        assert_eq!(uint_ceildivpow2(0, 3), 0);
    }

    #[test]
    fn int_floordivpow2_basic() {
        assert_eq!(int_floordivpow2(10, 2), 2);
        assert_eq!(int_floordivpow2(8, 2), 2);
        assert_eq!(int_floordivpow2(0, 3), 0);
        assert_eq!(int_floordivpow2(-8, 2), -2);
        assert_eq!(int_floordivpow2(-7, 2), -2);
    }

    #[test]
    fn int_floorlog2_basic() {
        assert_eq!(int_floorlog2(1), 0);
        assert_eq!(int_floorlog2(2), 1);
        assert_eq!(int_floorlog2(3), 1);
        assert_eq!(int_floorlog2(4), 2);
        assert_eq!(int_floorlog2(7), 2);
        assert_eq!(int_floorlog2(8), 3);
        assert_eq!(int_floorlog2(1024), 10);
    }

    #[test]
    fn uint_floorlog2_basic() {
        assert_eq!(uint_floorlog2(1), 0);
        assert_eq!(uint_floorlog2(2), 1);
        assert_eq!(uint_floorlog2(4), 2);
        assert_eq!(uint_floorlog2(1024), 10);
        assert_eq!(uint_floorlog2(u32::MAX), 31);
    }

    #[test]
    fn int_fix_mul_basic() {
        assert_eq!(int_fix_mul(8192, 8192), 8192); // 1.0 * 1.0 = 1.0
        assert_eq!(int_fix_mul(8192, 4096), 4096); // 1.0 * 0.5 = 0.5
        assert_eq!(int_fix_mul(0, 8192), 0);
    }

    // --- T1 constants ---

    #[test]
    fn t1_context_offsets() {
        assert_eq!(T1_CTXNO_ZC, 0);
        assert_eq!(T1_CTXNO_SC, 9);
        assert_eq!(T1_CTXNO_MAG, 14);
        assert_eq!(T1_CTXNO_AGG, 17);
        assert_eq!(T1_CTXNO_UNI, 18);
        assert_eq!(T1_NUMCTXS, 19);
    }

    #[test]
    fn t1_flag_bit_positions() {
        // SIGMA bits are contiguous 0..17
        assert_eq!(T1_SIGMA_0, 1);
        assert_eq!(T1_SIGMA_17, 1 << 17);
        // CHI/MU/PI interleaving
        assert_eq!(T1_CHI_0, 1 << 18);
        assert_eq!(T1_CHI_1, 1 << 19);
        assert_eq!(T1_MU_0, 1 << 20);
        assert_eq!(T1_PI_0, 1 << 21);
        assert_eq!(T1_CHI_5, 1 << 31);
        // Direction aliases
        assert_eq!(T1_SIGMA_NW, T1_SIGMA_0);
        assert_eq!(T1_SIGMA_THIS, T1_SIGMA_4);
        assert_eq!(T1_SIGMA_SE, T1_SIGMA_8);
        assert_eq!(T1_CHI_THIS, T1_CHI_1);
    }

    #[test]
    fn t1_sigma_neighbours_covers_8_directions() {
        let expected = T1_SIGMA_0
            | T1_SIGMA_1
            | T1_SIGMA_2
            | T1_SIGMA_3
            | T1_SIGMA_5
            | T1_SIGMA_6
            | T1_SIGMA_7
            | T1_SIGMA_8;
        assert_eq!(T1_SIGMA_NEIGHBOURS, expected);
        // Does NOT include THIS (SIGMA_4)
        assert_eq!(T1_SIGMA_NEIGHBOURS & T1_SIGMA_THIS, 0);
    }

    #[test]
    fn cblk_style_flags() {
        assert_eq!(J2K_CCP_CBLKSTY_LAZY, 0x01);
        assert_eq!(J2K_CCP_CBLKSTY_RESET, 0x02);
        assert_eq!(J2K_CCP_CBLKSTY_TERMALL, 0x04);
        assert_eq!(J2K_CCP_CBLKSTY_VSC, 0x08);
        assert_eq!(J2K_CCP_CBLKSTY_PTERM, 0x10);
        assert_eq!(J2K_CCP_CBLKSTY_SEGSYM, 0x20);
    }

    #[test]
    fn int_fix_mul_t1_basic() {
        // 1.0 * 1.0 = 1.0 in Q13
        assert_eq!(int_fix_mul_t1(8192, 8192), 8192);
        // 1.0 * 0.5 = 0.5
        assert_eq!(int_fix_mul_t1(8192, 4096), 4096);
        assert_eq!(int_fix_mul_t1(0, 8192), 0);
    }

    #[test]
    fn int_fix_mul_t1_rounding() {
        // int_fix_mul_t1 rounds to nearest (adds bit 12 before shift)
        // vs int_fix_mul which uses biased rounding (adds 4096)
        // For value where bit 12 is set: 5 * 8192 = 40960
        // temp = 5 * 8192 = 40960, bit 12 = 40960 & 4096 = 0 -> same as biased
        // For value 3 * 4097 = 12291 (has bit 12 set differently):
        let a = 3;
        let b = 4097;
        let temp = a as i64 * b as i64; // 12291
        let rounded = ((temp + (temp & 4096)) >> 13) as i32;
        assert_eq!(int_fix_mul_t1(a, b), rounded);
    }
}
