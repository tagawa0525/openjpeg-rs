// Phase 400a: J2K coding parameter structures
//
// Minimal subset of J2K parameters needed by PI, T2, and TCD.
// Marker read/write logic will be added in Phase 500.

use crate::types::{
    COMP_PARAM_DEFAULT_NUMRESOLUTION, J2K_MAX_POCS, J2K_MAXBANDS, J2K_MAXRLVLS, ProgressionOrder,
};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// T2 processing mode (C: J2K_T2_MODE).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum T2Mode {
    /// Rate allocation threshold calculation.
    ThreshCalc = 0,
    /// Final encoding pass.
    FinalPass = 1,
}

/// Quality layer allocation strategy (C: J2K_QUALITY_LAYER_ALLOCATION_STRATEGY).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum QualityLayerAllocStrategy {
    /// Allocation by rate/distortion ratio.
    #[default]
    RateDistortionRatio = 0,
    /// Allocation by fixed distortion ratio (PSNR).
    FixedDistortionRatio = 1,
    /// Allocation by fixed layer.
    FixedLayer = 2,
}

// ---------------------------------------------------------------------------
// Structures
// ---------------------------------------------------------------------------

/// Quantization step size (C: opj_stepsize_t).
#[derive(Debug, Default, Clone, Copy)]
pub struct Stepsize {
    pub expn: i32,
    pub mant: i32,
}

/// Tile component coding parameters (C: opj_tccp_t).
///
/// Minimal subset for Phase 400. Additional fields (MCT data, etc.)
/// will be added in Phase 500.
#[derive(Debug, Clone)]
pub struct TileCompCodingParameters {
    /// Coding style (C: csty).
    pub csty: u32,
    /// Number of resolution levels (C: numresolutions).
    pub numresolutions: u32,
    /// Code-block width exponent (C: cblkw). Actual width = 1 << cblkw.
    pub cblkw: u32,
    /// Code-block height exponent (C: cblkh). Actual height = 1 << cblkh.
    pub cblkh: u32,
    /// Code-block coding style (C: cblksty).
    pub cblksty: u32,
    /// Wavelet filter: 0 = 9-7 irreversible, 1 = 5-3 reversible (C: qmfbid).
    pub qmfbid: u32,
    /// Quantization style (C: qntsty).
    pub qntsty: u32,
    /// Quantization step sizes (C: stepsizes).
    pub stepsizes: [Stepsize; J2K_MAXBANDS],
    /// Number of guard bits (C: numgbits).
    pub numgbits: u32,
    /// ROI shift (C: roishift).
    pub roishift: i32,
    /// Precinct width exponents per resolution (C: prcw).
    pub prcw: [u32; J2K_MAXRLVLS],
    /// Precinct height exponents per resolution (C: prch).
    pub prch: [u32; J2K_MAXRLVLS],
    /// DC level shift (C: m_dc_level_shift).
    pub m_dc_level_shift: i32,
}

impl Default for TileCompCodingParameters {
    fn default() -> Self {
        Self {
            csty: 0,
            numresolutions: COMP_PARAM_DEFAULT_NUMRESOLUTION,
            cblkw: 6, // log2(64) = 6
            cblkh: 6,
            cblksty: 0,
            qmfbid: 0,
            qntsty: 0,
            stepsizes: [Stepsize::default(); J2K_MAXBANDS],
            numgbits: 2,
            roishift: 0,
            prcw: [15; J2K_MAXRLVLS], // 2^15 = 32768 (max precinct)
            prch: [15; J2K_MAXRLVLS],
            m_dc_level_shift: 0,
        }
    }
}

/// Progression order change (C: opj_poc_t).
#[derive(Debug, Clone, Copy)]
pub struct Poc {
    pub resno0: u32,
    pub compno0: u32,
    pub layno1: u32,
    pub resno1: u32,
    pub compno1: u32,
    pub layno0: u32,
    pub precno0: u32,
    pub precno1: u32,
    pub prg1: ProgressionOrder,
    pub prg: ProgressionOrder,
    pub tile: u32,
    /// Tile origin X (C: tx0 — semantically u32 but stored as i32).
    pub tx0: i32,
    pub tx1: i32,
    pub ty0: i32,
    pub ty1: i32,
    // Encoder-side init fields
    pub lay_s: u32,
    pub res_s: u32,
    pub comp_s: u32,
    pub prc_s: u32,
    pub lay_e: u32,
    pub res_e: u32,
    pub comp_e: u32,
    pub prc_e: u32,
    pub tx_s: u32,
    pub tx_e: u32,
    pub ty_s: u32,
    pub ty_e: u32,
    pub dx: u32,
    pub dy: u32,
    // Tilepart temporaries
    pub lay_t: u32,
    pub res_t: u32,
    pub comp_t: u32,
    pub prc_t: u32,
    pub tx0_t: u32,
    pub ty0_t: u32,
}

impl Default for Poc {
    fn default() -> Self {
        Self {
            resno0: 0,
            compno0: 0,
            layno1: 0,
            resno1: 0,
            compno1: 0,
            layno0: 0,
            precno0: 0,
            precno1: 0,
            prg1: ProgressionOrder::Lrcp,
            prg: ProgressionOrder::Lrcp,
            tile: 0,
            tx0: 0,
            tx1: 0,
            ty0: 0,
            ty1: 0,
            lay_s: 0,
            res_s: 0,
            comp_s: 0,
            prc_s: 0,
            lay_e: 0,
            res_e: 0,
            comp_e: 0,
            prc_e: 0,
            tx_s: 0,
            tx_e: 0,
            ty_s: 0,
            ty_e: 0,
            dx: 0,
            dy: 0,
            lay_t: 0,
            res_t: 0,
            comp_t: 0,
            prc_t: 0,
            tx0_t: 0,
            ty0_t: 0,
        }
    }
}

/// Tile coding parameters (C: opj_tcp_t) — minimal subset for Phase 400.
#[derive(Debug, Clone)]
pub struct TileCodingParameters {
    /// Coding style (C: csty).
    pub csty: u32,
    /// Progression order (C: prg).
    pub prg: ProgressionOrder,
    /// Number of layers (C: numlayers).
    pub numlayers: u32,
    /// Number of layers to decode (C: num_layers_to_decode).
    pub num_layers_to_decode: u32,
    /// Multi-component transform: 0=none, 1=ICT/RCT (C: mct).
    pub mct: u32,
    /// Target bit-rates per layer (C: rates).
    pub rates: [f32; 100],
    /// Number of POCs (C: numpocs).
    pub numpocs: u32,
    /// POC entries (C: pocs).
    pub pocs: Vec<Poc>,
    /// Target distortion ratios per layer (C: distoratio).
    pub distoratio: [f32; 100],
    /// Per-component coding parameters (C: tccps).
    pub tccps: Vec<TileCompCodingParameters>,
    /// Custom MCT norms (C: mct_norms).
    pub mct_norms: Option<Vec<f64>>,
    /// MCT decoding matrix (C: m_mct_decoding_matrix).
    pub m_mct_decoding_matrix: Option<Vec<f32>>,
    /// MCT coding matrix (C: m_mct_coding_matrix).
    pub m_mct_coding_matrix: Option<Vec<f32>>,
    /// COD marker found (C: cod bitfield).
    pub cod: bool,
    /// PPT marker found (C: ppt bitfield).
    pub ppt: bool,
    /// POC marker found (C: POC bitfield).
    pub poc: bool,
}

impl Default for TileCodingParameters {
    fn default() -> Self {
        Self {
            csty: 0,
            prg: ProgressionOrder::Lrcp,
            numlayers: 1,
            num_layers_to_decode: 1,
            mct: 0,
            rates: [0.0; 100],
            numpocs: 0,
            pocs: vec![Poc::default(); J2K_MAX_POCS],
            distoratio: [0.0; 100],
            tccps: Vec::new(),
            mct_norms: None,
            m_mct_decoding_matrix: None,
            m_mct_coding_matrix: None,
            cod: false,
            ppt: false,
            poc: false,
        }
    }
}

/// Encoding-specific parameters (C: opj_encoding_param_t).
#[derive(Debug, Default, Clone)]
pub struct EncodingParam {
    /// Maximum component size (C: m_max_comp_size).
    pub m_max_comp_size: u32,
    /// Tile part position (C: m_tp_pos).
    pub m_tp_pos: i32,
    /// Tile parts enabled (C: m_tp_on bitfield).
    pub m_tp_on: bool,
    /// Quality layer allocation strategy (C: m_quality_layer_alloc_strategy).
    pub m_quality_layer_alloc_strategy: QualityLayerAllocStrategy,
    /// Progression order matrix (C: m_matrice).
    pub m_matrice: Option<Vec<i32>>,
}

/// Decoding-specific parameters (C: opj_decoding_param_t).
#[derive(Debug, Default, Clone, Copy)]
pub struct DecodingParam {
    /// Resolution reduction factor (C: m_reduce).
    pub m_reduce: u32,
    /// Maximum number of layers to decode (C: m_layer).
    pub m_layer: u32,
}

/// Coding parameter mode — replaces C union of encoding/decoding params.
#[derive(Debug, Clone)]
pub enum CodingParamMode {
    Encoder(EncodingParam),
    Decoder(DecodingParam),
}

/// Coding parameters (C: opj_cp_t) — minimal subset for Phase 400.
#[derive(Debug, Clone)]
pub struct CodingParameters {
    /// Profile (C: rsiz).
    pub rsiz: u16,
    /// Tile grid origin X (C: tx0).
    pub tx0: u32,
    /// Tile grid origin Y (C: ty0).
    pub ty0: u32,
    /// Tile width (C: tdx).
    pub tdx: u32,
    /// Tile height (C: tdy).
    pub tdy: u32,
    /// Number of tiles horizontally (C: tw).
    pub tw: u32,
    /// Number of tiles vertically (C: th).
    pub th: u32,
    /// Per-tile coding parameters (C: tcps).
    pub tcps: Vec<TileCodingParameters>,
    /// Encoder or decoder specific parameters.
    pub mode: CodingParamMode,
    /// Strict mode (C: strict).
    pub strict: bool,
    /// PPM marker present (C: ppm bitfield).
    pub ppm: bool,
}

impl CodingParameters {
    /// Create encoder-mode coding parameters.
    pub fn new_encoder() -> Self {
        Self {
            rsiz: 0,
            tx0: 0,
            ty0: 0,
            tdx: 0,
            tdy: 0,
            tw: 0,
            th: 0,
            tcps: Vec::new(),
            mode: CodingParamMode::Encoder(EncodingParam::default()),
            strict: true,
            ppm: false,
        }
    }

    /// Create decoder-mode coding parameters.
    pub fn new_decoder() -> Self {
        Self {
            rsiz: 0,
            tx0: 0,
            ty0: 0,
            tdx: 0,
            tdy: 0,
            tw: 0,
            th: 0,
            tcps: Vec::new(),
            mode: CodingParamMode::Decoder(DecodingParam::default()),
            strict: true,
            ppm: false,
        }
    }

    /// Returns `true` if these are decoder parameters.
    pub fn is_decoder(&self) -> bool {
        matches!(self.mode, CodingParamMode::Decoder(_))
    }

    /// Returns encoding parameters, if in encoder mode.
    pub fn encoding_param(&self) -> Option<&EncodingParam> {
        match &self.mode {
            CodingParamMode::Encoder(p) => Some(p),
            _ => None,
        }
    }

    /// Returns decoding parameters, if in decoder mode.
    pub fn decoding_param(&self) -> Option<&DecodingParam> {
        match &self.mode {
            CodingParamMode::Decoder(p) => Some(p),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t2_mode_values() {
        assert_eq!(T2Mode::ThreshCalc as i32, 0);
        assert_eq!(T2Mode::FinalPass as i32, 1);
        assert_ne!(T2Mode::ThreshCalc, T2Mode::FinalPass);
    }

    #[test]
    fn quality_layer_alloc_strategy_values() {
        assert_eq!(QualityLayerAllocStrategy::RateDistortionRatio as i32, 0);
        assert_eq!(QualityLayerAllocStrategy::FixedDistortionRatio as i32, 1);
        assert_eq!(QualityLayerAllocStrategy::FixedLayer as i32, 2);
        assert_eq!(
            QualityLayerAllocStrategy::default(),
            QualityLayerAllocStrategy::RateDistortionRatio
        );
    }

    #[test]
    fn stepsize_default() {
        let s = Stepsize::default();
        assert_eq!(s.expn, 0);
        assert_eq!(s.mant, 0);
    }

    #[test]
    fn tccp_default() {
        let tccp = TileCompCodingParameters::default();
        assert_eq!(tccp.numresolutions, COMP_PARAM_DEFAULT_NUMRESOLUTION);
        assert_eq!(tccp.cblkw, 6);
        assert_eq!(tccp.cblkh, 6);
        assert_eq!(tccp.numgbits, 2);
        assert_eq!(tccp.prcw[0], 15);
        assert_eq!(tccp.prch[0], 15);
        assert_eq!(tccp.m_dc_level_shift, 0);
    }

    #[test]
    fn poc_default() {
        let poc = Poc::default();
        assert_eq!(poc.resno0, 0);
        assert_eq!(poc.prg, ProgressionOrder::Lrcp);
        assert_eq!(poc.tx0, 0);
        assert_eq!(poc.dx, 0);
    }

    #[test]
    fn tcp_default() {
        let tcp = TileCodingParameters::default();
        assert_eq!(tcp.numlayers, 1);
        assert_eq!(tcp.prg, ProgressionOrder::Lrcp);
        assert_eq!(tcp.pocs.len(), J2K_MAX_POCS);
        assert!(!tcp.cod);
        assert!(!tcp.poc);
    }

    #[test]
    fn encoding_param_default() {
        let ep = EncodingParam::default();
        assert_eq!(ep.m_max_comp_size, 0);
        assert!(!ep.m_tp_on);
        assert_eq!(
            ep.m_quality_layer_alloc_strategy,
            QualityLayerAllocStrategy::RateDistortionRatio
        );
    }

    #[test]
    fn decoding_param_default() {
        let dp = DecodingParam::default();
        assert_eq!(dp.m_reduce, 0);
        assert_eq!(dp.m_layer, 0);
    }

    #[test]
    fn coding_parameters_encoder() {
        let cp = CodingParameters::new_encoder();
        assert!(!cp.is_decoder());
        assert!(cp.encoding_param().is_some());
        assert!(cp.decoding_param().is_none());
        assert!(cp.strict);
    }

    #[test]
    fn coding_parameters_decoder() {
        let cp = CodingParameters::new_decoder();
        assert!(cp.is_decoder());
        assert!(cp.encoding_param().is_none());
        assert!(cp.decoding_param().is_some());
    }

    #[test]
    fn coding_param_mode_variants() {
        let enc = CodingParamMode::Encoder(EncodingParam::default());
        assert!(matches!(enc, CodingParamMode::Encoder(_)));

        let dec = CodingParamMode::Decoder(DecodingParam::default());
        assert!(matches!(dec, CodingParamMode::Decoder(_)));
    }
}
