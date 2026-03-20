// Phase 300b: Tier-1 coding (C: opj_t1_t)
//
// Encodes/decodes code-block coefficients using context-based MQ arithmetic coding.
// Three coding passes per bitplane: Significance, Refinement, Clean-up.

use crate::error::{Error, Result};
use crate::j2k::params::TileCodingParameters;
use crate::tcd::{TcdCodeBlocks, TcdTile};
use crate::transform::dwt::{dwt_getnorm, dwt_getnorm_real};
use crate::types::*;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

/// Encoding pass information (C: opj_tcd_pass_t).
#[derive(Debug, Default, Clone)]
pub struct TcdPass {
    pub rate: u32,
    pub distortion_decrease: f64,
    pub len: u32,
    pub term: bool,
}

/// Decoder segment.
pub struct DecodeSegment<'a> {
    pub data: &'a [u8],
    pub num_passes: u32,
}

/// Compute weighted MSE decrease for a coding pass (C: opj_t1_getwmsedec).
#[allow(clippy::too_many_arguments)]
pub fn t1_getwmsedec(
    nmsedec: i32,
    compno: u32,
    level: u32,
    orient: u32,
    bpno: i32,
    qmfbid: u32,
    mut stepsize: f64,
    mct_norms: Option<&[f64]>,
) -> f64 {
    let w1 = match mct_norms {
        Some(norms) if (compno as usize) < norms.len() => norms[compno as usize],
        _ => 1.0,
    };

    let w2 = if qmfbid == 1 {
        dwt_getnorm(level, orient)
    } else {
        let log2_gain = match orient {
            0 => 0,
            3 => 2,
            _ => 1,
        };
        let w2 = dwt_getnorm_real(level, orient);
        stepsize /= (1 << log2_gain) as f64;
        w2
    };

    debug_assert!(bpno >= 0, "t1_getwmsedec: bpno must be non-negative");
    let wmsedec = w1 * w2 * stepsize * ((1u32 << bpno as u32) as f64);
    wmsedec * wmsedec * nmsedec as f64 / 8192.0
}

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
        if w > 1024 || h > 1024 || w * h > 4096 {
            return Err(crate::error::Error::InvalidInput(format!(
                "code-block dimensions out of range: w={w}, h={h} (max 1024x1024, w*h<=4096)"
            )));
        }

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

    // --- Encoding step helpers ---

    /// Significance pass step (encoder) for one coefficient.
    #[allow(clippy::too_many_arguments)]
    fn enc_sigpass_step(
        &mut self,
        mqc: &mut crate::coding::mqc::Mqc,
        fp: usize,
        datap: usize,
        bpno: i32,
        one: u32,
        nmsedec: &mut i32,
        pass_type: u8,
        ci: u32,
        vsc: bool,
    ) {
        let flags = self.flags[fp];
        let shift = ci * 3;

        if (flags & ((T1_SIGMA_THIS | T1_PI_THIS) << shift)) == 0
            && (flags & (T1_SIGMA_NEIGHBOURS << shift)) != 0
        {
            let ctxt1 = getctxno_zc(self.lut_ctxno_zc_orient_offset, flags >> shift) as usize;
            let v = if (smr_abs(self.data[datap]) & one) != 0 {
                1u32
            } else {
                0u32
            };
            mqc.set_curctx(ctxt1);
            if pass_type == T1_TYPE_RAW {
                mqc.bypass_enc(v);
            } else {
                mqc.encode(v);
            }
            if v != 0 {
                let lu = getctxtno_sc_or_spb_index(
                    self.flags[fp],
                    self.flags[fp - 1],
                    self.flags[fp + 1],
                    ci,
                );
                let ctxt2 = getctxno_sc(lu) as usize;
                let sign = smr_sign(self.data[datap]);
                *nmsedec += getnmsedec_sig(smr_abs(self.data[datap]), bpno as u32) as i32;
                mqc.set_curctx(ctxt2);
                if pass_type == T1_TYPE_RAW {
                    mqc.bypass_enc(sign);
                } else {
                    let spb = getspb(lu) as u32;
                    mqc.encode(sign ^ spb);
                }
                let stride = self.flags_stride();
                update_flags(&mut self.flags, fp, ci, sign, stride, vsc);
            }
            self.flags[fp] |= T1_PI_THIS << shift;
        }
    }

    // --- Encoding passes ---

    /// Significance pass encoder (C: opj_t1_enc_sigpass).
    pub fn enc_sigpass(
        &mut self,
        mqc: &mut crate::coding::mqc::Mqc,
        bpno: i32,
        pass_type: u8,
        cblksty: u32,
    ) -> i32 {
        let mut nmsedec = 0i32;
        let one = 1u32 << (bpno as u32 + T1_NMSEDEC_FRACBITS);
        let w = self.w as usize;
        let h = self.h as usize;
        let stride = self.flags_stride();

        let mut datap = 0usize;
        let mut fp = stride + 1; // T1_FLAGS(0, 0)

        // Full 4-row strips
        for _k in (0..h & !3).step_by(4) {
            for _i in 0..w {
                if self.flags[fp] != 0 {
                    let vsc0 = (cblksty & J2K_CCP_CBLKSTY_VSC) != 0;
                    self.enc_sigpass_step(
                        mqc,
                        fp,
                        datap,
                        bpno,
                        one,
                        &mut nmsedec,
                        pass_type,
                        0,
                        vsc0,
                    );
                    self.enc_sigpass_step(
                        mqc,
                        fp,
                        datap + 1,
                        bpno,
                        one,
                        &mut nmsedec,
                        pass_type,
                        1,
                        false,
                    );
                    self.enc_sigpass_step(
                        mqc,
                        fp,
                        datap + 2,
                        bpno,
                        one,
                        &mut nmsedec,
                        pass_type,
                        2,
                        false,
                    );
                    self.enc_sigpass_step(
                        mqc,
                        fp,
                        datap + 3,
                        bpno,
                        one,
                        &mut nmsedec,
                        pass_type,
                        3,
                        false,
                    );
                }
                datap += 4;
                fp += 1;
            }
            fp += 2; // skip border columns
        }

        // Remaining rows
        let k = h & !3;
        if k < h {
            for _i in 0..w {
                if self.flags[fp] != 0 {
                    for j in k..h {
                        let ci = (j - k) as u32;
                        let vsc = ci == 0 && (cblksty & J2K_CCP_CBLKSTY_VSC) != 0;
                        self.enc_sigpass_step(
                            mqc,
                            fp,
                            datap,
                            bpno,
                            one,
                            &mut nmsedec,
                            pass_type,
                            ci,
                            vsc,
                        );
                        datap += 1;
                    }
                } else {
                    datap += h - k;
                }
                fp += 1;
            }
        }

        nmsedec
    }

    // --- Refinement encoding step helper ---

    /// Refinement pass step (encoder) for one coefficient (C: opj_t1_enc_refpass_step_macro).
    #[allow(clippy::too_many_arguments)]
    fn enc_refpass_step(
        &mut self,
        mqc: &mut crate::coding::mqc::Mqc,
        fp: usize,
        datap: usize,
        bpno: i32,
        one: u32,
        nmsedec: &mut i32,
        pass_type: u8,
        ci: u32,
    ) {
        let flags = self.flags[fp];
        let shift = ci * 3;

        if (flags & ((T1_SIGMA_THIS | T1_PI_THIS) << shift)) == (T1_SIGMA_THIS << shift) {
            let ctxt = getctxno_mag(flags >> shift) as usize;
            let abs_data = smr_abs(self.data[datap]);
            *nmsedec += getnmsedec_ref(abs_data, bpno as u32) as i32;
            let v = if (abs_data & one) != 0 { 1u32 } else { 0u32 };
            mqc.set_curctx(ctxt);
            if pass_type == T1_TYPE_RAW {
                mqc.bypass_enc(v);
            } else {
                mqc.encode(v);
            }
            self.flags[fp] |= T1_MU_THIS << shift;
        }
    }

    // --- Refinement encoding pass ---

    /// Refinement pass encoder (C: opj_t1_enc_refpass).
    pub fn enc_refpass(
        &mut self,
        mqc: &mut crate::coding::mqc::Mqc,
        bpno: i32,
        pass_type: u8,
    ) -> i32 {
        let mut nmsedec = 0i32;
        let one = 1u32 << (bpno as u32 + T1_NMSEDEC_FRACBITS);
        let w = self.w as usize;
        let h = self.h as usize;
        let stride = self.flags_stride();

        let any_sigma = T1_SIGMA_4 | T1_SIGMA_7 | T1_SIGMA_10 | T1_SIGMA_13;
        let all_pi = T1_PI_0 | T1_PI_1 | T1_PI_2 | T1_PI_3;

        let mut datap = 0usize;
        let mut fp = stride + 1;

        // Full 4-row strips
        for _k in (0..h & !3).step_by(4) {
            for _i in 0..w {
                let flags = self.flags[fp];
                if (flags & any_sigma) == 0 {
                    // none significant
                    datap += 4;
                    fp += 1;
                    continue;
                }
                if (flags & all_pi) == all_pi {
                    // all processed by sigpass
                    datap += 4;
                    fp += 1;
                    continue;
                }
                self.enc_refpass_step(mqc, fp, datap, bpno, one, &mut nmsedec, pass_type, 0);
                self.enc_refpass_step(mqc, fp, datap + 1, bpno, one, &mut nmsedec, pass_type, 1);
                self.enc_refpass_step(mqc, fp, datap + 2, bpno, one, &mut nmsedec, pass_type, 2);
                self.enc_refpass_step(mqc, fp, datap + 3, bpno, one, &mut nmsedec, pass_type, 3);
                datap += 4;
                fp += 1;
            }
            fp += 2; // skip border columns
        }

        // Remaining rows
        let k = h & !3;
        if k < h {
            let remaining = h - k;
            for _i in 0..w {
                if (self.flags[fp] & any_sigma) == 0 {
                    datap += remaining;
                    fp += 1;
                    continue;
                }
                for j in 0..remaining {
                    self.enc_refpass_step(
                        mqc,
                        fp,
                        datap,
                        bpno,
                        one,
                        &mut nmsedec,
                        pass_type,
                        j as u32,
                    );
                    datap += 1;
                }
                fp += 1;
            }
        }

        nmsedec
    }

    // --- Clean-up encoding step helper ---

    /// Clean-up pass step (encoder) for one coefficient (C: opj_t1_enc_clnpass_step_macro).
    ///
    /// Processes coefficients from `runlen` to `lim` in the current column.
    /// If `agg` is true and `ci == runlen`, the coefficient is known to be significant
    /// (determined by aggregation) and skips the ZC encode.
    #[allow(clippy::too_many_arguments)]
    fn enc_clnpass_step(
        &mut self,
        mqc: &mut crate::coding::mqc::Mqc,
        fp: usize,
        datap: usize,
        bpno: i32,
        one: u32,
        nmsedec: &mut i32,
        agg: bool,
        runlen: u32,
        lim: u32,
        cblksty: u32,
    ) {
        let check = T1_SIGMA_4
            | T1_SIGMA_7
            | T1_SIGMA_10
            | T1_SIGMA_13
            | T1_PI_0
            | T1_PI_1
            | T1_PI_2
            | T1_PI_3;

        // If all 4 samples are significant AND all have PI set, just clear PI bits
        if (self.flags[fp] & check) == check {
            match runlen {
                0 => self.flags[fp] &= !(T1_PI_0 | T1_PI_1 | T1_PI_2 | T1_PI_3),
                1 => self.flags[fp] &= !(T1_PI_1 | T1_PI_2 | T1_PI_3),
                2 => self.flags[fp] &= !(T1_PI_2 | T1_PI_3),
                3 => self.flags[fp] &= !T1_PI_3,
                _ => {}
            }
            return;
        }

        let mut l_datap = datap;
        for ci in runlen..lim {
            let mut goto_partial = false;

            if agg && ci == runlen {
                goto_partial = true;
            } else if (self.flags[fp] & ((T1_SIGMA_THIS | T1_PI_THIS) << (ci * 3))) == 0 {
                let ctxt1 = getctxno_zc(self.lut_ctxno_zc_orient_offset, self.flags[fp] >> (ci * 3))
                    as usize;
                let v = if (smr_abs(self.data[l_datap]) & one) != 0 {
                    1u32
                } else {
                    0u32
                };
                mqc.set_curctx(ctxt1);
                mqc.encode(v);
                if v != 0 {
                    goto_partial = true;
                }
            }

            if goto_partial {
                let lu = getctxtno_sc_or_spb_index(
                    self.flags[fp],
                    self.flags[fp - 1],
                    self.flags[fp + 1],
                    ci,
                );
                *nmsedec += getnmsedec_sig(smr_abs(self.data[l_datap]), bpno as u32) as i32;
                let ctxt2 = getctxno_sc(lu) as usize;
                mqc.set_curctx(ctxt2);
                let sign = smr_sign(self.data[l_datap]);
                let spb = getspb(lu) as u32;
                mqc.encode(sign ^ spb);
                let vsc = (cblksty & J2K_CCP_CBLKSTY_VSC) != 0 && ci == 0;
                let stride = self.flags_stride();
                update_flags(&mut self.flags, fp, ci, sign, stride, vsc);
            }

            self.flags[fp] &= !(T1_PI_THIS << (3 * ci));
            l_datap += 1;
        }
    }

    // --- Clean-up encoding pass ---

    /// Clean-up pass encoder (C: opj_t1_enc_clnpass).
    pub fn enc_clnpass(
        &mut self,
        mqc: &mut crate::coding::mqc::Mqc,
        bpno: i32,
        cblksty: u32,
    ) -> i32 {
        let mut nmsedec = 0i32;
        let one = 1u32 << (bpno as u32 + T1_NMSEDEC_FRACBITS);
        let w = self.w as usize;
        let h = self.h as usize;
        let stride = self.flags_stride();

        let mut datap = 0usize;
        let mut fp = stride + 1;

        // Full 4-row strips
        for _k in (0..h & !3).step_by(4) {
            for _i in 0..w {
                let agg = self.flags[fp] == 0;
                let runlen;

                if agg {
                    // Find first significant sample
                    let mut rl = 0u32;
                    while rl < 4 {
                        if (smr_abs(self.data[datap + rl as usize]) & one) != 0 {
                            break;
                        }
                        rl += 1;
                    }
                    runlen = rl;

                    mqc.set_curctx(T1_CTXNO_AGG);
                    mqc.encode(if runlen != 4 { 1 } else { 0 });
                    if runlen == 4 {
                        datap += 4;
                        fp += 1;
                        continue;
                    }
                    mqc.set_curctx(T1_CTXNO_UNI);
                    mqc.encode(runlen >> 1);
                    mqc.encode(runlen & 1);
                } else {
                    runlen = 0;
                }

                self.enc_clnpass_step(
                    mqc,
                    fp,
                    datap + runlen as usize,
                    bpno,
                    one,
                    &mut nmsedec,
                    agg,
                    runlen,
                    4,
                    cblksty,
                );
                datap += 4;
                fp += 1;
            }
            fp += 2; // skip border columns
        }

        // Remaining rows (no aggregation)
        let k = h & !3;
        if k < h {
            let remaining = (h - k) as u32;
            for _i in 0..w {
                self.enc_clnpass_step(
                    mqc,
                    fp,
                    datap,
                    bpno,
                    one,
                    &mut nmsedec,
                    false,
                    0,
                    remaining,
                    cblksty,
                );
                datap += remaining as usize;
                fp += 1;
            }
        }

        nmsedec
    }

    // --- Decoding step helpers ---

    fn dec_sigpass_step_mqc(
        &mut self,
        mqc: &mut crate::coding::mqc::Mqc,
        fp: usize,
        datap: usize,
        oneplushalf: i32,
        ci: u32,
        vsc: bool,
    ) {
        let flags = self.flags[fp];
        let shift = ci * 3;

        if (flags & ((T1_SIGMA_THIS | T1_PI_THIS) << shift)) == 0
            && (flags & (T1_SIGMA_NEIGHBOURS << shift)) != 0
        {
            let ctxt1 = getctxno_zc(self.lut_ctxno_zc_orient_offset, flags >> shift) as usize;
            mqc.set_curctx(ctxt1);
            let v = mqc.decode();
            if v != 0 {
                let lu = getctxtno_sc_or_spb_index(
                    self.flags[fp],
                    self.flags[fp - 1],
                    self.flags[fp + 1],
                    ci,
                );
                let ctxt2 = getctxno_sc(lu) as usize;
                let spb = getspb(lu) as u32;
                mqc.set_curctx(ctxt2);
                let sign = mqc.decode() ^ spb;
                self.data[datap] = if sign != 0 { -oneplushalf } else { oneplushalf };
                let stride = self.flags_stride();
                update_flags(&mut self.flags, fp, ci, sign, stride, vsc);
            }
            self.flags[fp] |= T1_PI_THIS << shift;
        }
    }

    fn dec_sigpass_step_raw(
        &mut self,
        mqc: &mut crate::coding::mqc::Mqc,
        fp: usize,
        datap: usize,
        oneplushalf: i32,
        ci: u32,
        vsc: bool,
    ) {
        let flags = self.flags[fp];
        let shift = ci * 3;

        if (flags & ((T1_SIGMA_THIS | T1_PI_THIS) << shift)) == 0
            && (flags & (T1_SIGMA_NEIGHBOURS << shift)) != 0
        {
            if mqc.raw_decode() != 0 {
                let v = mqc.raw_decode();
                self.data[datap] = if v != 0 { -oneplushalf } else { oneplushalf };
                let stride = self.flags_stride();
                update_flags(&mut self.flags, fp, ci, v, stride, vsc);
            }
            self.flags[fp] |= T1_PI_THIS << shift;
        }
    }

    // --- Decoding passes ---

    /// Significance pass decoder, MQ mode (C: opj_t1_dec_sigpass_mqc).
    pub fn dec_sigpass_mqc(
        &mut self,
        mqc: &mut crate::coding::mqc::Mqc,
        bpno_plus_one: i32,
        cblksty: u32,
    ) {
        // C: one = 1 << bpno (where bpno is actually bpno_plus_one)
        let one = 1i32 << bpno_plus_one;
        let half = one >> 1;
        let oneplushalf = one | half;
        let w = self.w as usize;
        let h = self.h as usize;
        let stride = self.flags_stride();
        let vsc_flag = (cblksty & J2K_CCP_CBLKSTY_VSC) != 0;

        let mut datap = 0usize; // row-major: data[row * w + col]
        let mut fp = stride + 1;

        // Full 4-row strips
        for _k in (0..h & !3).step_by(4) {
            for _i in 0..w {
                if self.flags[fp] != 0 {
                    self.dec_sigpass_step_mqc(mqc, fp, datap, oneplushalf, 0, vsc_flag);
                    self.dec_sigpass_step_mqc(mqc, fp, datap + w, oneplushalf, 1, false);
                    self.dec_sigpass_step_mqc(mqc, fp, datap + 2 * w, oneplushalf, 2, false);
                    self.dec_sigpass_step_mqc(mqc, fp, datap + 3 * w, oneplushalf, 3, false);
                }
                datap += 1;
                fp += 1;
            }
            datap += 3 * w; // advance past the 3 remaining rows of this strip
            fp += 2;
        }

        // Remaining rows
        let k = h & !3;
        if k < h {
            for _i in 0..w {
                for j in 0..(h - k) {
                    let vsc = j == 0 && vsc_flag;
                    self.dec_sigpass_step_mqc(mqc, fp, datap + j * w, oneplushalf, j as u32, vsc);
                }
                datap += 1;
                fp += 1;
            }
        }
    }

    /// Significance pass decoder, RAW mode (C: opj_t1_dec_sigpass_raw).
    pub fn dec_sigpass_raw(
        &mut self,
        mqc: &mut crate::coding::mqc::Mqc,
        bpno_plus_one: i32,
        cblksty: u32,
    ) {
        let one = 1i32 << bpno_plus_one;
        let half = one >> 1;
        let oneplushalf = one | half;
        let w = self.w as usize;
        let h = self.h as usize;
        let stride = self.flags_stride();
        let vsc_flag = (cblksty & J2K_CCP_CBLKSTY_VSC) != 0;

        let mut datap = 0usize;
        let mut fp = stride + 1;

        for _k in (0..h & !3).step_by(4) {
            for _i in 0..w {
                if self.flags[fp] != 0 {
                    self.dec_sigpass_step_raw(mqc, fp, datap, oneplushalf, 0, vsc_flag);
                    self.dec_sigpass_step_raw(mqc, fp, datap + w, oneplushalf, 1, false);
                    self.dec_sigpass_step_raw(mqc, fp, datap + 2 * w, oneplushalf, 2, false);
                    self.dec_sigpass_step_raw(mqc, fp, datap + 3 * w, oneplushalf, 3, false);
                }
                datap += 1;
                fp += 1;
            }
            datap += 3 * w;
            fp += 2;
        }

        let k = h & !3;
        if k < h {
            for _i in 0..w {
                for j in 0..(h - k) {
                    let vsc = j == 0 && vsc_flag;
                    self.dec_sigpass_step_raw(mqc, fp, datap + j * w, oneplushalf, j as u32, vsc);
                }
                datap += 1;
                fp += 1;
            }
        }
    }

    // --- Refinement decoding step helpers ---

    fn dec_refpass_step_mqc(
        &mut self,
        mqc: &mut crate::coding::mqc::Mqc,
        fp: usize,
        datap: usize,
        poshalf: i32,
        ci: u32,
    ) {
        let flags = self.flags[fp];
        let shift = ci * 3;

        if (flags & ((T1_SIGMA_THIS | T1_PI_THIS) << shift)) == (T1_SIGMA_THIS << shift) {
            let ctxt = getctxno_mag(flags >> shift) as usize;
            mqc.set_curctx(ctxt);
            let v = mqc.decode();
            let is_negative = self.data[datap] < 0;
            self.data[datap] += if (v ^ (is_negative as u32)) != 0 {
                poshalf
            } else {
                -poshalf
            };
            self.flags[fp] |= T1_MU_THIS << shift;
        }
    }

    fn dec_refpass_step_raw(
        &mut self,
        mqc: &mut crate::coding::mqc::Mqc,
        fp: usize,
        datap: usize,
        poshalf: i32,
        ci: u32,
    ) {
        let flags = self.flags[fp];
        let shift = ci * 3;

        if (flags & ((T1_SIGMA_THIS | T1_PI_THIS) << shift)) == (T1_SIGMA_THIS << shift) {
            let v = mqc.raw_decode();
            let is_negative = self.data[datap] < 0;
            self.data[datap] += if (v ^ (is_negative as u32)) != 0 {
                poshalf
            } else {
                -poshalf
            };
            self.flags[fp] |= T1_MU_THIS << shift;
        }
    }

    // --- Refinement decoding passes ---

    /// Refinement pass decoder, MQ mode (C: opj_t1_dec_refpass_mqc).
    pub fn dec_refpass_mqc(&mut self, mqc: &mut crate::coding::mqc::Mqc, bpno_plus_one: i32) {
        let one = 1i32 << bpno_plus_one;
        let poshalf = one >> 1;
        let w = self.w as usize;
        let h = self.h as usize;
        let stride = self.flags_stride();

        let mut datap = 0usize;
        let mut fp = stride + 1;

        for _k in (0..h & !3).step_by(4) {
            for _i in 0..w {
                if self.flags[fp] != 0 {
                    self.dec_refpass_step_mqc(mqc, fp, datap, poshalf, 0);
                    self.dec_refpass_step_mqc(mqc, fp, datap + w, poshalf, 1);
                    self.dec_refpass_step_mqc(mqc, fp, datap + 2 * w, poshalf, 2);
                    self.dec_refpass_step_mqc(mqc, fp, datap + 3 * w, poshalf, 3);
                }
                datap += 1;
                fp += 1;
            }
            datap += 3 * w;
            fp += 2;
        }

        let k = h & !3;
        if k < h {
            for _i in 0..w {
                for j in 0..(h - k) {
                    self.dec_refpass_step_mqc(mqc, fp, datap + j * w, poshalf, j as u32);
                }
                datap += 1;
                fp += 1;
            }
        }
    }

    /// Refinement pass decoder, RAW mode (C: opj_t1_dec_refpass_raw).
    pub fn dec_refpass_raw(&mut self, mqc: &mut crate::coding::mqc::Mqc, bpno_plus_one: i32) {
        let one = 1i32 << bpno_plus_one;
        let poshalf = one >> 1;
        let w = self.w as usize;
        let h = self.h as usize;
        let stride = self.flags_stride();

        let mut datap = 0usize;
        let mut fp = stride + 1;

        for _k in (0..h & !3).step_by(4) {
            for _i in 0..w {
                if self.flags[fp] != 0 {
                    self.dec_refpass_step_raw(mqc, fp, datap, poshalf, 0);
                    self.dec_refpass_step_raw(mqc, fp, datap + w, poshalf, 1);
                    self.dec_refpass_step_raw(mqc, fp, datap + 2 * w, poshalf, 2);
                    self.dec_refpass_step_raw(mqc, fp, datap + 3 * w, poshalf, 3);
                }
                datap += 1;
                fp += 1;
            }
            datap += 3 * w;
            fp += 2;
        }

        let k = h & !3;
        if k < h {
            for _i in 0..w {
                for j in 0..(h - k) {
                    self.dec_refpass_step_raw(mqc, fp, datap + j * w, poshalf, j as u32);
                }
                datap += 1;
                fp += 1;
            }
        }
    }

    // --- Clean-up decoding step helpers ---

    /// Clean-up pass step (decoder) for one coefficient (C: opj_t1_dec_clnpass_step_macro).
    ///
    /// `check_flags`: if true, skip if already significant/PI. If false, always process.
    /// `partial`: if true, skip ZC decode (coefficient is known significant from aggregation).
    #[allow(clippy::too_many_arguments)]
    fn dec_clnpass_step(
        &mut self,
        mqc: &mut crate::coding::mqc::Mqc,
        fp: usize,
        datap: usize,
        oneplushalf: i32,
        ci: u32,
        check_flags: bool,
        partial: bool,
        vsc: bool,
    ) {
        if check_flags && (self.flags[fp] & ((T1_SIGMA_THIS | T1_PI_THIS) << (ci * 3))) != 0 {
            return;
        }

        if !partial {
            let ctxt1 =
                getctxno_zc(self.lut_ctxno_zc_orient_offset, self.flags[fp] >> (ci * 3)) as usize;
            mqc.set_curctx(ctxt1);
            let v = mqc.decode();
            if v == 0 {
                return;
            }
        }

        // Coefficient is significant: decode sign
        let lu =
            getctxtno_sc_or_spb_index(self.flags[fp], self.flags[fp - 1], self.flags[fp + 1], ci);
        let ctxt2 = getctxno_sc(lu) as usize;
        mqc.set_curctx(ctxt2);
        let v = mqc.decode() ^ getspb(lu) as u32;
        self.data[datap] = if v != 0 { -oneplushalf } else { oneplushalf };
        let stride = self.flags_stride();
        update_flags(&mut self.flags, fp, ci, v, stride, vsc);
    }

    // --- Clean-up decoding pass ---

    /// Clean-up pass decoder (C: opj_t1_dec_clnpass).
    pub fn dec_clnpass(
        &mut self,
        mqc: &mut crate::coding::mqc::Mqc,
        bpno_plus_one: i32,
        cblksty: u32,
    ) {
        let one = 1i32 << bpno_plus_one;
        let half = one >> 1;
        let oneplushalf = one | half;
        let w = self.w as usize;
        let h = self.h as usize;
        let stride = self.flags_stride();
        let vsc = (cblksty & J2K_CCP_CBLKSTY_VSC) != 0;

        let mut datap = 0usize;
        let mut fp = stride + 1;

        // Full 4-row strips
        for _k in (0..h & !3).step_by(4) {
            for _i in 0..w {
                if self.flags[fp] == 0 {
                    // Aggregation: all flags are zero
                    mqc.set_curctx(T1_CTXNO_AGG);
                    let v = mqc.decode();
                    if v == 0 {
                        // No significant coefficients in this column
                        datap += 1;
                        fp += 1;
                        continue;
                    }
                    // Decode run length
                    mqc.set_curctx(T1_CTXNO_UNI);
                    let rl_hi = mqc.decode();
                    let rl_lo = mqc.decode();
                    let runlen = (rl_hi << 1) | rl_lo;

                    // Fallthrough: process from runlen to 3
                    // runlen is the first significant sample (partial=true for it)
                    let mut partial = true;
                    for ci in runlen..4 {
                        let vsc_ci = vsc && ci == 0;
                        self.dec_clnpass_step(
                            mqc,
                            fp,
                            datap + (ci as usize) * w,
                            oneplushalf,
                            ci,
                            false,
                            partial,
                            vsc_ci,
                        );
                        partial = false;
                    }
                } else {
                    // Non-zero flags: standard step for each ci
                    self.dec_clnpass_step(mqc, fp, datap, oneplushalf, 0, true, false, vsc);
                    self.dec_clnpass_step(mqc, fp, datap + w, oneplushalf, 1, true, false, false);
                    self.dec_clnpass_step(
                        mqc,
                        fp,
                        datap + 2 * w,
                        oneplushalf,
                        2,
                        true,
                        false,
                        false,
                    );
                    self.dec_clnpass_step(
                        mqc,
                        fp,
                        datap + 3 * w,
                        oneplushalf,
                        3,
                        true,
                        false,
                        false,
                    );
                }
                // Clear PI flags
                self.flags[fp] &= !(T1_PI_0 | T1_PI_1 | T1_PI_2 | T1_PI_3);
                datap += 1;
                fp += 1;
            }
            datap += 3 * w;
            fp += 2;
        }

        // Remaining rows (no aggregation)
        let k = h & !3;
        if k < h {
            for _i in 0..w {
                for j in 0..(h - k) {
                    let vsc_j = vsc && j == 0;
                    self.dec_clnpass_step(
                        mqc,
                        fp,
                        datap + j * w,
                        oneplushalf,
                        j as u32,
                        true,
                        false,
                        vsc_j,
                    );
                }
                self.flags[fp] &= !(T1_PI_0 | T1_PI_1 | T1_PI_2 | T1_PI_3);
                datap += 1;
                fp += 1;
            }
        }

        // SEGSYM check
        if (cblksty & J2K_CCP_CBLKSTY_SEGSYM) != 0 {
            mqc.set_curctx(T1_CTXNO_UNI);
            let b0 = mqc.decode();
            let b1 = mqc.decode();
            let b2 = mqc.decode();
            let b3 = mqc.decode();
            let _sym = (b0 << 3) | (b1 << 2) | (b2 << 1) | b3;
            // C version: warn if sym != 0xa
        }
    }

    // --- Full code-block encode/decode ---

    /// Determine if a pass should be terminated (C: opj_t1_enc_is_term_pass).
    fn is_term_pass(numbps: u32, cblksty: u32, bpno: i32, passtype: u32) -> bool {
        // Last cleanup pass
        if passtype == 2 && bpno == 0 {
            return true;
        }

        if (cblksty & J2K_CCP_CBLKSTY_TERMALL) != 0 {
            return true;
        }

        if (cblksty & J2K_CCP_CBLKSTY_LAZY) != 0 {
            // Terminate the 4th cleanup pass
            if bpno == (numbps as i32 - 4) && passtype == 2 {
                return true;
            }
            // Beyond that, terminate refpass + clnpass (passtype > 0)
            if bpno < (numbps as i32 - 4) && passtype > 0 {
                return true;
            }
        }

        false
    }

    /// Encode a code-block (C: opj_t1_encode_cblk).
    ///
    /// Data must already be in zigzag layout, shifted by T1_NMSEDEC_FRACBITS,
    /// in two's complement. Returns (passes, cumulative_wmsedec).
    #[allow(clippy::too_many_arguments)]
    pub fn encode_cblk(
        &mut self,
        buf: &mut [u8],
        orient: u32,
        compno: u32,
        level: u32,
        qmfbid: u32,
        stepsize: f64,
        cblksty: u32,
        numcomps: u32,
        mct_norms: Option<&[f64]>,
    ) -> (Vec<TcdPass>, f64) {
        let _ = numcomps; // unused, matches C

        self.set_orient(orient);

        // --- Convert to SMR, find max ---
        let mut max = 0i32;
        for i in 0..self.data.len() {
            let tmp = self.data[i];
            if tmp < 0 {
                let clamped = if tmp == i32::MIN { i32::MIN + 1 } else { tmp };
                max = max.max(-clamped);
                self.data[i] = to_smr(clamped);
            } else {
                max = max.max(tmp);
            }
        }

        let numbps = if max != 0 {
            (int_floorlog2(max) + 1 - T1_NMSEDEC_FRACBITS as i32) as u32
        } else {
            0
        };

        if numbps == 0 {
            return (Vec::new(), 0.0);
        }

        // --- Init MQC ---
        let mut mqc = crate::coding::mqc::Mqc::new(buf);
        mqc.reset_states();
        mqc.set_state(T1_CTXNO_UNI, 0, 46);
        mqc.set_state(T1_CTXNO_AGG, 0, 3);
        mqc.set_state(T1_CTXNO_ZC, 0, 4);
        mqc.init_enc();

        let mut bpno = numbps as i32 - 1;
        let mut passtype = 2u32;
        let mut cumwmsedec = 0.0f64;
        let mut passes = Vec::new();

        while bpno >= 0 {
            let pass_type_byte = if bpno < (numbps as i32 - 4)
                && passtype < 2
                && (cblksty & J2K_CCP_CBLKSTY_LAZY) != 0
            {
                T1_TYPE_RAW
            } else {
                T1_TYPE_MQ
            };

            // Re-init after previous termination
            if !passes.is_empty() {
                let prev: &TcdPass = passes.last().unwrap();
                if prev.term {
                    if pass_type_byte == T1_TYPE_RAW {
                        mqc.bypass_init_enc();
                    } else {
                        mqc.restart_init_enc();
                    }
                }
            }

            let nmsedec = match passtype {
                0 => self.enc_sigpass(&mut mqc, bpno, pass_type_byte, cblksty),
                1 => self.enc_refpass(&mut mqc, bpno, pass_type_byte),
                2 => {
                    let n = self.enc_clnpass(&mut mqc, bpno, cblksty);
                    if (cblksty & J2K_CCP_CBLKSTY_SEGSYM) != 0 {
                        mqc.segmark_enc();
                    }
                    n
                }
                _ => unreachable!(),
            };

            let tempwmsedec = t1_getwmsedec(
                nmsedec, compno, level, orient, bpno, qmfbid, stepsize, mct_norms,
            );
            cumwmsedec += tempwmsedec;

            let term = Self::is_term_pass(numbps, cblksty, bpno, passtype);

            let rate = if term {
                if pass_type_byte == T1_TYPE_RAW {
                    mqc.bypass_flush_enc((cblksty & J2K_CCP_CBLKSTY_PTERM) != 0);
                } else if (cblksty & J2K_CCP_CBLKSTY_PTERM) != 0 {
                    mqc.erterm_enc();
                } else {
                    mqc.flush();
                }
                mqc.num_bytes() as u32
            } else {
                let rate_extra_bytes = if pass_type_byte == T1_TYPE_RAW {
                    mqc.bypass_get_extra_bytes((cblksty & J2K_CCP_CBLKSTY_PTERM) != 0)
                } else {
                    3
                };
                mqc.num_bytes() as u32 + rate_extra_bytes
            };

            passes.push(TcdPass {
                rate,
                distortion_decrease: cumwmsedec,
                len: 0, // computed below
                term,
            });

            passtype += 1;
            if passtype == 3 {
                passtype = 0;
                bpno -= 1;
            }

            // Code-switch RESET
            if (cblksty & J2K_CCP_CBLKSTY_RESET) != 0 {
                mqc.reset_enc();
            }
        }

        // --- Post-process passes ---
        if !passes.is_empty() {
            // Ensure rates are monotonically increasing (backward scan)
            let last_pass_rate = mqc.num_bytes() as u32;
            let mut current_max = last_pass_rate;
            for pass in passes.iter_mut().rev() {
                if pass.rate > current_max {
                    pass.rate = current_max;
                } else {
                    current_max = pass.rate;
                }
            }

            // Prevent 0xFF as last data byte of a pass, and compute len
            for passno in 0..passes.len() {
                // data is written starting at buf[1] (buf[0] is padding)
                if passes[passno].rate > 0 {
                    let byte_idx = passes[passno].rate as usize; // offset from start=1 => buf[rate]
                    if buf[byte_idx] == 0xFF {
                        passes[passno].rate -= 1;
                    }
                }
                let prev_rate = if passno == 0 {
                    0
                } else {
                    passes[passno - 1].rate
                };
                passes[passno].len = passes[passno].rate - prev_rate;
            }
        }

        (passes, cumwmsedec)
    }

    /// Decode a code-block (C: opj_t1_decode_cblk).
    pub fn decode_cblk(
        &mut self,
        segments: &[DecodeSegment],
        orient: u32,
        roishift: u32,
        numbps: u32,
        cblksty: u32,
    ) -> Result<()> {
        self.set_orient(orient);

        // Init MQC contexts
        // We need a temporary buffer to back the MQC decoder. We'll process segment by segment.
        let mut bpno_plus_one = (roishift + numbps) as i32;
        if bpno_plus_one >= 31 {
            return Err(crate::error::Error::InvalidInput(format!(
                "decode_cblk: unsupported bpno_plus_one = {} >= 31",
                bpno_plus_one
            )));
        }
        let mut passtype = 2u32;

        // Concatenate all segment data into a contiguous buffer (+ extra bytes for decoder)
        let total_len: usize = segments.iter().map(|s| s.data.len()).sum();
        let mut cblkdata = vec![0u8; total_len + crate::types::COMMON_CBLK_DATA_EXTRA];
        let mut offset = 0;
        for seg in segments {
            cblkdata[offset..offset + seg.data.len()].copy_from_slice(seg.data);
            offset += seg.data.len();
        }

        let mut cblkdataindex = 0usize;

        for seg in segments {
            let seg_len = seg.data.len();

            // Determine type for first pass in this segment
            let pass_type_byte = if bpno_plus_one <= (numbps as i32 - 4)
                && passtype < 2
                && (cblksty & J2K_CCP_CBLKSTY_LAZY) != 0
            {
                T1_TYPE_RAW
            } else {
                T1_TYPE_MQ
            };

            let mut mqc = crate::coding::mqc::Mqc::new(&mut cblkdata[cblkdataindex..]);
            mqc.reset_states();
            mqc.set_state(T1_CTXNO_UNI, 0, 46);
            mqc.set_state(T1_CTXNO_AGG, 0, 3);
            mqc.set_state(T1_CTXNO_ZC, 0, 4);

            if pass_type_byte == T1_TYPE_RAW {
                mqc.raw_init_dec(seg_len);
            } else {
                mqc.init_dec(seg_len);
            }

            for _passno in 0..seg.num_passes {
                if bpno_plus_one < 1 {
                    break;
                }

                match passtype {
                    0 => {
                        if pass_type_byte == T1_TYPE_RAW {
                            self.dec_sigpass_raw(&mut mqc, bpno_plus_one, cblksty);
                        } else {
                            self.dec_sigpass_mqc(&mut mqc, bpno_plus_one, cblksty);
                        }
                    }
                    1 => {
                        if pass_type_byte == T1_TYPE_RAW {
                            self.dec_refpass_raw(&mut mqc, bpno_plus_one);
                        } else {
                            self.dec_refpass_mqc(&mut mqc, bpno_plus_one);
                        }
                    }
                    2 => {
                        self.dec_clnpass(&mut mqc, bpno_plus_one, cblksty);
                    }
                    _ => unreachable!(),
                }

                // Code-switch RESET
                if (cblksty & J2K_CCP_CBLKSTY_RESET) != 0 && pass_type_byte == T1_TYPE_MQ {
                    mqc.reset_states();
                    mqc.set_state(T1_CTXNO_UNI, 0, 46);
                    mqc.set_state(T1_CTXNO_AGG, 0, 3);
                    mqc.set_state(T1_CTXNO_ZC, 0, 4);
                }

                passtype += 1;
                if passtype == 3 {
                    passtype = 0;
                    bpno_plus_one -= 1;
                }
            }

            mqc.finish_dec();
            cblkdataindex += seg_len;
        }

        Ok(())
    }

    /// Decode an HT (High Throughput) code-block (C: opj_t1_ht_decode_cblk).
    ///
    /// Dispatched when `cblksty & J2K_CCP_CBLKSTY_HT != 0`. HT codeblocks
    /// have a different segment structure: segment 0 = cleanup (1 pass),
    /// segment 1 = SPP+MRP (remaining passes).
    pub fn decode_cblk_ht(
        &mut self,
        segments: &[DecodeSegment],
        numbps: u32,
        roishift: u32,
        zero_bplanes: u32,
    ) -> Result<()> {
        if segments.is_empty() {
            return Ok(());
        }

        let p = roishift + numbps;
        if p > 31 {
            return Err(crate::error::Error::InvalidInput(format!(
                "decode_cblk_ht: p ({p}) must be <= 31"
            )));
        }

        // Concatenate segment data
        let total_len: usize = segments.iter().map(|s| s.data.len()).sum();
        let mut cblkdata = vec![0u8; total_len];
        let mut offset = 0;
        for seg in segments {
            cblkdata[offset..offset + seg.data.len()].copy_from_slice(seg.data);
            offset += seg.data.len();
        }

        // Build lengths array: segment 0 = cleanup, segment 1 = SPP+MRP
        let mut num_passes = 0u32;
        let mut lengths = Vec::new();
        for seg in segments {
            num_passes += seg.num_passes;
            lengths.push(seg.data.len() as u32);
        }

        let result = crate::coding::ht_dec::ht_decode_cblk(
            &cblkdata,
            self.w,
            self.h,
            num_passes,
            &lengths,
            zero_bplanes,
            p,
        )?;

        // Copy HT decode results into T1's data buffer
        let n = result.len().min(self.data.len());
        self.data[..n].copy_from_slice(&result[..n]);

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Batch codeblock decode/encode (C: opj_t1_decode_cblks / opj_t1_encode_cblks)
// ---------------------------------------------------------------------------

/// Decode job for one codeblock.
struct CblkDecodeJob {
    /// Concatenated segment data.
    data: Vec<u8>,
    /// Per-segment: (offset, length, num_passes).
    seg_info: Vec<(usize, usize, u32)>,
    /// Codeblock dimensions.
    w: u32,
    h: u32,
    /// Sub-band orientation.
    orient: u32,
    /// ROI shift.
    roishift: u32,
    /// Number of significant bit-planes.
    numbps: u32,
    /// Code-block coding style flags.
    cblksty: u32,
    /// Zero bit-planes (for HT: band.numbps - cblk.numbps).
    zero_bplanes: u32,
    /// Location in TCD hierarchy for write-back.
    comp: usize,
    res: usize,
    band: usize,
    prec: usize,
    cblk: usize,
}

/// Process a single codeblock decode job.
fn decode_one_cblk(job: &CblkDecodeJob) -> Result<Vec<i32>> {
    let mut t1 = T1::new(false);
    t1.allocate_buffers(job.w, job.h)?;

    if job.seg_info.is_empty() {
        return Ok(t1.data);
    }

    let mut segments: Vec<DecodeSegment> = Vec::with_capacity(job.seg_info.len());
    for &(off, len, np) in &job.seg_info {
        if off.checked_add(len).is_none_or(|end| end > job.data.len()) {
            return Err(Error::InvalidInput(format!(
                "segment offset {off}+{len} exceeds data length {}",
                job.data.len()
            )));
        }
        segments.push(DecodeSegment {
            data: &job.data[off..off + len],
            num_passes: np,
        });
    }

    let is_ht = (job.cblksty & J2K_CCP_CBLKSTY_HT) != 0;
    if is_ht {
        t1.decode_cblk_ht(&segments, job.numbps, job.roishift, job.zero_bplanes)?;
    } else {
        t1.decode_cblk(&segments, job.orient, job.roishift, job.numbps, job.cblksty)?;
    }

    Ok(t1.data)
}

/// Encode job for one codeblock.
struct CblkEncodeJob {
    /// Coefficient data in stripe-column layout, shifted by T1_NMSEDEC_FRACBITS.
    /// Stripe-column: for each 4-row stripe, for each column, sequential rows.
    t1_data: Vec<i32>,
    /// Codeblock dimensions.
    w: u32,
    h: u32,
    /// Sub-band orientation.
    orient: u32,
    /// Component number.
    compno: u32,
    /// Resolution level.
    level: u32,
    /// Wavelet filter (0=9-7, 1=5-3).
    qmfbid: u32,
    /// Quantization step size.
    stepsize: f64,
    /// Code-block coding style flags.
    cblksty: u32,
    /// Total number of components.
    numcomps: u32,
    /// MCT normalization coefficients.
    mct_norms: Option<Vec<f64>>,
    /// Location in TCD hierarchy for write-back.
    comp: usize,
    res: usize,
    band: usize,
    prec: usize,
    cblk: usize,
}

/// Result of encoding one codeblock.
struct CblkEncodeResult {
    data: Vec<u8>,
    passes: Vec<TcdPass>,
    numbps: u32,
}

/// Process a single codeblock encode job.
fn encode_one_cblk(job: &CblkEncodeJob) -> Result<CblkEncodeResult> {
    let mut t1 = T1::new(true);
    t1.allocate_buffers(job.w, job.h)?;
    t1.data[..job.t1_data.len()].copy_from_slice(&job.t1_data);

    let mut enc_buf = vec![0u8; (job.w * job.h * 4 + 1024) as usize];
    let (passes, _cumwmsedec) = t1.encode_cblk(
        &mut enc_buf,
        job.orient,
        job.compno,
        job.level,
        job.qmfbid,
        job.stepsize,
        job.cblksty,
        job.numcomps,
        job.mct_norms.as_deref(),
    );

    let total_rate = passes.last().map_or(0, |p| p.rate as usize);
    let data = if total_rate > 0 {
        enc_buf[1..1 + total_rate].to_vec()
    } else {
        Vec::new()
    };

    // Compute numbps from data
    let max_abs = t1.data.iter().map(|&v| v & 0x7FFFFFFF).max().unwrap_or(0);
    let numbps = if max_abs != 0 {
        (int_floorlog2(max_abs) + 1 - T1_NMSEDEC_FRACBITS as i32) as u32
    } else {
        0
    };

    Ok(CblkEncodeResult {
        data,
        passes,
        numbps,
    })
}

/// Decode all codeblocks in a tile (C: opj_t1_decode_cblks).
///
/// Walks the TCD hierarchy and decodes each codeblock, writing results
/// to `TcdCblkDec::decoded_data`. When the `parallel` feature is enabled,
/// codeblocks are decoded in parallel using rayon.
pub fn t1_decode_cblks(tile: &mut TcdTile, tcp: &TileCodingParameters) -> Result<()> {
    // 1. Collect decode jobs
    let mut jobs: Vec<CblkDecodeJob> = Vec::new();

    for (compno, comp) in tile.comps.iter().enumerate() {
        let tccp = tcp
            .tccps
            .get(compno)
            .ok_or_else(|| Error::InvalidInput(format!("missing TCCP for component {compno}")))?;

        for (resno, res) in comp.resolutions.iter().enumerate() {
            for (bandno, band) in res.bands.iter().enumerate() {
                if band.is_empty() {
                    continue;
                }
                let orient = if resno == 0 { 0 } else { bandno as u32 + 1 };

                for (precno, prec) in band.precincts.iter().enumerate() {
                    if let TcdCodeBlocks::Dec(cblks) = &prec.cblks {
                        for (cblkno, cblk) in cblks.iter().enumerate() {
                            let w = (cblk.x1 - cblk.x0).max(0) as u32;
                            let h = (cblk.y1 - cblk.y0).max(0) as u32;
                            if w == 0 || h == 0 {
                                continue;
                            }

                            // Concatenate chunks into contiguous buffer
                            let total_len: usize = cblk.chunks.iter().map(|c| c.len as usize).sum();
                            let mut data = vec![0u8; total_len + COMMON_CBLK_DATA_EXTRA];
                            let mut off = 0;
                            for chunk in &cblk.chunks {
                                let clen = (chunk.len as usize).min(chunk.data.len());
                                data[off..off + clen].copy_from_slice(&chunk.data[..clen]);
                                off += clen;
                            }

                            // Build segment info
                            let mut seg_off = 0usize;
                            let mut seg_info = Vec::new();
                            for seg in &cblk.segs {
                                let slen = seg.len as usize;
                                if seg.real_num_passes > 0 {
                                    seg_info.push((seg_off, slen, seg.real_num_passes));
                                }
                                seg_off += slen;
                            }

                            let zero_bplanes = (band.numbps - cblk.numbps as i32).max(0) as u32;

                            jobs.push(CblkDecodeJob {
                                data,
                                seg_info,
                                w,
                                h,
                                orient,
                                roishift: tccp.roishift as u32,
                                numbps: cblk.numbps,
                                cblksty: tccp.cblksty,
                                zero_bplanes,
                                comp: compno,
                                res: resno,
                                band: bandno,
                                prec: precno,
                                cblk: cblkno,
                            });
                        }
                    }
                }
            }
        }
    }

    // 2. Process jobs
    // 2. Process jobs and write back results
    let results: Vec<Result<Vec<i32>>>;

    #[cfg(feature = "parallel")]
    {
        results = jobs.par_iter().map(decode_one_cblk).collect();
    }

    #[cfg(not(feature = "parallel"))]
    {
        results = jobs.iter().map(decode_one_cblk).collect();
    }

    for (job, result) in jobs.iter().zip(results) {
        let decoded_data = result?;
        if let TcdCodeBlocks::Dec(cblks) =
            &mut tile.comps[job.comp].resolutions[job.res].bands[job.band].precincts[job.prec].cblks
        {
            cblks[job.cblk].decoded_data = Some(decoded_data);
        }
    }

    Ok(())
}

/// Encode all codeblocks in a tile (C: opj_t1_encode_cblks).
///
/// Walks the TCD hierarchy and encodes each codeblock. Coefficient data
/// is read from `TcdTileComp::data` in row-major order and converted to
/// the T1 zigzag layout. When the `parallel` feature is enabled,
/// codeblocks are encoded in parallel using rayon.
#[allow(clippy::too_many_arguments)]
pub fn t1_encode_cblks(
    tile: &mut TcdTile,
    tcp: &TileCodingParameters,
    mct_norms: Option<&[f64]>,
    numcomps: u32,
) -> Result<()> {
    // 1. Collect encode jobs
    let mut jobs: Vec<CblkEncodeJob> = Vec::new();

    for (compno, comp) in tile.comps.iter().enumerate() {
        let tccp = tcp
            .tccps
            .get(compno)
            .ok_or_else(|| Error::InvalidInput(format!("missing TCCP for component {compno}")))?;

        let comp_w = (comp.x1 - comp.x0).max(0) as usize;

        for (resno, res) in comp.resolutions.iter().enumerate() {
            let level = (comp.numresolutions as usize - 1 - resno) as u32;

            for (bandno, band) in res.bands.iter().enumerate() {
                if band.is_empty() {
                    continue;
                }
                let orient = if resno == 0 { 0 } else { bandno as u32 + 1 };

                for (precno, prec) in band.precincts.iter().enumerate() {
                    if let TcdCodeBlocks::Enc(cblks) = &prec.cblks {
                        for (cblkno, cblk) in cblks.iter().enumerate() {
                            let cblk_w = (cblk.x1 - cblk.x0).max(0) as u32;
                            let cblk_h = (cblk.y1 - cblk.y0).max(0) as u32;
                            if cblk_w == 0 || cblk_h == 0 {
                                continue;
                            }

                            // Copy from tile component (row-major) to stripe-column layout
                            // matching the encoder's scan order: for each 4-row stripe,
                            // for each column, sequential rows within the stripe.
                            let cblk_x0 = (cblk.x0 - comp.x0) as usize;
                            let cblk_y0 = (cblk.y0 - comp.y0) as usize;
                            let h = cblk_h as usize;
                            let w = cblk_w as usize;
                            let mut t1_data = vec![0i32; w * h];
                            let mut dst_idx = 0;
                            for stripe_start in (0..h).step_by(4) {
                                let stripe_h = 4.min(h - stripe_start);
                                for c in 0..w {
                                    for r in 0..stripe_h {
                                        let row = stripe_start + r;
                                        let src_idx = (cblk_y0 + row) * comp_w + (cblk_x0 + c);
                                        let val = if src_idx < comp.data.len() {
                                            comp.data[src_idx]
                                        } else {
                                            0
                                        };
                                        t1_data[dst_idx] = val << T1_NMSEDEC_FRACBITS;
                                        dst_idx += 1;
                                    }
                                }
                            }

                            let stepsize_idx = if resno == 0 {
                                0
                            } else {
                                3 * (resno - 1) + bandno + 1
                            };
                            let stepsize = if stepsize_idx < tccp.stepsizes.len() {
                                let ss = &tccp.stepsizes[stepsize_idx];
                                (1.0 + ss.mant as f64 / 2048.0)
                                    * f64::powi(2.0, tccp.numgbits as i32 - ss.expn)
                            } else {
                                1.0
                            };

                            jobs.push(CblkEncodeJob {
                                t1_data,
                                w: cblk_w,
                                h: cblk_h,
                                orient,
                                compno: compno as u32,
                                level,
                                qmfbid: tccp.qmfbid,
                                stepsize,
                                cblksty: tccp.cblksty,
                                numcomps,
                                mct_norms: mct_norms.map(|n| n.to_vec()),
                                comp: compno,
                                res: resno,
                                band: bandno,
                                prec: precno,
                                cblk: cblkno,
                            });
                        }
                    }
                }
            }
        }
    }

    // 2. Process jobs and write back results
    let results: Result<Vec<CblkEncodeResult>>;

    #[cfg(feature = "parallel")]
    {
        results = jobs.par_iter().map(encode_one_cblk).collect();
    }

    #[cfg(not(feature = "parallel"))]
    {
        results = jobs.iter().map(encode_one_cblk).collect();
    }
    for (job, result) in jobs.iter().zip(results?) {
        if let TcdCodeBlocks::Enc(cblks) =
            &mut tile.comps[job.comp].resolutions[job.res].bands[job.band].precincts[job.prec].cblks
        {
            cblks[job.cblk].data = result.data;
            cblks[job.cblk].passes = result.passes;
            cblks[job.cblk].numbps = result.numbps;
            cblks[job.cblk].totalpasses = cblks[job.cblk].passes.len() as u32;
        }
    }

    Ok(())
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
pub fn update_flags(flags: &mut [u32], flagsp: usize, ci: u32, s: u32, stride: usize, vsc: bool) {
    // West neighbour (flagsp-1): set SIGMA_E (= SIGMA_5) to mark "east is significant"
    flags[flagsp - 1] |= T1_SIGMA_5 << (3 * ci);

    // Mark target as significant + set sign
    flags[flagsp] |= ((s << T1_CHI_1_I) | T1_SIGMA_4) << (3 * ci);

    // East neighbour (flagsp+1): set SIGMA_W (= SIGMA_3) to mark "west is significant"
    flags[flagsp + 1] |= T1_SIGMA_3 << (3 * ci);

    // North: NW, N, NE (only for ci==0 and not VSC)
    if ci == 0 && !vsc {
        let north = flagsp - stride;
        flags[north] |= (s << T1_CHI_5_I) | T1_SIGMA_16;
        flags[north - 1] |= T1_SIGMA_17;
        flags[north + 1] |= T1_SIGMA_15;
    }

    // South: SW, S, SE (only for ci==3)
    if ci == 3 {
        let south = flagsp + stride;
        flags[south] |= (s << T1_CHI_0_I) | T1_SIGMA_1;
        flags[south - 1] |= T1_SIGMA_2;
        flags[south + 1] |= T1_SIGMA_0;
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
    fn update_flags_sets_sigma_this() {
        let (mut flags, fp, stride) = setup_flags(8, 8);
        update_flags(&mut flags, fp, 0, 0, stride, false);
        // T1_SIGMA_THIS (T1_SIGMA_4) should be set for ci=0
        assert_ne!(flags[fp] & (T1_SIGMA_4), 0);
    }

    #[test]
    fn update_flags_sets_chi_sign() {
        let (mut flags, fp, stride) = setup_flags(8, 8);
        // s=1 (negative sign)
        update_flags(&mut flags, fp, 0, 1, stride, false);
        // CHI_1 should be set (sign=1 for ci=0)
        assert_ne!(flags[fp] & (T1_CHI_1), 0);
    }

    #[test]
    fn update_flags_propagates_east_west() {
        let (mut flags, fp, stride) = setup_flags(8, 8);
        update_flags(&mut flags, fp, 0, 0, stride, false);
        // West neighbour (flagsp[-1]) should have T1_SIGMA_E (= T1_SIGMA_5) set
        assert_ne!(flags[fp - 1] & (T1_SIGMA_5), 0);
        // East neighbour (flagsp[+1]) should have T1_SIGMA_W (= T1_SIGMA_3) set
        assert_ne!(flags[fp + 1] & (T1_SIGMA_3), 0);
    }

    #[test]
    fn update_flags_propagates_north() {
        let (mut flags, fp, stride) = setup_flags(8, 8);
        // ci=0, vsc=false: should propagate north
        update_flags(&mut flags, fp, 0, 0, stride, false);
        let north = fp - stride;
        // T1_SIGMA_16 (south significance in north neighbour's row)
        assert_ne!(flags[north] & T1_SIGMA_16, 0);
    }

    #[test]
    fn update_flags_vsc_blocks_north() {
        let (mut flags, fp, stride) = setup_flags(8, 8);
        // ci=0, vsc=true: should NOT propagate north
        update_flags(&mut flags, fp, 0, 0, stride, true);
        let north = fp - stride;
        assert_eq!(flags[north] & T1_SIGMA_16, 0);
    }

    #[test]
    fn update_flags_propagates_south() {
        let (mut flags, fp, stride) = setup_flags(8, 8);
        // ci=3: should propagate south
        update_flags(&mut flags, fp, 3, 0, stride, false);
        let south = fp + stride;
        // T1_SIGMA_1 (north significance in south neighbour's row)
        assert_ne!(flags[south] & T1_SIGMA_1, 0);
    }

    #[test]
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

    // --- Significance pass tests ---

    /// Helper: initialize MQC contexts for T1 encoding/decoding.
    fn init_t1_mqc_contexts(mqc: &mut crate::coding::mqc::Mqc) {
        mqc.reset_states();
        mqc.set_state(T1_CTXNO_UNI, 0, 46);
        mqc.set_state(T1_CTXNO_AGG, 0, 3);
        mqc.set_state(T1_CTXNO_ZC, 0, 4);
    }

    #[test]
    fn sigpass_encode_decode_roundtrip() {
        use crate::coding::mqc::Mqc;

        // Setup: 4x4 block. Coefficient at (col=0, row=0) is already significant
        // (simulating a prior clean-up pass). Its east neighbor (col=1, row=0)
        // has a non-zero bit at the test bitplane and should be coded by sigpass.
        let bpno: i32 = 3;
        let one = 1i32 << (bpno + T1_NMSEDEC_FRACBITS as i32);

        // --- Encode ---
        let mut enc = T1::new(true);
        enc.allocate_buffers(4, 4).unwrap();
        enc.set_orient(0);

        // Mark (col=0, row=0) as already significant in flags
        let fp00 = enc.flags_index(0, 0);
        let stride = enc.flags_stride();
        update_flags(&mut enc.flags, fp00, 0, 0, stride, false);

        // Encoder data is zigzag: col 0 = data[0..4], col 1 = data[4..8]
        // Set (col=1, row=0) = data[4] to positive value with bit at bpno set
        enc.data[4] = one; // positive, SMR same as two's complement

        let mut enc_buf = vec![0u8; 256];
        let num_bytes;
        {
            let mut mqc = Mqc::new(&mut enc_buf);
            init_t1_mqc_contexts(&mut mqc);
            mqc.init_enc();
            let nmsedec = enc.enc_sigpass(&mut mqc, bpno, T1_TYPE_MQ, 0);
            mqc.flush();
            assert!(nmsedec >= 0);
            num_bytes = mqc.num_bytes();
            assert!(num_bytes > 0);
        }

        // --- Decode ---
        // Extract encoded data (encoder writes starting at buf[1])
        let mut dec_buf = vec![0u8; 256];
        dec_buf[..num_bytes].copy_from_slice(&enc_buf[1..1 + num_bytes]);

        let mut dec = T1::new(false);
        dec.allocate_buffers(4, 4).unwrap();
        dec.set_orient(0);
        // Same prior flag state
        let fp00 = dec.flags_index(0, 0);
        let stride = dec.flags_stride();
        update_flags(&mut dec.flags, fp00, 0, 0, stride, false);

        {
            let mut mqc = Mqc::new(&mut dec_buf);
            init_t1_mqc_contexts(&mut mqc);
            mqc.init_dec(num_bytes);
            dec.dec_sigpass_mqc(&mut mqc, bpno + 1, 0);
            mqc.finish_dec();
        }

        // Verify: (col=1, row=0) in row-major = data[0*4 + 1] = data[1]
        // Decoder uses one = 1 << bpno_plus_one (2x scale), so
        // oneplushalf = (1 << (bpno+1)) | (1 << bpno).
        let oneplushalf = (1i32 << (bpno + 1)) | (1i32 << bpno);
        assert_eq!(dec.data[1], oneplushalf);
    }

    // --- Refinement pass tests ---

    #[test]
    fn refpass_encode_decode_roundtrip() {
        use crate::coding::mqc::Mqc;

        // Setup: 4x4 block. Coefficient at (col=1, row=0) was made significant
        // at a higher bitplane (bpno=5). We now run refpass at a lower bitplane
        // (bpno=3) to refine the magnitude bit.
        let higher_bpno: i32 = 5;
        let refine_bpno: i32 = 3;

        // --- Encode ---
        let mut enc = T1::new(true);
        enc.allocate_buffers(4, 4).unwrap();
        enc.set_orient(0);

        // Encoder zigzag layout: col 1 = data[4..8], row 0 = data[4]
        // Set coefficient to a value that has bits at both bpno=5 and bpno=3
        let val = (1i32 << (higher_bpno + T1_NMSEDEC_FRACBITS as i32))
            | (1i32 << (refine_bpno + T1_NMSEDEC_FRACBITS as i32));
        enc.data[4] = val; // positive, SMR = two's complement for positive

        // Mark (col=1, row=0) as already significant (simulating prior clnpass at higher bpno)
        let fp = enc.flags_index(1, 0);
        let stride = enc.flags_stride();
        update_flags(&mut enc.flags, fp, 0, 0, stride, false); // sign=0 (positive)

        let mut enc_buf = vec![0u8; 256];
        let num_bytes;
        {
            let mut mqc = Mqc::new(&mut enc_buf);
            init_t1_mqc_contexts(&mut mqc);
            mqc.init_enc();
            let nmsedec = enc.enc_refpass(&mut mqc, refine_bpno, T1_TYPE_MQ);
            mqc.flush();
            assert!(nmsedec >= 0);
            num_bytes = mqc.num_bytes();
            assert!(num_bytes > 0);
        }

        // Verify MU flag was set on encoder
        assert_ne!(
            enc.flags[fp] & T1_MU_THIS,
            0,
            "MU_THIS should be set after enc_refpass"
        );

        // --- Decode ---
        let mut dec_buf = vec![0u8; 256];
        dec_buf[..num_bytes].copy_from_slice(&enc_buf[1..1 + num_bytes]);

        let mut dec = T1::new(false);
        dec.allocate_buffers(4, 4).unwrap();
        dec.set_orient(0);

        // Decoder row-major: (col=1, row=0) = data[0*4 + 1] = data[1]
        // Set initial decoded value as oneplushalf from the higher bitplane.
        // Decoder uses 2x scale: one = 1 << (bpno+1).
        let one_high = 1i32 << (higher_bpno + 1);
        let half_high = one_high >> 1;
        let oneplushalf_high = one_high | half_high;
        dec.data[1] = oneplushalf_high; // positive

        // Same prior flag state
        let fp_dec = dec.flags_index(1, 0);
        let stride_dec = dec.flags_stride();
        update_flags(&mut dec.flags, fp_dec, 0, 0, stride_dec, false);

        {
            let mut mqc = Mqc::new(&mut dec_buf);
            init_t1_mqc_contexts(&mut mqc);
            mqc.init_dec(num_bytes);
            dec.dec_refpass_mqc(&mut mqc, refine_bpno + 1);
            mqc.finish_dec();
        }

        // Refinement should add poshalf (since bit=1 and data>=0, v=1 ^ 0 = 1 → +poshalf)
        // At 2x scale: poshalf = 1 << refine_bpno (was 1 << (refine_bpno - 1))
        let poshalf = 1i32 << refine_bpno;
        let expected = oneplushalf_high + poshalf;
        assert_eq!(
            dec.data[1], expected,
            "refinement bit should adjust decoded value"
        );
        // MU flag should be set
        assert_ne!(
            dec.flags[fp_dec] & T1_MU_THIS,
            0,
            "MU_THIS should be set after dec_refpass"
        );
    }

    // --- Clean-up pass tests ---

    #[test]
    fn clnpass_encode_decode_roundtrip() {
        use crate::coding::mqc::Mqc;

        // Setup: 4x4 block with some non-zero coefficients at bpno=3.
        // Clean-up pass is always the first pass for a fresh (all-zero flags) block,
        // so no prior significance/PI state needed.
        let bpno: i32 = 3;
        let one = 1i32 << (bpno + T1_NMSEDEC_FRACBITS as i32);

        // --- Encode ---
        let mut enc = T1::new(true);
        enc.allocate_buffers(4, 4).unwrap();
        enc.set_orient(0);

        // Encoder zigzag layout: col c = data[c*4..c*4+4]
        // Set (col=0, row=0) = data[0] to positive value with bit at bpno set
        enc.data[0] = one;
        // Set (col=2, row=1) = data[2*4 + 1] = data[9] to negative value
        enc.data[9] = (one as u32 | 0x8000_0000) as i32; // SMR negative

        let mut enc_buf = vec![0u8; 256];
        let num_bytes;
        {
            let mut mqc = Mqc::new(&mut enc_buf);
            init_t1_mqc_contexts(&mut mqc);
            mqc.init_enc();
            let nmsedec = enc.enc_clnpass(&mut mqc, bpno, 0);
            mqc.flush();
            assert!(nmsedec >= 0);
            num_bytes = mqc.num_bytes();
            assert!(num_bytes > 0);
        }

        // --- Decode ---
        let mut dec_buf = vec![0u8; 256];
        dec_buf[..num_bytes].copy_from_slice(&enc_buf[1..1 + num_bytes]);

        let mut dec = T1::new(false);
        dec.allocate_buffers(4, 4).unwrap();
        dec.set_orient(0);

        {
            let mut mqc = Mqc::new(&mut dec_buf);
            init_t1_mqc_contexts(&mut mqc);
            mqc.init_dec(num_bytes);
            dec.dec_clnpass(&mut mqc, bpno + 1, 0);
            mqc.finish_dec();
        }

        // Verify: decoder row-major layout
        // Decoder uses 2x scale: oneplushalf = (1 << (bpno+1)) | (1 << bpno)
        let oneplushalf = (1i32 << (bpno + 1)) | (1i32 << bpno);

        // (col=0, row=0) in row-major = data[0*4 + 0] = data[0]
        assert_eq!(
            dec.data[0], oneplushalf,
            "(0,0) should be positive oneplushalf"
        );

        // (col=2, row=1) in row-major = data[1*4 + 2] = data[6]
        assert_eq!(
            dec.data[6], -oneplushalf,
            "(2,1) should be negative oneplushalf"
        );

        // Other coefficients should remain 0
        assert_eq!(dec.data[1], 0, "(1,0) should be 0");
        assert_eq!(dec.data[5], 0, "(1,1) should be 0");
    }

    // --- t1_getwmsedec tests ---

    #[test]
    fn getwmsedec_qmfbid1_no_mct() {
        // reversible 5-3, no MCT norms, orient=0, level=0, bpno=3
        let w = t1_getwmsedec(1000, 0, 0, 0, 3, 1, 1.0, None);
        // w1=1.0, w2=dwt_getnorm(0,0)=1.0, stepsize=1.0
        // wmsedec = 1.0 * 1.0 * 1.0 * 8.0 = 8.0
        // result = 8.0 * 8.0 * 1000.0 / 8192.0 = 7.8125
        let expected = 1.0 * 1.0 * 1.0 * 8.0;
        let expected = expected * expected * 1000.0 / 8192.0;
        assert!((w - expected).abs() < 1e-10);
    }

    #[test]
    fn getwmsedec_qmfbid0_orient3() {
        // irreversible 9-7, orient=3, log2_gain=2
        let w = t1_getwmsedec(500, 0, 1, 3, 2, 0, 4.0, None);
        // w1=1.0, w2=dwt_getnorm_real(1,3)
        // stepsize = 4.0 / (1<<2) = 1.0
        // wmsedec = 1.0 * w2 * 1.0 * 4.0
        use crate::transform::dwt::dwt_getnorm_real;
        let w2 = dwt_getnorm_real(1, 3);
        let base = 1.0 * w2 * 1.0 * 4.0;
        let expected = base * base * 500.0 / 8192.0;
        assert!((w - expected).abs() < 1e-10);
    }

    #[test]
    fn getwmsedec_with_mct_norms() {
        let norms = [0.5, 0.3, 0.7];
        let w = t1_getwmsedec(100, 1, 0, 0, 1, 1, 1.0, Some(&norms));
        use crate::transform::dwt::dwt_getnorm;
        let w1 = 0.3;
        let w2 = dwt_getnorm(0, 0);
        let base = w1 * w2 * 1.0 * 2.0;
        let expected = base * base * 100.0 / 8192.0;
        assert!((w - expected).abs() < 1e-10);
    }

    // --- is_term_pass tests ---

    #[test]
    fn is_term_pass_last_cleanup() {
        assert!(T1::is_term_pass(5, 0, 0, 2));
    }

    #[test]
    fn is_term_pass_termall() {
        assert!(T1::is_term_pass(5, J2K_CCP_CBLKSTY_TERMALL, 3, 0));
        assert!(T1::is_term_pass(5, J2K_CCP_CBLKSTY_TERMALL, 3, 1));
        assert!(T1::is_term_pass(5, J2K_CCP_CBLKSTY_TERMALL, 3, 2));
    }

    #[test]
    fn is_term_pass_not_terminated_normal() {
        assert!(!T1::is_term_pass(5, 0, 3, 0));
        assert!(!T1::is_term_pass(5, 0, 3, 1));
        assert!(!T1::is_term_pass(5, 0, 3, 2));
    }

    #[test]
    fn is_term_pass_lazy() {
        // numbps=8, lazy: 4th cleanup pass is at bpno = numbps-4 = 4
        assert!(T1::is_term_pass(8, J2K_CCP_CBLKSTY_LAZY, 4, 2));
        // Beyond (bpno < 4): refpass and clnpass terminated
        assert!(T1::is_term_pass(8, J2K_CCP_CBLKSTY_LAZY, 3, 1));
        assert!(T1::is_term_pass(8, J2K_CCP_CBLKSTY_LAZY, 3, 2));
        // sigpass not terminated even below threshold
        assert!(!T1::is_term_pass(8, J2K_CCP_CBLKSTY_LAZY, 3, 0));
    }

    // --- Multi-pass roundtrip test ---

    #[test]
    fn multi_pass_roundtrip() {
        // 1. Create a 4x4 block with known coefficients
        let original: [i32; 16] = [
            100, -50, 25, 0, -30, 60, -10, 5, 15, -20, 40, -35, 0, 70, -80, 45,
        ];

        let w = 4u32;
        let h = 4u32;

        // 2. Convert to zigzag SMR format shifted by FRACBITS.
        //    Zigzag layout: col c stores data[c*4..c*4+4], where sub-indices
        //    0..3 correspond to rows 0..3.
        //    So zigzag[c * h + r] = original[r * w + c].
        let mut zigzag_data = vec![0i32; (w * h) as usize];
        for r in 0..h as usize {
            for c in 0..w as usize {
                let val = original[r * w as usize + c];
                let shifted = val << T1_NMSEDEC_FRACBITS;
                zigzag_data[c * h as usize + r] = shifted; // two's complement
            }
        }

        // 3. Encode
        let mut enc = T1::new(true);
        enc.allocate_buffers(w, h).unwrap();
        enc.data[..zigzag_data.len()].copy_from_slice(&zigzag_data);

        let mut enc_buf = vec![0u8; 4096];
        let (passes, cumwmsedec) = enc.encode_cblk(
            &mut enc_buf,
            0,    // orient (LL)
            0,    // compno
            0,    // level
            1,    // qmfbid (5-3 reversible)
            1.0,  // stepsize
            0,    // cblksty (no special flags)
            1,    // numcomps
            None, // mct_norms
        );

        assert!(!passes.is_empty(), "should produce at least one pass");
        assert!(
            cumwmsedec >= 0.0,
            "cumulative wmsedec should be non-negative"
        );

        // 4. Compute total encoded length and total passes
        let total_rate = passes.last().unwrap().rate as usize;
        let total_passes = passes.len() as u32;

        // Extract encoded data: encoder writes from buf[1..1+total_rate]
        let encoded_data = enc_buf[1..1 + total_rate].to_vec();

        // 5. Decode: all passes in a single segment
        let mut dec = T1::new(false);
        dec.allocate_buffers(w, h).unwrap();

        // The decoder needs the encoded data + COMMON_CBLK_DATA_EXTRA sentinel bytes
        let mut dec_buf = vec![0u8; encoded_data.len() + COMMON_CBLK_DATA_EXTRA];
        dec_buf[..encoded_data.len()].copy_from_slice(&encoded_data);

        let segments = [DecodeSegment {
            data: &dec_buf[..encoded_data.len()],
            num_passes: total_passes,
        }];

        // Compute numbps the same way encode_cblk does
        let max_abs = original.iter().map(|&v| v.abs()).max().unwrap();
        let max_shifted = max_abs << T1_NMSEDEC_FRACBITS;
        let numbps = if max_shifted != 0 {
            (int_floorlog2(max_shifted) + 1 - T1_NMSEDEC_FRACBITS as i32) as u32
        } else {
            0
        };

        dec.decode_cblk(&segments, 0, 0, numbps, 0).unwrap();

        // 6. Convert decoded row-major data back.
        //    Decoder data is in row-major: data[r * w + c].
        //    The T1 decoder now reconstructs at 2x scale (one = 1 << bpno_plus_one),
        //    so raw decoded values are 2x the original coefficients.
        //    copy_decoded_cblks_to_data divides by 2 for reversible mode to recover
        //    original scale, but decode_cblk here returns raw 2x values.
        // 7. Verify decoded values are close to 2x original (within ±4 tolerance,
        //    accounting for encode-side SMR rounding and decode-side half-bit midpoint
        //    at 2x scale).
        let tolerance = 4i32;
        for r in 0..h as usize {
            for c in 0..w as usize {
                let decoded = dec.data[r * w as usize + c];
                let expected = original[r * w as usize + c] * 2;
                let diff = (decoded - expected).abs();
                assert!(
                    diff <= tolerance,
                    "coefficient ({},{}) mismatch: decoded={}, expected={} (2x original {}), diff={}, tolerance={}",
                    c,
                    r,
                    decoded,
                    expected,
                    original[r * w as usize + c],
                    diff,
                    tolerance,
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Tests: HT decode dispatch
    // -----------------------------------------------------------------------

    #[test]
    fn t1_decode_cblk_ht_allzero() {
        // HT decode of an all-zero codeblock should overwrite data with zeros.
        let mut t1 = T1::new(false);
        t1.allocate_buffers(2, 2).unwrap();
        // Pre-fill with sentinels to verify decode actually writes
        t1.data.fill(0x12345678);

        let data: Vec<u8> = vec![0xFF, 0x00, 0x00, 0x00, 0x02, 0x00];
        let segments = [DecodeSegment {
            data: &data,
            num_passes: 1,
        }];

        t1.decode_cblk_ht(&segments, 8, 0, 7).unwrap();
        // Sentinels should be overwritten with zeros
        assert!(t1.data.iter().all(|&v| v == 0));
    }

    #[test]
    fn t1_decode_cblk_ht_copies_to_data() {
        // Verify HT decode results are stored in T1's data buffer.
        let mut t1 = T1::new(false);
        t1.allocate_buffers(4, 4).unwrap();
        // Pre-fill with sentinels
        t1.data.fill(0xDEADBEEFu32 as i32);

        let data: Vec<u8> = vec![0xFF, 0x00, 0x00, 0x00, 0x02, 0x00];
        let segments = [DecodeSegment {
            data: &data,
            num_passes: 1,
        }];

        t1.decode_cblk_ht(&segments, 8, 0, 7).unwrap();
        assert_eq!(t1.data.len(), 16); // 4x4
        // Sentinels should be overwritten
        assert!(t1.data.iter().all(|&v| v != 0xDEADBEEFu32 as i32));
    }

    // -----------------------------------------------------------------------
    // Tests: Batch codeblock decode/encode (Phase 800a)
    // -----------------------------------------------------------------------

    use crate::j2k::params::TileCompCodingParameters;
    use crate::tcd::{
        TcdBand, TcdCblkDec, TcdCblkEnc, TcdCodeBlocks, TcdPrecinct, TcdResolution, TcdTile,
        TcdTileComp,
    };

    /// Build a minimal tile with one decode codeblock (no segment data).
    fn build_tile_one_dec_cblk(w: i32, h: i32) -> TcdTile {
        TcdTile {
            x0: 0,
            y0: 0,
            x1: w,
            y1: h,
            comps: vec![TcdTileComp {
                x0: 0,
                y0: 0,
                x1: w,
                y1: h,
                compno: 0,
                numresolutions: 1,
                minimum_num_resolutions: 1,
                resolutions: vec![TcdResolution {
                    x0: 0,
                    y0: 0,
                    x1: w,
                    y1: h,
                    pw: 1,
                    ph: 1,
                    numbands: 1,
                    bands: vec![TcdBand {
                        x0: 0,
                        y0: 0,
                        x1: w,
                        y1: h,
                        bandno: 0,
                        numbps: 8,
                        stepsize: 1.0,
                        precincts: vec![TcdPrecinct {
                            x0: 0,
                            y0: 0,
                            x1: w,
                            y1: h,
                            cw: 1,
                            ch: 1,
                            cblks: TcdCodeBlocks::Dec(vec![TcdCblkDec {
                                x0: 0,
                                y0: 0,
                                x1: w,
                                y1: h,
                                numbps: 8,
                                ..Default::default()
                            }]),
                            ..Default::default()
                        }],
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    /// Build a minimal tile with one encode codeblock and test data.
    fn build_tile_one_enc_cblk(w: i32, h: i32, fill: i32) -> TcdTile {
        let n = (w * h) as usize;
        TcdTile {
            x0: 0,
            y0: 0,
            x1: w,
            y1: h,
            comps: vec![TcdTileComp {
                x0: 0,
                y0: 0,
                x1: w,
                y1: h,
                compno: 0,
                numresolutions: 1,
                minimum_num_resolutions: 1,
                resolutions: vec![TcdResolution {
                    x0: 0,
                    y0: 0,
                    x1: w,
                    y1: h,
                    pw: 1,
                    ph: 1,
                    numbands: 1,
                    bands: vec![TcdBand {
                        x0: 0,
                        y0: 0,
                        x1: w,
                        y1: h,
                        bandno: 0,
                        numbps: 8,
                        stepsize: 1.0,
                        precincts: vec![TcdPrecinct {
                            x0: 0,
                            y0: 0,
                            x1: w,
                            y1: h,
                            cw: 1,
                            ch: 1,
                            cblks: TcdCodeBlocks::Enc(vec![TcdCblkEnc {
                                x0: 0,
                                y0: 0,
                                x1: w,
                                y1: h,
                                ..Default::default()
                            }]),
                            ..Default::default()
                        }],
                    }],
                    ..Default::default()
                }],
                data: vec![fill; n],
                numpix: n,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn simple_tcp() -> TileCodingParameters {
        TileCodingParameters {
            tccps: vec![TileCompCodingParameters {
                cblksty: 0,
                roishift: 0,
                qmfbid: 1,
                numresolutions: 1,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn t1_decode_cblks_empty_segments() {
        // Codeblock with no segment data → decoded_data should be all zeros.
        let mut tile = build_tile_one_dec_cblk(4, 4);
        let tcp = simple_tcp();

        t1_decode_cblks(&mut tile, &tcp).unwrap();

        if let TcdCodeBlocks::Dec(cblks) = &tile.comps[0].resolutions[0].bands[0].precincts[0].cblks
        {
            assert!(cblks[0].decoded_data.is_some());
            let data = cblks[0].decoded_data.as_ref().unwrap();
            assert_eq!(data.len(), 16); // 4x4
            assert!(data.iter().all(|&v| v == 0));
        } else {
            panic!("expected Dec codeblocks");
        }
    }

    #[test]
    fn t1_decode_cblks_enc_blocks_skipped() {
        // Enc codeblocks should be skipped (no crash).
        let mut tile = build_tile_one_enc_cblk(4, 4, 100);
        let tcp = simple_tcp();

        t1_decode_cblks(&mut tile, &tcp).unwrap();

        // Enc blocks should remain unchanged
        if let TcdCodeBlocks::Enc(cblks) = &tile.comps[0].resolutions[0].bands[0].precincts[0].cblks
        {
            assert!(cblks[0].data.is_empty());
        } else {
            panic!("expected Enc codeblocks");
        }
    }

    #[test]
    fn t1_encode_cblks_produces_data() {
        // Encode with non-zero coefficient data → should produce passes + data.
        let mut tile = build_tile_one_enc_cblk(4, 4, 100);
        let tcp = simple_tcp();

        t1_encode_cblks(&mut tile, &tcp, None, 1).unwrap();

        if let TcdCodeBlocks::Enc(cblks) = &tile.comps[0].resolutions[0].bands[0].precincts[0].cblks
        {
            assert!(
                !cblks[0].data.is_empty(),
                "encoded data should not be empty"
            );
            assert!(!cblks[0].passes.is_empty(), "passes should not be empty");
            assert!(cblks[0].numbps > 0);
            assert_eq!(cblks[0].totalpasses, cblks[0].passes.len() as u32);
        } else {
            panic!("expected Enc codeblocks");
        }
    }

    #[test]
    fn t1_encode_cblks_zero_data_no_passes() {
        // All-zero data → no passes needed.
        let mut tile = build_tile_one_enc_cblk(4, 4, 0);
        let tcp = simple_tcp();

        t1_encode_cblks(&mut tile, &tcp, None, 1).unwrap();

        if let TcdCodeBlocks::Enc(cblks) = &tile.comps[0].resolutions[0].bands[0].precincts[0].cblks
        {
            assert!(cblks[0].passes.is_empty());
        } else {
            panic!("expected Enc codeblocks");
        }
    }

    #[test]
    fn t1_batch_encode_decode_roundtrip() {
        // Encode then decode via batch functions, verify roundtrip.
        let original_data: Vec<i32> = (0..16).map(|i| (i * 10) - 70).collect();
        let mut enc_tile = build_tile_one_enc_cblk(4, 4, 0);
        enc_tile.comps[0].data = original_data.clone();
        let tcp = simple_tcp();

        // Encode
        t1_encode_cblks(&mut enc_tile, &tcp, None, 1).unwrap();

        let (enc_data, passes, numbps) = if let TcdCodeBlocks::Enc(cblks) =
            &enc_tile.comps[0].resolutions[0].bands[0].precincts[0].cblks
        {
            (
                cblks[0].data.clone(),
                cblks[0].passes.clone(),
                cblks[0].numbps,
            )
        } else {
            panic!("expected Enc codeblocks");
        };

        // Build a decode tile from the encoded data
        let mut dec_tile = build_tile_one_dec_cblk(4, 4);
        if let TcdCodeBlocks::Dec(cblks) =
            &mut dec_tile.comps[0].resolutions[0].bands[0].precincts[0].cblks
        {
            let total_passes = passes.len() as u32;
            cblks[0].numbps = numbps;
            cblks[0].segs.push(crate::tcd::TcdSeg {
                len: enc_data.len() as u32,
                numpasses: total_passes,
                real_num_passes: total_passes,
                maxpasses: total_passes,
                ..Default::default()
            });
            cblks[0].chunks.push(crate::tcd::TcdSegDataChunk {
                data: enc_data.clone(),
                len: enc_data.len() as u32,
            });
        }

        // Decode
        t1_decode_cblks(&mut dec_tile, &tcp).unwrap();

        if let TcdCodeBlocks::Dec(cblks) =
            &dec_tile.comps[0].resolutions[0].bands[0].precincts[0].cblks
        {
            let decoded = cblks[0].decoded_data.as_ref().unwrap();
            assert_eq!(decoded.len(), 16);
            // T1 decoder now produces 2x-scale raw values. Verify roundtrip
            // within tolerance (±4 for rounding at 2x scale).
            for (i, (&dec, &orig)) in decoded.iter().zip(original_data.iter()).enumerate() {
                let expected_2x = orig * 2;
                assert!(
                    (dec - expected_2x).abs() <= 4,
                    "sample {i}: decoded={dec}, expected_2x={expected_2x} (original={orig})"
                );
            }
        } else {
            panic!("expected Dec codeblocks");
        }
    }
}
