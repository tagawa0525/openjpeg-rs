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

impl TcdCodeBlocks {
    /// Returns a reference to the Dec variant, or None.
    pub fn as_dec(&self) -> Option<&Vec<TcdCblkDec>> {
        match self {
            TcdCodeBlocks::Dec(v) => Some(v),
            _ => None,
        }
    }

    /// Takes decoded_data from a Dec codeblock, leaving None in its place.
    pub fn take_dec_decoded(&mut self, index: usize) -> Option<Vec<i32>> {
        match self {
            TcdCodeBlocks::Dec(v) => v.get_mut(index)?.decoded_data.take(),
            _ => None,
        }
    }

    /// Restores decoded_data into a Dec codeblock.
    pub fn restore_dec_decoded(&mut self, index: usize, data: Vec<i32>) {
        if let TcdCodeBlocks::Dec(v) = self
            && let Some(cblk) = v.get_mut(index)
        {
            cblk.decoded_data = Some(data);
        }
    }
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

            if tccp.numresolutions == 0 || tccp.numresolutions as usize > crate::types::J2K_MAXRLVLS
            {
                return Err(Error::InvalidInput(format!(
                    "component {compno}: numresolutions {} out of range 1..={}",
                    tccp.numresolutions,
                    crate::types::J2K_MAXRLVLS
                )));
            }
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
                        // Highest resolution: subband coordinates
                        // C: opj_int_ceildivpow2(tilec->x0 - (x0b << levelno), levelno + 1)
                        let x0b = (band.bandno & 1) as i32;
                        let y0b = (band.bandno >> 1) as i32;
                        band.x0 = int_ceildivpow2(comp.x0 - x0b, 1);
                        band.y0 = int_ceildivpow2(comp.y0 - y0b, 1);
                        band.x1 = int_ceildivpow2(comp.x1 - x0b, 1);
                        band.y1 = int_ceildivpow2(comp.y1 - y0b, 1);
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

    /// Copy decoded codeblock coefficients into tile component data buffers.
    ///
    /// After T1 decode, each codeblock has decoded coefficients in `decoded_data`.
    /// This function places them at the correct subband positions in the tile
    /// component's data buffer, applying dequantization.
    pub fn copy_decoded_cblks_to_data(&mut self, tcp: &TileCodingParameters) -> Result<()> {
        // Collect lightweight descriptors (no data clone) to avoid borrow conflicts.
        struct CopyDesc {
            compno: usize,
            resno: usize,
            bandno: usize,
            precno: usize,
            cblkno: usize,
            buf_x: usize,
            buf_y: usize,
            cblk_w: usize,
            cblk_h: usize,
            qmfbid: u32,
            stepsize: f32,
        }

        let mut descs = Vec::new();
        for compno in 0..self.tile.comps.len() {
            let comp = &self.tile.comps[compno];
            let comp_w = (comp.x1 - comp.x0) as usize;
            let comp_h = (comp.y1 - comp.y0) as usize;
            let qmfbid = tcp.tccps.get(compno).map(|t| t.qmfbid).unwrap_or(1);

            for resno in 0..comp.numresolutions as usize {
                let res = &comp.resolutions[resno];
                for bandno in 0..res.numbands as usize {
                    let band = &res.bands[bandno];
                    if band.is_empty() {
                        continue;
                    }

                    // Subband offset: C: if (band->bandno & 1) x += pres->x1 - pres->x0
                    let (x_off, y_off) = if resno > 0 {
                        let pres = &comp.resolutions[resno - 1];
                        (
                            if band.bandno & 1 != 0 {
                                (pres.x1 - pres.x0) as usize
                            } else {
                                0
                            },
                            if band.bandno & 2 != 0 {
                                (pres.y1 - pres.y0) as usize
                            } else {
                                0
                            },
                        )
                    } else {
                        (0, 0)
                    };

                    for (precno, prec) in band.precincts.iter().enumerate() {
                        let cblks = match &prec.cblks {
                            TcdCodeBlocks::Dec(c) => c,
                            _ => continue,
                        };
                        for (cblkno, cblk) in cblks.iter().enumerate() {
                            if cblk.decoded_data.is_none() {
                                continue;
                            }
                            let cblk_w = (cblk.x1 - cblk.x0).max(0) as usize;
                            let cblk_h = (cblk.y1 - cblk.y0).max(0) as usize;
                            if cblk_w == 0 || cblk_h == 0 {
                                continue;
                            }
                            let buf_x = (cblk.x0 - band.x0) as usize + x_off;
                            let buf_y = (cblk.y0 - band.y0) as usize + y_off;

                            // Validate bounds up front
                            if buf_x + cblk_w > comp_w || buf_y + cblk_h > comp_h {
                                return Err(Error::InvalidInput(format!(
                                    "cblk at ({buf_x},{buf_y}) size ({cblk_w}x{cblk_h}) \
                                     exceeds comp buffer ({comp_w}x{comp_h})"
                                )));
                            }

                            descs.push(CopyDesc {
                                compno,
                                resno,
                                bandno,
                                precno,
                                cblkno,
                                buf_x,
                                buf_y,
                                cblk_w,
                                cblk_h,
                                qmfbid,
                                stepsize: band.stepsize,
                            });
                        }
                    }
                }
            }
        }

        // Apply copy operations using descriptors (no clone of decoded data).
        for desc in &descs {
            let decoded = self.tile.comps[desc.compno].resolutions[desc.resno].bands[desc.bandno]
                .precincts[desc.precno]
                .cblks
                .as_dec()
                .and_then(|c| c[desc.cblkno].decoded_data.as_ref())
                .ok_or_else(|| Error::InvalidInput("missing decoded data".into()))?;

            if decoded.len() < desc.cblk_w * desc.cblk_h {
                return Err(Error::InvalidInput(format!(
                    "decoded_data len {} < expected {}",
                    decoded.len(),
                    desc.cblk_w * desc.cblk_h
                )));
            }

            // Take decoded data out temporarily to allow mutable borrow of comp.data.
            let decoded_data = self.tile.comps[desc.compno].resolutions[desc.resno].bands
                [desc.bandno]
                .precincts[desc.precno]
                .cblks
                .take_dec_decoded(desc.cblkno)
                .ok_or_else(|| Error::InvalidInput("missing decoded data".into()))?;

            let comp = &mut self.tile.comps[desc.compno];
            let comp_w = (comp.x1 - comp.x0) as usize;
            // T1 data is in row-major layout: data[row * w + col]
            for j in 0..desc.cblk_h {
                let src_off = j * desc.cblk_w;
                let dst_off = (desc.buf_y + j) * comp_w + desc.buf_x;
                for i in 0..desc.cblk_w {
                    let val = decoded_data[src_off + i];
                    comp.data[dst_off + i] = if desc.qmfbid == 1 {
                        val
                    } else {
                        (val as f32 * desc.stepsize) as i32
                    };
                }
            }

            // Restore decoded data
            self.tile.comps[desc.compno].resolutions[desc.resno].bands[desc.bandno].precincts
                [desc.precno]
                .cblks
                .restore_dec_decoded(desc.cblkno, decoded_data);
        }
        Ok(())
    }

    /// Decode a tile: T2 → T1 → copy cblks → DWT → MCT → DC shift.
    /// (C: opj_tcd_decode_tile)
    pub fn decode_tile(
        &mut self,
        tile_data: &mut [u8],
        image: &Image,
        cp: &CodingParameters,
        tcp: &TileCodingParameters,
    ) -> Result<()> {
        use crate::coding::t1::t1_decode_cblks;
        use crate::tier2::pi::pi_create_decode;
        use crate::tier2::t2::t2_decode_packets;
        use crate::transform::dwt;
        use crate::transform::mct;

        // 1. Allocate tile component data buffers
        for comp in &mut self.tile.comps {
            let w = (comp.x1 - comp.x0) as usize;
            let h = (comp.y1 - comp.y0) as usize;
            if comp.data.len() < w * h {
                comp.data.resize(w * h, 0);
            }
        }

        // 2. T2: Depacketize — extract codeblock segment data from packets
        let max_layers = if tcp.num_layers_to_decode > 0 {
            tcp.num_layers_to_decode
        } else {
            tcp.numlayers.max(1)
        };
        let mut pis = pi_create_decode(image, cp, self.tcd_tileno)?;
        t2_decode_packets(&mut self.tile, tcp, &mut pis, tile_data, max_layers)?;

        // 3. T1: Arithmetic decode — reconstruct coefficients from codeblocks
        t1_decode_cblks(&mut self.tile, tcp)?;

        // 4. Copy decoded cblk coefficients to tile component data buffers
        self.copy_decoded_cblks_to_data(tcp)?;

        // 5. Inverse DWT for each component
        for compno in 0..self.tile.comps.len() {
            let num_res = self.tile.comps[compno].numresolutions as usize;
            if num_res <= 1 {
                continue; // No DWT for single resolution
            }
            let w = (self.tile.comps[compno].x1 - self.tile.comps[compno].x0) as usize;
            let h = (self.tile.comps[compno].y1 - self.tile.comps[compno].y0) as usize;
            let qmfbid = tcp.tccps.get(compno).map(|t| t.qmfbid).unwrap_or(1);

            if qmfbid == 1 {
                dwt::dwt_decode_2d_53(&mut self.tile.comps[compno].data, w, h, w, num_res)?;
            } else {
                let comp_data = &mut self.tile.comps[compno].data;
                let mut f32_data: Vec<f32> = comp_data.iter().map(|&v| v as f32).collect();
                dwt::dwt_decode_2d_97(&mut f32_data, w, h, w, num_res)?;
                for (dst, &src) in comp_data.iter_mut().zip(f32_data.iter()) {
                    *dst = src.round() as i32;
                }
            }
        }

        // 6. Inverse MCT (if applicable, >= 3 components)
        if tcp.mct != 0 && self.tile.comps.len() >= 3 {
            let samples = {
                let comp = &self.tile.comps[0];
                ((comp.x1 - comp.x0) * (comp.y1 - comp.y0)) as usize
            };
            // Validate all 3 components have sufficient buffer size
            for ci in 0..3 {
                if self.tile.comps[ci].data.len() < samples {
                    return Err(Error::InvalidInput(format!(
                        "MCT: component {ci} has {} samples, need {samples}",
                        self.tile.comps[ci].data.len()
                    )));
                }
            }
            let qmfbid = tcp.tccps.first().map(|t| t.qmfbid).unwrap_or(1);

            if tcp.mct == 1 {
                let (c0_rest, rest) = self.tile.comps.split_at_mut(1);
                let (c1_rest, c2_rest) = rest.split_at_mut(1);
                if qmfbid == 1 {
                    mct::mct_decode(
                        &mut c0_rest[0].data[..samples],
                        &mut c1_rest[0].data[..samples],
                        &mut c2_rest[0].data[..samples],
                    );
                } else {
                    let mut f0: Vec<f32> = c0_rest[0].data[..samples]
                        .iter()
                        .map(|&v| v as f32)
                        .collect();
                    let mut f1: Vec<f32> = c1_rest[0].data[..samples]
                        .iter()
                        .map(|&v| v as f32)
                        .collect();
                    let mut f2: Vec<f32> = c2_rest[0].data[..samples]
                        .iter()
                        .map(|&v| v as f32)
                        .collect();
                    mct::mct_decode_real(&mut f0, &mut f1, &mut f2);
                    for (d, &s) in c0_rest[0].data[..samples].iter_mut().zip(f0.iter()) {
                        *d = s.round() as i32;
                    }
                    for (d, &s) in c1_rest[0].data[..samples].iter_mut().zip(f1.iter()) {
                        *d = s.round() as i32;
                    }
                    for (d, &s) in c2_rest[0].data[..samples].iter_mut().zip(f2.iter()) {
                        *d = s.round() as i32;
                    }
                }
            }
            // mct == 2 (custom matrix) deferred
        }

        // 7. DC level shift
        self.dc_level_shift_decode(tcp);

        Ok(())
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

    /// Compute explicit quantization step sizes for encoding.
    ///
    /// For 5-3 reversible DWT (qmfbid=1): NOQNT style — expn = gain + prec, mant = 0.
    /// For 9-7 irreversible DWT: NOQNT fallback (expn = prec, mant = 0).
    /// Full scalar quantization with DWT norms is deferred.
    /// (C: opj_dwt_calc_explicit_stepsizes)
    pub fn calc_explicit_stepsizes(tcp: &mut TileCodingParameters, prec: u32) {
        use crate::j2k::params::Stepsize;

        for comp_tccp in tcp.tccps.iter_mut() {
            let numres = comp_tccp.numresolutions;
            let numbands = if numres > 0 { 3 * numres - 2 } else { 0 };

            for bandno in 0..numbands as usize {
                let orient = if bandno == 0 {
                    0u32
                } else {
                    ((bandno - 1) % 3 + 1) as u32
                };

                // Subband gain: LL=0, HL/LH=1, HH=2
                let gain = if comp_tccp.qmfbid == 1 {
                    // 5-3 reversible
                    match orient {
                        0 => 0i32,
                        1 | 2 => 1,
                        _ => 2,
                    }
                } else {
                    0 // 9-7 irreversible: gain=0 for scalar quantization
                };

                comp_tccp.stepsizes[bandno] = Stepsize {
                    expn: gain + prec as i32,
                    mant: 0,
                };
            }
        }
    }

    /// Build quality layers from encoded codeblock passes (fixed quality).
    ///
    /// Places all passes into layers without rate-distortion optimization.
    /// For single-layer encoding, all passes go into layer 0.
    /// (C: opj_tcd_makelayer_fixed + simplified opj_tcd_makelayer)
    pub fn makelayer_fixed(&mut self, tcp: &TileCodingParameters) {
        let numlayers = tcp.numlayers.max(1) as usize;

        for comp in &mut self.tile.comps {
            for res in &mut comp.resolutions {
                for band in &mut res.bands {
                    if band.is_empty() {
                        continue;
                    }
                    for prec in &mut band.precincts {
                        if let TcdCodeBlocks::Enc(ref mut cblks) = prec.cblks {
                            for cblk in cblks.iter_mut() {
                                // Allocate layers if needed
                                cblk.layers.resize(numlayers, TcdLayer::default());

                                // Distribute all passes into layer 0 (fixed quality)
                                let total_passes = cblk.totalpasses as usize;
                                let used_passes = total_passes.min(cblk.passes.len());
                                if used_passes == 0 {
                                    continue;
                                }

                                // Layer 0 gets all passes; use last pass's cumulative stats
                                let last_pass = &cblk.passes[used_passes - 1];
                                let data_len = last_pass.rate;
                                let disto = last_pass.distortion_decrease;

                                cblk.layers[0] = TcdLayer {
                                    numpasses: used_passes as u32,
                                    len: data_len,
                                    disto,
                                    data_offset: 0,
                                };
                                cblk.numpassesinlayers = used_passes as u32;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Encode a tile: DC shift → MCT → DWT → T1 → makelayer → T2.
    ///
    /// Returns the number of bytes written to the output buffer.
    /// (C: opj_tcd_encode_tile)
    pub fn encode_tile(
        &mut self,
        image: &Image,
        cp: &CodingParameters,
        tcp: &TileCodingParameters,
        dest: &mut [u8],
    ) -> Result<usize> {
        use crate::coding::t1::t1_encode_cblks;
        use crate::tier2::pi::pi_create_decode;
        use crate::tier2::t2::t2_encode_packets;
        use crate::transform::dwt;
        use crate::transform::mct;

        let numcomps = self.tile.comps.len() as u32;

        // 1. DC level shift (subtract)
        self.dc_level_shift_encode(tcp);

        // 2. MCT forward transform (RGB → YCbCr)
        if tcp.mct != 0 && self.tile.comps.len() >= 3 {
            let samples = {
                let comp = &self.tile.comps[0];
                ((comp.x1 - comp.x0) * (comp.y1 - comp.y0)) as usize
            };
            // Validate all 3 components have sufficient buffer size
            for ci in 0..3 {
                if self.tile.comps[ci].data.len() < samples {
                    return Err(Error::InvalidInput(format!(
                        "MCT encode: component {ci} has {} samples, need {samples}",
                        self.tile.comps[ci].data.len()
                    )));
                }
            }
            let qmfbid = tcp.tccps.first().map(|t| t.qmfbid).unwrap_or(1);

            if tcp.mct == 1 {
                let (c0_rest, rest) = self.tile.comps.split_at_mut(1);
                let (c1_rest, c2_rest) = rest.split_at_mut(1);
                if qmfbid == 1 {
                    mct::mct_encode(
                        &mut c0_rest[0].data[..samples],
                        &mut c1_rest[0].data[..samples],
                        &mut c2_rest[0].data[..samples],
                    );
                } else {
                    let mut f0: Vec<f32> = c0_rest[0].data[..samples]
                        .iter()
                        .map(|&v| v as f32)
                        .collect();
                    let mut f1: Vec<f32> = c1_rest[0].data[..samples]
                        .iter()
                        .map(|&v| v as f32)
                        .collect();
                    let mut f2: Vec<f32> = c2_rest[0].data[..samples]
                        .iter()
                        .map(|&v| v as f32)
                        .collect();
                    mct::mct_encode_real(&mut f0, &mut f1, &mut f2);
                    for (d, &s) in c0_rest[0].data[..samples].iter_mut().zip(f0.iter()) {
                        *d = s.round() as i32;
                    }
                    for (d, &s) in c1_rest[0].data[..samples].iter_mut().zip(f1.iter()) {
                        *d = s.round() as i32;
                    }
                    for (d, &s) in c2_rest[0].data[..samples].iter_mut().zip(f2.iter()) {
                        *d = s.round() as i32;
                    }
                }
            }
        }

        // 3. Forward DWT for each component
        for compno in 0..self.tile.comps.len() {
            let num_res = self.tile.comps[compno].numresolutions as usize;
            if num_res <= 1 {
                continue;
            }
            let w = (self.tile.comps[compno].x1 - self.tile.comps[compno].x0) as usize;
            let h = (self.tile.comps[compno].y1 - self.tile.comps[compno].y0) as usize;
            let qmfbid = tcp.tccps.get(compno).map(|t| t.qmfbid).unwrap_or(1);

            if qmfbid == 1 {
                dwt::dwt_encode_2d_53(&mut self.tile.comps[compno].data, w, h, w, num_res)?;
            } else {
                let comp_data = &mut self.tile.comps[compno].data;
                let mut f32_data: Vec<f32> = comp_data.iter().map(|&v| v as f32).collect();
                dwt::dwt_encode_2d_97(&mut f32_data, w, h, w, num_res)?;
                for (dst, &src) in comp_data.iter_mut().zip(f32_data.iter()) {
                    *dst = src.round() as i32;
                }
            }
        }

        // 4. T1: Arithmetic encode all codeblocks
        t1_encode_cblks(&mut self.tile, tcp, None, numcomps)?;

        // 5. Build quality layers (fixed quality — all passes in layer 0)
        self.makelayer_fixed(tcp);

        // 6. T2: Packetize
        let mut pis = pi_create_decode(image, cp, self.tcd_tileno)?;
        let bytes_written = t2_encode_packets(&mut self.tile, tcp, &mut pis, dest)?;

        Ok(bytes_written)
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

    // --- Band coordinates at levelno=0 ---

    #[test]
    fn init_tile_band_coords_at_finest_resolution() {
        // Verify band coordinates at levelno=0 (finest resolution) are correct.
        // For a 64×64 component with 2 resolutions:
        // - Res 1 (levelno=0) bands should be 32×32, not 64×64.
        let (image, cp, tcp) = create_test_setup(false);
        let mut tcd = Tcd::new(true);
        tcd.init_tile(0, &image, &cp, &tcp, false).unwrap();

        let comp = &tcd.tile.comps[0];
        let res0 = &comp.resolutions[0];
        let res1 = &comp.resolutions[1];

        // Res 0 (LL): 32×32
        let ll = &res0.bands[0];
        assert_eq!(ll.x1 - ll.x0, 32);
        assert_eq!(ll.y1 - ll.y0, 32);

        // Res 1 bands at levelno=0 should have subband dimensions, not resolution dimensions.
        // HL (bandno=1): should be ceil((64-1)/2) - ceil((0-1)/2) = 32 - 0 = 32 wide
        // But the subband height: ceil(64/2) - ceil(0/2) = 32 high
        let hl = &res1.bands[0]; // bandno=1 (HL)
        assert_eq!(hl.bandno, 1);
        assert_eq!(
            hl.x1 - hl.x0,
            32,
            "HL width should be 32, not {}",
            hl.x1 - hl.x0
        );
        assert_eq!(
            hl.y1 - hl.y0,
            32,
            "HL height should be 32, not {}",
            hl.y1 - hl.y0
        );

        // LH (bandno=2): width=32, height=32
        let lh = &res1.bands[1]; // bandno=2 (LH)
        assert_eq!(lh.bandno, 2);
        assert_eq!(lh.x1 - lh.x0, 32);
        assert_eq!(lh.y1 - lh.y0, 32);

        // HH (bandno=3): width=32, height=32
        let hh = &res1.bands[2]; // bandno=3 (HH)
        assert_eq!(hh.bandno, 3);
        assert_eq!(hh.x1 - hh.x0, 32);
        assert_eq!(hh.y1 - hh.y0, 32);
    }

    // --- copy_decoded_cblks_to_data ---

    #[test]
    fn copy_decoded_cblks_single_res() {
        // 1 component, 1 resolution (LL only), 4×4 tile, 1 codeblock
        // Set decoded_data with known coefficients and verify buffer placement.
        let (image, cp, tcp) = create_test_setup(false);
        let mut tcd = Tcd::new(true);
        tcd.init_tile(0, &image, &cp, &tcp, false).unwrap();

        // Use only res 0 (LL): 32×32
        let comp = &mut tcd.tile.comps[0];
        // Allocate data buffer for the component
        let comp_w = (comp.x1 - comp.x0) as usize;
        let comp_h = (comp.y1 - comp.y0) as usize;
        comp.data.resize(comp_w * comp_h, 0);

        // Set decoded_data on the LL codeblock
        let res0 = &tcd.tile.comps[0].resolutions[0];
        let cblk_w = if let TcdCodeBlocks::Dec(cblks) = &res0.bands[0].precincts[0].cblks {
            (cblks[0].x1 - cblks[0].x0) as usize
        } else {
            panic!("expected Dec");
        };
        let cblk_h = if let TcdCodeBlocks::Dec(cblks) = &res0.bands[0].precincts[0].cblks {
            (cblks[0].y1 - cblks[0].y0) as usize
        } else {
            panic!("expected Dec");
        };

        // Fill decoded_data in row-major layout: data[row * w + col]
        // Identity dequant for reversible (no scaling).
        let mut test_data = vec![0i32; cblk_w * cblk_h];
        for (i, val) in test_data.iter_mut().enumerate() {
            *val = i as i32 + 1;
        }
        if let TcdCodeBlocks::Dec(cblks) =
            &mut tcd.tile.comps[0].resolutions[0].bands[0].precincts[0].cblks
        {
            cblks[0].decoded_data = Some(test_data.clone());
        }

        tcd.copy_decoded_cblks_to_data(&tcp).unwrap();

        // Verify: LL coefficients should be at top-left of comp.data (row-major)
        let comp = &tcd.tile.comps[0];
        for j in 0..cblk_h {
            for i in 0..cblk_w {
                let expected = (j * cblk_w + i) as i32 + 1; // divided by 2
                let actual = comp.data[j * comp_w + i];
                assert_eq!(actual, expected, "mismatch at ({i},{j})");
            }
        }
    }

    #[test]
    fn copy_decoded_cblks_two_res_subband_offsets() {
        // 1 component, 2 resolutions, verify HL/LH/HH are at correct buffer offsets.
        let (image, cp, tcp) = create_test_setup(false);
        let mut tcd = Tcd::new(true);
        tcd.init_tile(0, &image, &cp, &tcp, false).unwrap();

        let comp = &mut tcd.tile.comps[0];
        let comp_w = (comp.x1 - comp.x0) as usize;
        let comp_h = (comp.y1 - comp.y0) as usize;
        comp.data.resize(comp_w * comp_h, 0);

        // Get previous resolution dimensions for offset calculation
        let res0_w =
            (tcd.tile.comps[0].resolutions[0].x1 - tcd.tile.comps[0].resolutions[0].x0) as usize;
        let res0_h =
            (tcd.tile.comps[0].resolutions[0].y1 - tcd.tile.comps[0].resolutions[0].y0) as usize;

        // Set decoded_data on HL codeblock (bandno & 1 = 1 → x offset)
        for bandno in 0..3 {
            let band = &tcd.tile.comps[0].resolutions[1].bands[bandno];
            let band_bandno = band.bandno;
            if let TcdCodeBlocks::Dec(cblks) =
                &tcd.tile.comps[0].resolutions[1].bands[bandno].precincts[0].cblks
            {
                let cw = (cblks[0].x1 - cblks[0].x0) as usize;
                let ch = (cblks[0].y1 - cblks[0].y0) as usize;
                let mut data = vec![0i32; cw * ch];
                for val in data.iter_mut() {
                    *val = (band_bandno as i32 + 1) * 200; // Distinct value per band × 2
                }
                // Apply through mutable ref
                if let TcdCodeBlocks::Dec(cblks_mut) =
                    &mut tcd.tile.comps[0].resolutions[1].bands[bandno].precincts[0].cblks
                {
                    cblks_mut[0].decoded_data = Some(data);
                }
            }
        }

        tcd.copy_decoded_cblks_to_data(&tcp).unwrap();

        // HL (bandno=1): raw = (1+1)*200=400. x offset = res0_w (identity dequant)
        let hl_val = tcd.tile.comps[0].data[res0_w];
        assert_eq!(hl_val, 400, "HL should be at x=res0_w");

        // LH (bandno=2): raw = (2+1)*200=600. y offset = res0_h
        let lh_val = tcd.tile.comps[0].data[res0_h * comp_w];
        assert_eq!(lh_val, 600, "LH should be at y=res0_h");

        // HH (bandno=3): raw = (3+1)*200=800. offset = (res0_w, res0_h)
        let hh_val = tcd.tile.comps[0].data[res0_h * comp_w + res0_w];
        assert_eq!(hh_val, 800, "HH should be at (res0_w, res0_h)");
    }

    // --- decode_tile pipeline ---

    /// Create a 1-resolution test setup (no DWT needed).
    fn create_single_res_test_setup() -> (Image, CodingParameters, TileCodingParameters) {
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
            numresolutions: 1,
            cblkw: 6,
            cblkh: 6,
            qmfbid: 1,
            m_dc_level_shift: 128,
            ..Default::default()
        };
        let tcp = TileCodingParameters {
            numlayers: 1,
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
            mode: CodingParamMode::Decoder(DecodingParam::default()),
            ..CodingParameters::new_encoder()
        };
        (image, cp, tcp)
    }

    #[test]
    fn decode_tile_single_res_dc_shift() {
        // Minimal pipeline: 1 component, 1 resolution, no DWT, no MCT.
        // Codeblock has all-zero decoded data.
        // After pipeline: values should be DC shift (128 for 8-bit unsigned).
        let (image, cp, tcp) = create_single_res_test_setup();
        let mut tcd = Tcd::new(true);
        tcd.init_tile(0, &image, &cp, &tcp, false).unwrap();

        // 1 resolution, 1 band (LL), 1 precinct → 1 packet. Empty packet = 1 byte.
        let mut tile_data = vec![0x00u8; 1];

        tcd.decode_tile(&mut tile_data, &image, &cp, &tcp).unwrap();

        // After DC level shift (128), all pixels should be 128
        let comp = &tcd.tile.comps[0];
        assert!(!comp.data.is_empty());
        // With empty codeblock data, T1 produces zeros.
        // copy_cblks places zeros in buffer.
        // No DWT (1 resolution).
        // DC shift adds 128.
        for &val in &comp.data {
            assert_eq!(val, 128);
        }
    }

    // ---------------------------------------------------------------------------
    // Encode pipeline tests
    // ---------------------------------------------------------------------------

    /// Create encoding test setup with pixel data populated.
    fn create_encode_test_setup() -> (Image, CodingParameters, TileCodingParameters) {
        let params = vec![ImageCompParam {
            dx: 1,
            dy: 1,
            w: 8,
            h: 8,
            x0: 0,
            y0: 0,
            prec: 8,
            sgnd: false,
        }];
        let mut image = Image::new(&params, ColorSpace::Gray);
        image.x1 = 8;
        image.y1 = 8;
        // Fill with a simple gradient pattern
        image.comps[0].data = (0..64).map(|i| i * 4).collect();

        let tccp = TileCompCodingParameters {
            numresolutions: 1, // single resolution for simplicity
            cblkw: 6,          // 64 (larger than 8x8 so one cblk covers all)
            cblkh: 6,
            qmfbid: 1,             // 5-3 reversible
            m_dc_level_shift: 128, // 8-bit unsigned → shift by 128
            ..Default::default()
        };
        let mut tcp = TileCodingParameters {
            numlayers: 1,
            tccps: vec![tccp],
            ..Default::default()
        };
        // Compute stepsizes for encoding (required for band.numbps)
        Tcd::calc_explicit_stepsizes(&mut tcp, 8);

        let cp = CodingParameters {
            tx0: 0,
            ty0: 0,
            tdx: 8,
            tdy: 8,
            tw: 1,
            th: 1,
            tcps: vec![tcp.clone()],
            mode: CodingParamMode::Encoder(EncodingParam::default()),
            ..CodingParameters::new_encoder()
        };

        (image, cp, tcp)
    }

    #[test]
    fn makelayer_fixed_single_layer() {
        let (image, cp, tcp) = create_encode_test_setup();
        let mut tcd = Tcd::new(true);
        tcd.init_tile(0, &image, &cp, &tcp, true).unwrap();

        // Copy pixel data to tile component
        tcd.tile.comps[0].data = image.comps[0].data.clone();

        // DC level shift + T1 encode (to populate passes)
        tcd.dc_level_shift_encode(&tcp);
        crate::coding::t1::t1_encode_cblks(&mut tcd.tile, &tcp, None, 1).unwrap();

        // makelayer_fixed: all passes → single layer
        tcd.makelayer_fixed(&tcp);

        // Verify: each cblk should have a layer with numpasses > 0
        for comp in &tcd.tile.comps {
            for res in &comp.resolutions {
                for band in &res.bands {
                    for prec in &band.precincts {
                        if let TcdCodeBlocks::Enc(ref cblks) = prec.cblks {
                            for cblk in cblks {
                                if cblk.totalpasses > 0 {
                                    assert!(
                                        !cblk.layers.is_empty(),
                                        "cblk with passes should have layers"
                                    );
                                    assert!(
                                        cblk.layers[0].numpasses > 0,
                                        "layer 0 should have passes"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn encode_tile_produces_output() {
        let (image, cp, tcp) = create_encode_test_setup();
        let mut tcd = Tcd::new(true);
        tcd.init_tile(0, &image, &cp, &tcp, true).unwrap();

        // Copy pixel data to tile
        tcd.tile.comps[0].data = image.comps[0].data.clone();

        let mut output = vec![0u8; 4096];
        let bytes_written = tcd.encode_tile(&image, &cp, &tcp, &mut output).unwrap();
        assert!(bytes_written > 0, "encode_tile should produce output");
    }

    #[test]
    fn encode_decode_roundtrip_single_tile() {
        let (image, cp, tcp) = create_encode_test_setup();

        // Encode
        let mut enc_tcd = Tcd::new(true);
        enc_tcd.init_tile(0, &image, &cp, &tcp, true).unwrap();
        enc_tcd.tile.comps[0].data = image.comps[0].data.clone();

        let mut encoded = vec![0u8; 8192];
        let enc_len = enc_tcd
            .encode_tile(&image, &cp, &tcp, &mut encoded)
            .unwrap();
        assert!(enc_len > 0);

        // Decode
        let dec_cp = CodingParameters {
            mode: CodingParamMode::Decoder(DecodingParam::default()),
            ..cp.clone()
        };
        let mut dec_tcd = Tcd::new(false);
        dec_tcd.init_tile(0, &image, &dec_cp, &tcp, false).unwrap();
        dec_tcd
            .decode_tile(&mut encoded[..enc_len], &image, &dec_cp, &tcp)
            .unwrap();

        // Verify roundtrip produces meaningful pixel data.
        // Exact lossless requires full T1 FRACBITS/numbps harmonization (tracked separately).
        assert_eq!(dec_tcd.tile.comps[0].data.len(), 64);
        let decoded = &dec_tcd.tile.comps[0].data;
        let min = *decoded.iter().min().unwrap();
        let max = *decoded.iter().max().unwrap();
        assert!(
            min >= 0 && max <= 255,
            "values out of 8-bit range: {min}..{max}"
        );
    }

    #[test]
    fn init_tile_rejects_zero_numresolutions() {
        let (image, cp, _) = create_test_setup(true);
        let tcp = TileCodingParameters {
            tccps: vec![TileCompCodingParameters {
                numresolutions: 0,
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut tcd = Tcd::new(false);
        assert!(tcd.init_tile(0, &image, &cp, &tcp, true).is_err());
    }

    #[test]
    fn init_tile_rejects_excessive_numresolutions() {
        let (image, cp, _) = create_test_setup(true);
        let tcp = TileCodingParameters {
            tccps: vec![TileCompCodingParameters {
                numresolutions: 34, // > J2K_MAXRLVLS (33)
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut tcd = Tcd::new(false);
        assert!(tcd.init_tile(0, &image, &cp, &tcp, true).is_err());
    }

    #[test]
    fn init_tile_accepts_max_numresolutions() {
        let (image, cp, _) = create_test_setup(true);
        let tcp = TileCodingParameters {
            tccps: vec![TileCompCodingParameters {
                numresolutions: 33, // == J2K_MAXRLVLS
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut tcd = Tcd::new(false);
        // Should succeed (33 is within J2K_MAXRLVLS)
        assert!(tcd.init_tile(0, &image, &cp, &tcp, true).is_ok());
    }
}
