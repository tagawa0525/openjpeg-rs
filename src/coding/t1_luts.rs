// T1 lookup tables (C: t1_luts.h)
// Auto-generated values from the JPEG 2000 specification.

/// Zero Coding context lookup table (C: lut_ctxno_zc).
/// Indexed by: (orient << 9) | neighbour significance pattern.
pub static LUT_CTXNO_ZC: [u8; 2048] = [0; 2048]; // placeholder

/// Sign Coding context lookup table (C: lut_ctxno_sc).
pub static LUT_CTXNO_SC: [u8; 256] = [0; 256]; // placeholder

/// Sign Prediction Bit lookup table (C: lut_spb).
pub static LUT_SPB: [u8; 256] = [0; 256]; // placeholder

/// NMSEDEC significance lookup (C: lut_nmsedec_sig).
pub static LUT_NMSEDEC_SIG: [i16; 128] = [0; 128]; // placeholder

/// NMSEDEC significance (bpno=0) lookup (C: lut_nmsedec_sig0).
pub static LUT_NMSEDEC_SIG0: [i16; 128] = [0; 128]; // placeholder

/// NMSEDEC refinement lookup (C: lut_nmsedec_ref).
pub static LUT_NMSEDEC_REF: [i16; 128] = [0; 128]; // placeholder

/// NMSEDEC refinement (bpno=0) lookup (C: lut_nmsedec_ref0).
pub static LUT_NMSEDEC_REF0: [i16; 128] = [0; 128]; // placeholder

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_sizes() {
        assert_eq!(LUT_CTXNO_ZC.len(), 2048);
        assert_eq!(LUT_CTXNO_SC.len(), 256);
        assert_eq!(LUT_SPB.len(), 256);
        assert_eq!(LUT_NMSEDEC_SIG.len(), 128);
        assert_eq!(LUT_NMSEDEC_SIG0.len(), 128);
        assert_eq!(LUT_NMSEDEC_REF.len(), 128);
        assert_eq!(LUT_NMSEDEC_REF0.len(), 128);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn lut_ctxno_zc_spot_check() {
        // First entries (HL orient=0)
        assert_eq!(LUT_CTXNO_ZC[0], 0);
        assert_eq!(LUT_CTXNO_ZC[1], 1);
        assert_eq!(LUT_CTXNO_ZC[2], 3);
        assert_eq!(LUT_CTXNO_ZC[3], 3);
        // Last entry
        assert_eq!(LUT_CTXNO_ZC[2047], 8);
        // HH orient (offset 1536)
        assert_eq!(LUT_CTXNO_ZC[1536], 0);
        assert_eq!(LUT_CTXNO_ZC[1537], 3);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn lut_ctxno_sc_spot_check() {
        assert_eq!(LUT_CTXNO_SC[0], 0x9);
        assert_eq!(LUT_CTXNO_SC[1], 0x9);
        assert_eq!(LUT_CTXNO_SC[255], 0xd);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn lut_spb_spot_check() {
        assert_eq!(LUT_SPB[0], 0);
        assert_eq!(LUT_SPB[9], 1);
        assert_eq!(LUT_SPB[255], 1);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn lut_nmsedec_sig_spot_check() {
        // First 48 entries are 0
        assert_eq!(LUT_NMSEDEC_SIG[0], 0);
        assert_eq!(LUT_NMSEDEC_SIG[47], 0);
        // Entry 49 = 0x0180
        assert_eq!(LUT_NMSEDEC_SIG[49], 0x0180);
        // Last entry
        assert_eq!(LUT_NMSEDEC_SIG[127], 0x7680);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn lut_nmsedec_ref_spot_check() {
        assert_eq!(LUT_NMSEDEC_REF[0], 0x1800);
        // Middle entries are 0
        assert_eq!(LUT_NMSEDEC_REF[48], 0);
        assert_eq!(LUT_NMSEDEC_REF[127], 0x1780);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn lut_nmsedec_sig0_spot_check() {
        assert_eq!(LUT_NMSEDEC_SIG0[0], 0);
        assert_eq!(LUT_NMSEDEC_SIG0[6], 0x0080);
        assert_eq!(LUT_NMSEDEC_SIG0[127], 0x7e00);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn lut_nmsedec_ref0_spot_check() {
        assert_eq!(LUT_NMSEDEC_REF0[0], 0x2000);
        assert_eq!(LUT_NMSEDEC_REF0[63], 0);
        assert_eq!(LUT_NMSEDEC_REF0[127], 0x1f00);
    }
}
