// Phase 400a: TCD (Tile Coder/Decoder) data structures
//
// Defines the TCD hierarchy: Tile → TileComp → Resolution → Band → Precinct → Codeblock.
// Pipeline functions (init_tile, encode_tile, decode_tile) will be added in Phase 400d.

use crate::coding::t1::TcdPass;
use crate::coding::tgt::TagTree;
use crate::types::J2K_MAXLAYERS;

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
}
