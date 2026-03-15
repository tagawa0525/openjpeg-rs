// Phase 100: Common types, constants, and integer math

// --- Constants ---

pub const J2K_MAXRLVLS: usize = 33;
pub const J2K_MAXBANDS: usize = 3 * J2K_MAXRLVLS - 2;
pub const J2K_DEFAULT_NB_SEGS: u32 = 10;
pub const J2K_STREAM_CHUNK_SIZE: usize = 0x100000;
pub const PATH_LEN: usize = 4096;
pub const IMG_INFO: u32 = 1;
pub const J2K_MH_INFO: u32 = 2;
pub const J2K_TH_INFO: u32 = 4;
pub const J2K_TCH_INFO: u32 = 8;
pub const J2K_MH_IND: u32 = 16;
pub const J2K_TH_IND: u32 = 32;
pub const JP2_INFO: u32 = 128;
pub const JP2_IND: u32 = 256;
pub const COMMON_CBLK_DATA_EXTRA: usize = 2;
pub const COMP_PARAM_DEFAULT_CBLOCKW: u32 = 64;
pub const COMP_PARAM_DEFAULT_CBLOCKH: u32 = 64;
pub const COMP_PARAM_DEFAULT_NUMRESOLUTION: u32 = 6;

// --- Enums (stub) ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ProgressionOrder {
    Lrcp = 0,
    Rlcp = 1,
    Rpcl = 2,
    Pcrl = 3,
    Cprl = 4,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum CodecFormat {
    Unknown = -1,
    J2k = 0,
    Jpt = 1,
    Jp2 = 2,
}

// --- Integer math (stubs) ---

#[inline]
pub fn int_ceildiv(_a: i32, _b: i32) -> i32 {
    todo!()
}

#[inline]
pub fn uint_ceildiv(_a: u32, _b: u32) -> u32 {
    todo!()
}

#[inline]
pub fn uint64_ceildiv_as_u32(_a: u64, _b: u64) -> u32 {
    todo!()
}

#[inline]
pub fn int_ceildivpow2(_a: i32, _b: i32) -> i32 {
    todo!()
}

#[inline]
pub fn int64_ceildivpow2(_a: i64, _b: i32) -> i32 {
    todo!()
}

#[inline]
pub fn uint_ceildivpow2(_a: u32, _b: u32) -> u32 {
    todo!()
}

#[inline]
pub fn int_floordivpow2(_a: i32, _b: i32) -> i32 {
    todo!()
}

#[inline]
pub fn int_floorlog2(_a: i32) -> i32 {
    todo!()
}

#[inline]
pub fn uint_floorlog2(_a: u32) -> u32 {
    todo!()
}

#[inline]
pub fn int_fix_mul(_a: i32, _b: i32) -> i32 {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Constants ---

    #[test]
    #[ignore = "not yet implemented"]
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
    #[ignore = "not yet implemented"]
    fn progression_order_variants() {
        assert_eq!(ProgressionOrder::Lrcp as i32, 0);
        assert_eq!(ProgressionOrder::Rlcp as i32, 1);
        assert_eq!(ProgressionOrder::Rpcl as i32, 2);
        assert_eq!(ProgressionOrder::Pcrl as i32, 3);
        assert_eq!(ProgressionOrder::Cprl as i32, 4);
    }

    #[test]
    #[ignore = "not yet implemented"]
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
    #[ignore = "not yet implemented"]
    fn codec_format_variants() {
        assert_eq!(CodecFormat::Unknown as i32, -1);
        assert_eq!(CodecFormat::J2k as i32, 0);
        assert_eq!(CodecFormat::Jpt as i32, 1);
        assert_eq!(CodecFormat::Jp2 as i32, 2);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn enum_derive_traits() {
        // Clone + Copy
        let a = ProgressionOrder::Lrcp;
        let b = a;
        assert_eq!(a, b);

        // Debug
        let _ = format!("{:?}", ColorSpace::Srgb);

        // PartialEq + Eq
        assert_ne!(CodecFormat::J2k, CodecFormat::Jp2);
    }

    // --- Integer math ---

    #[test]
    #[ignore = "not yet implemented"]
    fn int_ceildiv_basic() {
        assert_eq!(int_ceildiv(10, 3), 4);
        assert_eq!(int_ceildiv(9, 3), 3);
        assert_eq!(int_ceildiv(1, 1), 1);
        assert_eq!(int_ceildiv(0, 5), 0);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn int_ceildiv_negative() {
        assert_eq!(int_ceildiv(-10, 3), -3);
        assert_eq!(int_ceildiv(-9, 3), -2);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn uint_ceildiv_basic() {
        assert_eq!(uint_ceildiv(10, 3), 4);
        assert_eq!(uint_ceildiv(9, 3), 3);
        assert_eq!(uint_ceildiv(0, 5), 0);
        assert_eq!(uint_ceildiv(1, 1), 1);
        assert_eq!(uint_ceildiv(u32::MAX, 2), (u32::MAX / 2) + 1);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn uint64_ceildiv_as_u32_basic() {
        assert_eq!(uint64_ceildiv_as_u32(10, 3), 4);
        assert_eq!(uint64_ceildiv_as_u32(9, 3), 3);
        assert_eq!(uint64_ceildiv_as_u32(0, 5), 0);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn int_ceildivpow2_basic() {
        assert_eq!(int_ceildivpow2(10, 2), 3);
        assert_eq!(int_ceildivpow2(8, 2), 2);
        assert_eq!(int_ceildivpow2(0, 3), 0);
        assert_eq!(int_ceildivpow2(1, 0), 1);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn int_ceildivpow2_negative() {
        assert_eq!(int_ceildivpow2(-8, 2), -2);
        assert_eq!(int_ceildivpow2(-7, 2), -1);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn int64_ceildivpow2_basic() {
        assert_eq!(int64_ceildivpow2(10, 2), 3);
        assert_eq!(int64_ceildivpow2(-8, 2), -2);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn uint_ceildivpow2_basic() {
        assert_eq!(uint_ceildivpow2(10, 2), 3);
        assert_eq!(uint_ceildivpow2(8, 2), 2);
        assert_eq!(uint_ceildivpow2(0, 3), 0);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn int_floordivpow2_basic() {
        assert_eq!(int_floordivpow2(10, 2), 2);
        assert_eq!(int_floordivpow2(8, 2), 2);
        assert_eq!(int_floordivpow2(0, 3), 0);
        assert_eq!(int_floordivpow2(-8, 2), -2);
        assert_eq!(int_floordivpow2(-7, 2), -2);
    }

    #[test]
    #[ignore = "not yet implemented"]
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
    #[ignore = "not yet implemented"]
    fn uint_floorlog2_basic() {
        assert_eq!(uint_floorlog2(1), 0);
        assert_eq!(uint_floorlog2(2), 1);
        assert_eq!(uint_floorlog2(4), 2);
        assert_eq!(uint_floorlog2(1024), 10);
        assert_eq!(uint_floorlog2(u32::MAX), 31);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn int_fix_mul_basic() {
        // ((i64)a * (i64)b + 4096) >> 13
        assert_eq!(int_fix_mul(8192, 8192), 8192); // 1.0 * 1.0 = 1.0
        assert_eq!(int_fix_mul(8192, 4096), 4096); // 1.0 * 0.5 = 0.5
        assert_eq!(int_fix_mul(0, 8192), 0);
    }
}
