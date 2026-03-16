// Phase 400a+d: TCD (Tile Coder/Decoder)
//
// Defines the TCD hierarchy: Tile → TileComp → Resolution → Band → Precinct → Codeblock.
// Provides pipeline functions for init_tile, DC level shift, MCT, DWT wrappers.

use crate::coding::t1::TcdPass;
use crate::coding::tgt::TagTree;
use crate::error::{Error, Result};
use crate::image::Image;
use crate::j2k::params::{CodingParameters, TileCodingParameters};
use crate::types::{
    J2K_MAXLAYERS, int_ceildiv, int_ceildivpow2, int_floordivpow2, uint_ceildiv, uint_ceildivpow2,
};

// ---------------------------------------------------------------------------
// Layer / Codeblock structures
// ---------------------------------------------------------------------------

/// Layer information for a code block (C: opj_tcd_layer_t).
#[derive(Debug, Default, Clone)]
pub struct TcdLayer {
    /// Number of passes in this layer.
    pub numpasses: u32,
    /// Length of coded data in this layer.
    pub len: u32,
    /// Distortion decrease contributed by this layer.
    pub disto: f64,
    /// Offset into the code block's data buffer.
    pub data_offset: u32,
}

/// Encoding code block (C: opj_tcd_cblk_enc_t).
#[derive(Debug, Default, Clone)]
pub struct TcdCblkEnc {
    /// Compressed data buffer.
    pub data: Vec<u8>,
    /// Per-layer information.
    pub layers: Vec<TcdLayer>,
    /// Per-pass information.
    pub passes: Vec<TcdPass>,
    /// Bounding box.
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
    /// Number of significant bit-planes.
    pub numbps: u32,
    /// Number of length bits.
    pub numlenbits: u32,
    /// Number of passes already done.
    pub numpasses: u32,
    /// Number of passes included in layers so far.
    pub numpassesinlayers: u32,
    /// Total number of passes.
    pub totalpasses: u32,
}

/// Decoding segment (C: opj_tcd_seg_t).
#[derive(Debug, Default, Clone)]
pub struct TcdSeg {
    /// Size of data in this segment.
    pub len: u32,
    /// Number of passes in this segment.
    pub numpasses: u32,
    /// Real number of passes (after correction).
    pub real_num_passes: u32,
    /// Maximum passes this segment can hold.
    pub maxpasses: u32,
    /// Number of new passes to add.
    pub numnewpasses: u32,
    /// New length to add.
    pub newlen: u32,
}

/// Segment data chunk (C: opj_tcd_seg_data_chunk_t).
#[derive(Debug, Clone)]
pub struct TcdSegDataChunk {
    /// Segment data (owned copy).
    pub data: Vec<u8>,
    /// Usable length.
    pub len: u32,
}

/// Decoding code block (C: opj_tcd_cblk_dec_t).
#[derive(Debug, Default, Clone)]
pub struct TcdCblkDec {
    /// Segment information.
    pub segs: Vec<TcdSeg>,
    /// Data chunks.
    pub chunks: Vec<TcdSegDataChunk>,
    /// Bounding box.
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
    /// Maximum bit-planes (Mb) — for HT correctness checks.
    pub mb: u32,
    /// Number of significant bit-planes.
    pub numbps: u32,
    /// Number of length bits.
    pub numlenbits: u32,
    /// Number of new passes to add.
    pub numnewpasses: u32,
    /// Number of segments in use.
    pub numsegs: u32,
    /// Real number of segments.
    pub real_num_segs: u32,
    /// Decoded coefficient data (allocated on first decode).
    pub decoded_data: Option<Vec<i32>>,
    /// Whether data is corrupted.
    pub corrupted: bool,
}

/// Code blocks for a precinct — replaces C union of enc/dec.
#[derive(Debug, Default, Clone)]
pub enum TcdCodeBlocks {
    Enc(Vec<TcdCblkEnc>),
    Dec(Vec<TcdCblkDec>),
    #[default]
    Empty,
}

// ---------------------------------------------------------------------------
// Hierarchy: Precinct → Band → Resolution → TileComp → Tile
// ---------------------------------------------------------------------------

/// Precinct (C: opj_tcd_precinct_t).
#[derive(Debug, Default, Clone)]
pub struct TcdPrecinct {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
    /// Number of code blocks in width.
    pub cw: u32,
    /// Number of code blocks in height.
    pub ch: u32,
    /// Code blocks.
    pub cblks: TcdCodeBlocks,
    /// Inclusion tag tree.
    pub incltree: Option<TagTree>,
    /// IMSB tag tree.
    pub imsbtree: Option<TagTree>,
}

/// Sub-band (C: opj_tcd_band_t).
#[derive(Debug, Default, Clone)]
pub struct TcdBand {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
    /// Band number: 0=LL (lowest res), 1=HL, 2=LH, 3=HH.
    pub bandno: u32,
    /// Precincts in this band.
    pub precincts: Vec<TcdPrecinct>,
    /// Number of significant bit-planes.
    pub numbps: i32,
    /// Quantization step size.
    pub stepsize: f32,
}

impl TcdBand {
    /// Returns `true` if this band has zero area.
    pub fn is_empty(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }
}

/// Resolution level (C: opj_tcd_resolution_t).
#[derive(Debug, Default, Clone)]
pub struct TcdResolution {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
    /// Number of precincts in width.
    pub pw: u32,
    /// Number of precincts in height.
    pub ph: u32,
    /// Number of bands (1 for lowest resolution, 3 otherwise).
    pub numbands: u32,
    /// Sub-bands. Length = numbands.
    pub bands: Vec<TcdBand>,
    /// Window of interest (decode only).
    pub win_x0: u32,
    pub win_y0: u32,
    pub win_x1: u32,
    pub win_y1: u32,
}

/// Tile component (C: opj_tcd_tilecomp_t).
#[derive(Debug, Default, Clone)]
pub struct TcdTileComp {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
    /// Component number.
    pub compno: u32,
    /// Number of resolution levels.
    pub numresolutions: u32,
    /// Minimum number of resolution levels (after reduce).
    pub minimum_num_resolutions: u32,
    /// Resolution levels.
    pub resolutions: Vec<TcdResolution>,
    /// Tile component data (coefficients).
    pub data: Vec<i32>,
    /// Number of pixels.
    pub numpix: usize,
    /// Window of interest (decode only).
    pub win_x0: u32,
    pub win_y0: u32,
    pub win_x1: u32,
    pub win_y1: u32,
    /// Windowed data (decode only).
    pub data_win: Option<Vec<i32>>,
}

/// Tile (C: opj_tcd_tile_t).
#[derive(Debug, Clone)]
pub struct TcdTile {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
    /// Tile components.
    pub comps: Vec<TcdTileComp>,
    /// Total number of pixels.
    pub numpix: usize,
    /// Total distortion for the tile.
    pub distotile: f64,
    /// Distortion per layer.
    pub distolayer: [f64; J2K_MAXLAYERS],
    /// Current packet number.
    pub packno: u32,
}

impl Default for TcdTile {
    fn default() -> Self {
        Self {
            x0: 0,
            y0: 0,
            x1: 0,
            y1: 0,
            comps: Vec::new(),
            numpix: 0,
            distotile: 0.0,
            distolayer: [0.0; J2K_MAXLAYERS],
            packno: 0,
        }
    }
}

/// Tile coder/decoder (C: opj_tcd_t).
///
/// Pipeline methods (init_tile, encode_tile, decode_tile) will be
/// added in Phase 400d.
#[derive(Debug, Clone)]
pub struct Tcd {
    /// Tile-part position.
    pub tp_pos: i32,
    /// Tile-part number.
    pub tp_num: u32,
    /// Current tile-part number.
    pub cur_tp_num: u32,
    /// Total number of tile-parts for current tile.
    pub cur_totnum_tp: u32,
    /// Current POC index.
    pub cur_pino: u32,
    /// The tile being processed.
    pub tile: TcdTile,
    /// Current tile number.
    pub tcd_tileno: u32,
    /// Whether this is a decoder.
    pub is_decoder: bool,
    /// Whether decoding the whole tile (vs windowed).
    pub whole_tile_decoding: bool,
    /// Which components are used (None = all).
    pub used_component: Option<Vec<bool>>,
    /// Window of interest.
    pub win_x0: u32,
    pub win_y0: u32,
    pub win_x1: u32,
    pub win_y1: u32,
}

impl Tcd {
    /// Create a new Tcd instance.
    pub fn new(is_decoder: bool) -> Self {
        Self {
            tp_pos: 0,
            tp_num: 0,
            cur_tp_num: 0,
            cur_totnum_tp: 0,
            cur_pino: 0,
            tile: TcdTile::default(),
            tcd_tileno: 0,
            is_decoder,
            whole_tile_decoding: true,
            used_component: None,
            win_x0: 0,
            win_y0: 0,
            win_x1: 0,
            win_y1: 0,
        }
    }

    /// Initialize tile hierarchy from coding parameters (C: opj_tcd_init_tile).
    ///
    /// Builds the complete TCD hierarchy: tile → components → resolutions →
    /// bands → precincts → codeblocks, computing geometry from the image
    /// and coding parameters.
    #[allow(clippy::too_many_lines)]
    pub fn init_tile(
        &mut self,
        tileno: u32,
        image: &Image,
        cp: &CodingParameters,
        tcp: &TileCodingParameters,
        is_encoder: bool,
    ) -> Result<()> {
        if cp.tw == 0 || cp.th == 0 || tileno >= cp.tw * cp.th {
            return Err(Error::InvalidInput(format!(
                "tileno {tileno} out of range (tw={}, th={})",
                cp.tw, cp.th
            )));
        }

        let p = tileno % cp.tw;
        let q = tileno / cp.tw;

        // Tile boundaries clipped to image extent
        let tx0 = (cp.tx0 + p * cp.tdx).max(image.x0);
        let ty0 = (cp.ty0 + q * cp.tdy).max(image.y0);
        let tx1 = (cp.tx0 + (p + 1) * cp.tdx).min(image.x1);
        let ty1 = (cp.ty0 + (q + 1) * cp.tdy).min(image.y1);

        if tx1 <= tx0 || ty1 <= ty0 {
            return Err(Error::InvalidInput(format!(
                "tile {tileno} has zero or negative area: ({tx0},{ty0})-({tx1},{ty1})"
            )));
        }

        self.tile.x0 = tx0 as i32;
        self.tile.y0 = ty0 as i32;
        self.tile.x1 = tx1 as i32;
        self.tile.y1 = ty1 as i32;
        self.tcd_tileno = tileno;

        let numcomps = image.comps.len();
        self.tile.comps.resize_with(numcomps, TcdTileComp::default);

        for compno in 0..numcomps {
            let img_comp = &image.comps[compno];
            let tccp = if compno < tcp.tccps.len() {
                &tcp.tccps[compno]
            } else {
                return Err(Error::InvalidInput(format!(
                    "missing TCCP for component {compno}"
                )));
            };

            let comp = &mut self.tile.comps[compno];
            comp.compno = compno as u32;

            // Component-level tile boundaries (scaled by subsampling)
            if img_comp.dx == 0 || img_comp.dy == 0 {
                return Err(Error::InvalidInput(format!(
                    "component {compno} has zero subsampling factor"
                )));
            }
            comp.x0 = int_ceildiv(tx0 as i32, img_comp.dx as i32);
            comp.y0 = int_ceildiv(ty0 as i32, img_comp.dy as i32);
            comp.x1 = int_ceildiv(tx1 as i32, img_comp.dx as i32);
            comp.y1 = int_ceildiv(ty1 as i32, img_comp.dy as i32);

            let numresolutions = tccp.numresolutions as usize;
            comp.numresolutions = tccp.numresolutions;
            comp.minimum_num_resolutions = tccp.numresolutions;

            comp.resolutions
                .resize_with(numresolutions, TcdResolution::default);

            // Allocate tile component data
            let comp_w = (comp.x1 - comp.x0) as usize;
            let comp_h = (comp.y1 - comp.y0) as usize;
            comp.numpix = comp_w * comp_h;
            if is_encoder && comp.data.len() < comp.numpix {
                comp.data.resize(comp.numpix, 0);
            }

            // Build resolutions
            for resno in 0..numresolutions {
                let res = &mut comp.resolutions[resno];
                let levelno = (numresolutions - 1 - resno) as i32;

                // Resolution boundaries
                res.x0 = int_ceildivpow2(comp.x0, levelno);
                res.y0 = int_ceildivpow2(comp.y0, levelno);
                res.x1 = int_ceildivpow2(comp.x1, levelno);
                res.y1 = int_ceildivpow2(comp.y1, levelno);

                // Precinct dimensions
                let pdx = tccp.prcw[resno];
                let pdy = tccp.prch[resno];

                let tprc_x0 = int_floordivpow2(res.x0, pdx as i32) as u32;
                let tprc_y0 = int_floordivpow2(res.y0, pdy as i32) as u32;
                let bprc_x1 = uint_ceildivpow2(res.x1 as u32, pdx);
                let bprc_y1 = uint_ceildivpow2(res.y1 as u32, pdy);

                res.pw = if res.x1 > res.x0 {
                    bprc_x1 - tprc_x0
                } else {
                    0
                };
                res.ph = if res.y1 > res.y0 {
                    bprc_y1 - tprc_y0
                } else {
                    0
                };

                // Number of bands
                res.numbands = if resno == 0 { 1 } else { 3 };
                res.bands
                    .resize_with(res.numbands as usize, TcdBand::default);

                // Build bands
                for bandno in 0..res.numbands as usize {
                    let band = &mut res.bands[bandno];
                    band.bandno = if resno == 0 { 0 } else { (bandno + 1) as u32 };

                    // Band boundaries (subband decomposition)
                    if resno == 0 {
                        // LL band
                        band.x0 = int_ceildivpow2(comp.x0, levelno);
                        band.y0 = int_ceildivpow2(comp.y0, levelno);
                        band.x1 = int_ceildivpow2(comp.x1, levelno);
                        band.y1 = int_ceildivpow2(comp.y1, levelno);
                    } else if levelno == 0 {
                        // Highest resolution: band boundaries = resolution boundaries
                        band.x0 = res.x0;
                        band.y0 = res.y0;
                        band.x1 = res.x1;
                        band.y1 = res.y1;
                    } else {
                        // HL, LH, HH bands at lower resolution levels
                        let half_level = 1i64 << (levelno - 1);
                        let x0b = (band.bandno & 1) as i64;
                        let y0b = (band.bandno >> 1) as i64;
                        band.x0 = ((comp.x0 as i64 - half_level * x0b + (1i64 << levelno) - 1)
                            >> levelno) as i32;
                        band.y0 = ((comp.y0 as i64 - half_level * y0b + (1i64 << levelno) - 1)
                            >> levelno) as i32;
                        band.x1 = ((comp.x1 as i64 - half_level * x0b + (1i64 << levelno) - 1)
                            >> levelno) as i32;
                        band.y1 = ((comp.y1 as i64 - half_level * y0b + (1i64 << levelno) - 1)
                            >> levelno) as i32;
                    }

                    // Quantization stepsize
                    let stepsize_idx = if resno == 0 {
                        0
                    } else {
                        3 * (resno - 1) + bandno + 1
                    };
                    if stepsize_idx < tccp.stepsizes.len() {
                        let ss = &tccp.stepsizes[stepsize_idx];
                        let numbps = img_comp.prec as i32 + tccp.numgbits as i32;
                        band.stepsize = ((1.0 + ss.mant as f32 / 2048.0)
                            * f32::powi(2.0, numbps - ss.expn))
                            * if tccp.qmfbid == 1 { 1.0 } else { 0.5 };
                        band.numbps = ss.expn as i32 + tccp.numgbits as i32 - 1;
                    }

                    if band.is_empty() {
                        continue;
                    }

                    // Build precincts
                    let num_precincts = (res.pw * res.ph) as usize;
                    band.precincts
                        .resize_with(num_precincts, TcdPrecinct::default);

                    let cblkw = 1u32 << tccp.cblkw.min(pdx);
                    let cblkh = 1u32 << tccp.cblkh.min(pdy);

                    for precno in 0..num_precincts {
                        let prc = &mut band.precincts[precno];
                        // Reset precinct to clean state
                        *prc = TcdPrecinct::default();

                        let pi = precno as u32 % res.pw;
                        let pj = precno as u32 / res.pw;

                        // Precinct boundaries with grid origin offset
                        prc.x0 = ((tprc_x0 + pi) << pdx).max(band.x0 as u32) as i32;
                        prc.y0 = ((tprc_y0 + pj) << pdy).max(band.y0 as u32) as i32;
                        prc.x1 = ((tprc_x0 + pi + 1) << pdx).min(band.x1 as u32) as i32;
                        prc.y1 = ((tprc_y0 + pj + 1) << pdy).min(band.y1 as u32) as i32;

                        let prc_w = (prc.x1 - prc.x0).max(0) as u32;
                        let prc_h = (prc.y1 - prc.y0).max(0) as u32;
                        prc.cw = uint_ceildiv(prc_w, cblkw);
                        prc.ch = uint_ceildiv(prc_h, cblkh);

                        let num_cblks = (prc.cw * prc.ch) as usize;

                        // Create tag trees
                        if num_cblks > 0 {
                            prc.incltree = Some(TagTree::new(prc.cw, prc.ch));
                            prc.imsbtree = Some(TagTree::new(prc.cw, prc.ch));
                        }

                        // Create code blocks (clamp to both precinct and band bounds)
                        let x1_max = prc.x1.min(band.x1);
                        let y1_max = prc.y1.min(band.y1);
                        if is_encoder {
                            let mut cblks = vec![TcdCblkEnc::default(); num_cblks];
                            for (cblkno, cblk) in cblks.iter_mut().enumerate() {
                                let ci = cblkno as u32 % prc.cw;
                                let cj = cblkno as u32 / prc.cw;
                                cblk.x0 = (prc.x0 as u32 + ci * cblkw) as i32;
                                cblk.y0 = (prc.y0 as u32 + cj * cblkh) as i32;
                                cblk.x1 = (cblk.x0 + cblkw as i32).min(x1_max);
                                cblk.y1 = (cblk.y0 + cblkh as i32).min(y1_max);
                            }
                            prc.cblks = TcdCodeBlocks::Enc(cblks);
                        } else {
                            let mut cblks = vec![TcdCblkDec::default(); num_cblks];
                            for (cblkno, cblk) in cblks.iter_mut().enumerate() {
                                let ci = cblkno as u32 % prc.cw;
                                let cj = cblkno as u32 / prc.cw;
                                cblk.x0 = (prc.x0 as u32 + ci * cblkw) as i32;
                                cblk.y0 = (prc.y0 as u32 + cj * cblkh) as i32;
                                cblk.x1 = (cblk.x0 + cblkw as i32).min(x1_max);
                                cblk.y1 = (cblk.y0 + cblkh as i32).min(y1_max);
                            }
                            prc.cblks = TcdCodeBlocks::Dec(cblks);
                        }
                    }
                }
            }
        }

        // Compute tile pixel count
        let tile_w = (self.tile.x1 - self.tile.x0) as usize;
        let tile_h = (self.tile.y1 - self.tile.y0) as usize;
        self.tile.numpix = tile_w * tile_h;

        Ok(())
    }

    /// Apply DC level shift for encoding (C: opj_tcd_dc_level_shift_encode).
    ///
    /// Subtracts the DC level shift from each sample before encoding.
    pub fn dc_level_shift_encode(&mut self, tcp: &TileCodingParameters) {
        for (compno, comp) in self.tile.comps.iter_mut().enumerate() {
            let dc_shift = if compno < tcp.tccps.len() {
                tcp.tccps[compno].m_dc_level_shift
            } else {
                0
            };
            if dc_shift == 0 {
                continue;
            }
            for sample in comp.data.iter_mut() {
                *sample -= dc_shift;
            }
        }
    }

    /// Apply DC level shift for decoding (C: opj_tcd_dc_level_shift_decode).
    ///
    /// Adds the DC level shift back to each sample after decoding.
    pub fn dc_level_shift_decode(&mut self, tcp: &TileCodingParameters) {
        for (compno, comp) in self.tile.comps.iter_mut().enumerate() {
            let dc_shift = if compno < tcp.tccps.len() {
                tcp.tccps[compno].m_dc_level_shift
            } else {
                0
            };
            if dc_shift == 0 {
                continue;
            }
            for sample in comp.data.iter_mut() {
                *sample += dc_shift;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- TcdLayer ---

    #[test]
    fn tcd_layer_default() {
        let layer = TcdLayer::default();
        assert_eq!(layer.numpasses, 0);
        assert_eq!(layer.len, 0);
        assert_eq!(layer.disto, 0.0);
        assert_eq!(layer.data_offset, 0);
    }

    // --- TcdCblkEnc ---

    #[test]
    fn tcd_cblk_enc_default() {
        let cblk = TcdCblkEnc::default();
        assert!(cblk.data.is_empty());
        assert!(cblk.layers.is_empty());
        assert!(cblk.passes.is_empty());
        assert_eq!(cblk.x0, 0);
        assert_eq!(cblk.numbps, 0);
        assert_eq!(cblk.totalpasses, 0);
    }

    // --- TcdSeg ---

    #[test]
    fn tcd_seg_default() {
        let seg = TcdSeg::default();
        assert_eq!(seg.len, 0);
        assert_eq!(seg.numpasses, 0);
        assert_eq!(seg.maxpasses, 0);
    }

    // --- TcdCblkDec ---

    #[test]
    fn tcd_cblk_dec_default() {
        let cblk = TcdCblkDec::default();
        assert!(cblk.segs.is_empty());
        assert!(cblk.chunks.is_empty());
        assert!(!cblk.corrupted);
        assert!(cblk.decoded_data.is_none());
    }

    // --- TcdCodeBlocks ---

    #[test]
    fn tcd_code_blocks_variants() {
        let empty = TcdCodeBlocks::Empty;
        assert!(matches!(empty, TcdCodeBlocks::Empty));

        let enc = TcdCodeBlocks::Enc(vec![TcdCblkEnc::default()]);
        assert!(matches!(enc, TcdCodeBlocks::Enc(_)));

        let dec = TcdCodeBlocks::Dec(vec![TcdCblkDec::default()]);
        assert!(matches!(dec, TcdCodeBlocks::Dec(_)));
    }

    #[test]
    fn tcd_code_blocks_default_is_empty() {
        let cblks = TcdCodeBlocks::default();
        assert!(matches!(cblks, TcdCodeBlocks::Empty));
    }

    // --- TcdPrecinct ---

    #[test]
    fn tcd_precinct_default() {
        let prec = TcdPrecinct::default();
        assert_eq!(prec.cw, 0);
        assert_eq!(prec.ch, 0);
        assert!(matches!(prec.cblks, TcdCodeBlocks::Empty));
        assert!(prec.incltree.is_none());
        assert!(prec.imsbtree.is_none());
    }

    // --- TcdBand ---

    #[test]
    fn tcd_band_default() {
        let band = TcdBand::default();
        assert_eq!(band.bandno, 0);
        assert!(band.precincts.is_empty());
        assert_eq!(band.numbps, 0);
        assert_eq!(band.stepsize, 0.0);
    }

    #[test]
    fn tcd_band_is_empty() {
        // Default band has zero area → empty
        let band = TcdBand::default();
        assert!(band.is_empty());

        // Band with positive area → not empty
        let band = TcdBand {
            x0: 0,
            y0: 0,
            x1: 32,
            y1: 32,
            ..Default::default()
        };
        assert!(!band.is_empty());

        // Band with degenerate area → empty
        let band = TcdBand {
            x0: 10,
            y0: 0,
            x1: 10,
            y1: 32,
            ..Default::default()
        };
        assert!(band.is_empty());
    }

    // --- TcdResolution ---

    #[test]
    fn tcd_resolution_default() {
        let res = TcdResolution::default();
        assert_eq!(res.pw, 0);
        assert_eq!(res.ph, 0);
        assert_eq!(res.numbands, 0);
        assert!(res.bands.is_empty());
    }

    // --- TcdTileComp ---

    #[test]
    fn tcd_tile_comp_default() {
        let tc = TcdTileComp::default();
        assert_eq!(tc.numresolutions, 0);
        assert!(tc.resolutions.is_empty());
        assert!(tc.data.is_empty());
        assert!(tc.data_win.is_none());
    }

    // --- TcdTile ---

    #[test]
    fn tcd_tile_default() {
        let tile = TcdTile::default();
        assert!(tile.comps.is_empty());
        assert_eq!(tile.numpix, 0);
        assert_eq!(tile.distotile, 0.0);
        assert_eq!(tile.packno, 0);
        assert_eq!(tile.distolayer.len(), J2K_MAXLAYERS);
    }

    // --- Tcd ---

    #[test]
    fn tcd_new_encoder() {
        let tcd = Tcd::new(false);
        assert!(!tcd.is_decoder);
        assert!(tcd.whole_tile_decoding);
        assert!(tcd.used_component.is_none());
    }

    #[test]
    fn tcd_new_decoder() {
        let tcd = Tcd::new(true);
        assert!(tcd.is_decoder);
    }

    // --- Hierarchy construction ---

    #[test]
    fn tcd_hierarchy_construction() {
        // Build a minimal 1-component, 1-resolution, 1-band, 1-precinct hierarchy
        let cblk_enc = TcdCblkEnc {
            x0: 0,
            y0: 0,
            x1: 64,
            y1: 64,
            ..Default::default()
        };

        let precinct = TcdPrecinct {
            x0: 0,
            y0: 0,
            x1: 64,
            y1: 64,
            cw: 1,
            ch: 1,
            cblks: TcdCodeBlocks::Enc(vec![cblk_enc]),
            incltree: Some(TagTree::new(1, 1)),
            imsbtree: Some(TagTree::new(1, 1)),
        };

        let band = TcdBand {
            x0: 0,
            y0: 0,
            x1: 64,
            y1: 64,
            bandno: 0,
            precincts: vec![precinct],
            numbps: 8,
            stepsize: 1.0,
        };

        let resolution = TcdResolution {
            x0: 0,
            y0: 0,
            x1: 64,
            y1: 64,
            pw: 1,
            ph: 1,
            numbands: 1,
            bands: vec![band],
            ..Default::default()
        };

        let tile_comp = TcdTileComp {
            x0: 0,
            y0: 0,
            x1: 64,
            y1: 64,
            compno: 0,
            numresolutions: 1,
            minimum_num_resolutions: 1,
            resolutions: vec![resolution],
            data: vec![0i32; 64 * 64],
            numpix: 64 * 64,
            ..Default::default()
        };

        let tile = TcdTile {
            x0: 0,
            y0: 0,
            x1: 64,
            y1: 64,
            comps: vec![tile_comp],
            numpix: 64 * 64,
            ..Default::default()
        };

        assert_eq!(tile.comps.len(), 1);
        assert_eq!(tile.comps[0].resolutions.len(), 1);
        assert_eq!(tile.comps[0].resolutions[0].bands.len(), 1);
        assert!(!tile.comps[0].resolutions[0].bands[0].is_empty());
        assert_eq!(tile.comps[0].resolutions[0].bands[0].precincts.len(), 1);
    }

    // --- TcdPass re-export ---

    #[test]
    fn tcd_pass_from_t1() {
        let pass = TcdPass {
            rate: 100,
            distortion_decrease: 5.0,
            len: 50,
            term: false,
        };
        assert_eq!(pass.rate, 100);
        assert_eq!(pass.len, 50);
    }

    // --- Pipeline tests (Phase 400d) ---

    use crate::image::{Image, ImageCompParam};
    use crate::j2k::params::{
        CodingParamMode, CodingParameters, DecodingParam, EncodingParam, TileCodingParameters,
        TileCompCodingParameters,
    };
    use crate::types::ColorSpace;

    /// Create a minimal 64x64, 1-component grayscale image + coding parameters
    /// for pipeline testing.
    fn create_test_setup(is_encoder: bool) -> (Image, CodingParameters, TileCodingParameters) {
        let params = vec![ImageCompParam {
            dx: 1,
            dy: 1,
            w: 64,
            h: 64,
            x0: 0,
            y0: 0,
            prec: 8,
            sgnd: false,
        }];
        let mut image = Image::new(&params, ColorSpace::Gray);
        image.x1 = 64;
        image.y1 = 64;

        let tccp = TileCompCodingParameters {
            numresolutions: 2,
            cblkw: 6, // 64
            cblkh: 6,
            qmfbid: 1,             // 5-3 reversible
            m_dc_level_shift: 128, // 8-bit unsigned → shift by 128
            ..Default::default()
        };
        let tcp = TileCodingParameters {
            tccps: vec![tccp],
            ..Default::default()
        };

        let cp = CodingParameters {
            tx0: 0,
            ty0: 0,
            tdx: 64,
            tdy: 64,
            tw: 1,
            th: 1,
            tcps: vec![tcp.clone()],
            mode: if is_encoder {
                CodingParamMode::Encoder(EncodingParam::default())
            } else {
                CodingParamMode::Decoder(DecodingParam::default())
            },
            ..CodingParameters::new_encoder()
        };

        (image, cp, tcp)
    }

    #[test]
    fn init_tile_basic_hierarchy() {
        let (image, cp, tcp) = create_test_setup(true);
        let mut tcd = Tcd::new(false);
        tcd.init_tile(0, &image, &cp, &tcp, true).unwrap();

        // Tile boundaries
        assert_eq!(tcd.tile.x0, 0);
        assert_eq!(tcd.tile.y0, 0);
        assert_eq!(tcd.tile.x1, 64);
        assert_eq!(tcd.tile.y1, 64);

        // 1 component
        assert_eq!(tcd.tile.comps.len(), 1);
        let comp = &tcd.tile.comps[0];
        assert_eq!(comp.x0, 0);
        assert_eq!(comp.x1, 64);
        assert_eq!(comp.numresolutions, 2);

        // 2 resolution levels
        assert_eq!(comp.resolutions.len(), 2);

        // Res 0 (coarsest): 32x32
        let res0 = &comp.resolutions[0];
        assert_eq!(res0.x1 - res0.x0, 32);
        assert_eq!(res0.y1 - res0.y0, 32);
        assert_eq!(res0.numbands, 1); // LL only

        // Res 1 (finest): 64x64
        let res1 = &comp.resolutions[1];
        assert_eq!(res1.x1 - res1.x0, 64);
        assert_eq!(res1.y1 - res1.y0, 64);
        assert_eq!(res1.numbands, 3); // HL, LH, HH
    }

    #[test]
    fn init_tile_codeblocks_created() {
        let (image, cp, tcp) = create_test_setup(true);
        let mut tcd = Tcd::new(false);
        tcd.init_tile(0, &image, &cp, &tcp, true).unwrap();

        // Check encoder code blocks exist
        let res0 = &tcd.tile.comps[0].resolutions[0];
        assert!(res0.pw > 0);
        assert!(res0.ph > 0);
        let band0 = &res0.bands[0];
        assert!(!band0.precincts.is_empty());
        let prc = &band0.precincts[0];
        assert!(matches!(prc.cblks, TcdCodeBlocks::Enc(_)));
        assert!(prc.incltree.is_some());
        assert!(prc.imsbtree.is_some());
    }

    #[test]
    fn init_tile_decoder_codeblocks() {
        let (image, cp, tcp) = create_test_setup(false);
        let mut tcd = Tcd::new(true);
        tcd.init_tile(0, &image, &cp, &tcp, false).unwrap();

        let res0 = &tcd.tile.comps[0].resolutions[0];
        let band0 = &res0.bands[0];
        let prc = &band0.precincts[0];
        assert!(matches!(prc.cblks, TcdCodeBlocks::Dec(_)));
    }

    #[test]
    fn dc_level_shift_roundtrip() {
        let (image, cp, tcp) = create_test_setup(true);
        let mut tcd = Tcd::new(false);
        tcd.init_tile(0, &image, &cp, &tcp, true).unwrap();

        // Fill with test data
        let original: Vec<i32> = (0..tcd.tile.comps[0].data.len() as i32).collect();
        tcd.tile.comps[0].data = original.clone();

        // Encode shift: subtract 128
        tcd.dc_level_shift_encode(&tcp);
        assert_eq!(tcd.tile.comps[0].data[0], original[0] - 128);
        assert_eq!(tcd.tile.comps[0].data[100], original[100] - 128);

        // Decode shift: add 128
        tcd.dc_level_shift_decode(&tcp);
        assert_eq!(tcd.tile.comps[0].data, original);
    }

    #[test]
    fn dc_level_shift_zero_is_noop() {
        let (image, cp, _) = create_test_setup(true);
        let tcp_no_shift = TileCodingParameters {
            tccps: vec![TileCompCodingParameters {
                m_dc_level_shift: 0,
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut tcd = Tcd::new(false);
        tcd.init_tile(0, &image, &cp, &tcp_no_shift, true).unwrap();

        let data: Vec<i32> = vec![42; tcd.tile.comps[0].data.len()];
        tcd.tile.comps[0].data = data.clone();
        tcd.dc_level_shift_encode(&tcp_no_shift);
        assert_eq!(tcd.tile.comps[0].data, data);
    }
}
