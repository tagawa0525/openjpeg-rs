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
    debug_assert!(b >= 0 && b < 31, "int_ceildivpow2: shift must be in 0..31");
    ((a as i64 + (1i64 << b) - 1) >> b) as i32
}

/// Ceiling division by 2^b for i64 (C: opj_int64_ceildivpow2).
#[inline]
pub fn int64_ceildivpow2(a: i64, b: i32) -> i32 {
    debug_assert!(
        b >= 0 && b < 63,
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
    debug_assert!(b >= 0 && b < 32, "int_floordivpow2: shift must be in 0..32");
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
}
