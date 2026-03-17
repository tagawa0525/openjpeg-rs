// Phase 600: JP2 file format module
//
// Box constants, JP2 types, and sub-module declarations.

pub mod read;

// ---------------------------------------------------------------------------
// Box type constants (C: JP2_*)
// ---------------------------------------------------------------------------

/// JPEG 2000 signature box.
pub const JP2_JP: u32 = 0x6A50_2020;
/// File type box.
pub const JP2_FTYP: u32 = 0x6674_7970;
/// JP2 header box (super-box).
pub const JP2_JP2H: u32 = 0x6A70_3268;
/// Image header box.
pub const JP2_IHDR: u32 = 0x6968_6472;
/// Colour specification box.
pub const JP2_COLR: u32 = 0x636F_6C72;
/// Contiguous codestream box.
pub const JP2_JP2C: u32 = 0x6A70_3263;
/// Palette box.
pub const JP2_PCLR: u32 = 0x7063_6C72;
/// Component Mapping box.
pub const JP2_CMAP: u32 = 0x636D_6170;
/// Channel Definition box.
pub const JP2_CDEF: u32 = 0x6364_6566;
/// Bits per component box.
pub const JP2_BPCC: u32 = 0x6270_6363;
/// File type brand "jp2 ".
pub const JP2_JP2_BRAND: u32 = 0x6A70_3220;

/// JP2 signature box magic number (0x0D0A870A).
pub const JP2_MAGIC: u32 = 0x0D0A_870A;

// ---------------------------------------------------------------------------
// JP2 state (C: JP2_STATE)
// ---------------------------------------------------------------------------

/// JP2 decoder state flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Jp2State {
    /// Initial state.
    None,
    /// JP signature box read.
    Signature,
    /// File type box read.
    FileType,
    /// JP2 header box read.
    Header,
    /// Codestream box found.
    Codestream,
}

// ---------------------------------------------------------------------------
// JP2 colour info (C: opj_jp2_color_t subset)
// ---------------------------------------------------------------------------

/// Colour specification method (COLR box METH field).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColourMethod {
    /// Enumerated colour space (METH=1).
    Enumerated,
    /// ICC profile (METH=2).
    Icc,
}

/// Colour information parsed from the COLR box.
#[derive(Debug, Clone)]
pub struct Jp2Colour {
    /// Colour method.
    pub meth: ColourMethod,
    /// Precedence.
    pub precedence: u8,
    /// Approximation accuracy.
    pub approx: u8,
    /// Enumerated colour space (valid when meth == Enumerated).
    pub enumcs: u32,
    /// ICC profile data (valid when meth == Icc).
    pub icc_profile: Vec<u8>,
}

impl Default for Jp2Colour {
    fn default() -> Self {
        Self {
            meth: ColourMethod::Enumerated,
            precedence: 0,
            approx: 0,
            enumcs: 0,
            icc_profile: Vec::new(),
        }
    }
}

/// Per-component info from IHDR/BPCC (C: opj_jp2_comps_t).
#[derive(Debug, Clone, Default)]
pub struct Jp2CompInfo {
    /// Bit precision (decoded from raw BPC/BPCC: (raw & 0x7F) + 1).
    pub prec: u8,
    /// Whether the component is signed (bit 7 of raw BPC/BPCC).
    pub sgnd: bool,
}

// ---------------------------------------------------------------------------
// CDEF (Channel Definition) box types (C: opj_jp2_cdef_info_t)
// ---------------------------------------------------------------------------

/// Channel definition entry from the CDEF box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CdefEntry {
    /// Channel index.
    pub cn: u16,
    /// Channel type (0 = colour, 1 = opacity, 2 = premultiplied opacity).
    pub typ: u16,
    /// Colour component association (1-based; 0 or 65535 = no association).
    pub asoc: u16,
}

// ---------------------------------------------------------------------------
// PCLR (Palette) box types (C: opj_jp2_pclr_t)
// ---------------------------------------------------------------------------

/// Palette data from the PCLR box.
#[derive(Debug, Clone)]
pub struct Pclr {
    /// Palette entries: `entries[entry_idx * nr_channels + col]`.
    pub entries: Vec<u32>,
    /// Per-column signedness.
    pub channel_sign: Vec<bool>,
    /// Per-column bit depth (decoded: actual precision).
    pub channel_size: Vec<u8>,
    /// Number of palette entries.
    pub nr_entries: u16,
    /// Number of palette columns.
    pub nr_channels: u8,
}

// ---------------------------------------------------------------------------
// CMAP (Component Mapping) box types (C: opj_jp2_cmap_comp_t)
// ---------------------------------------------------------------------------

/// Component mapping entry from the CMAP box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CmapEntry {
    /// Source component index in the decoded J2K image.
    pub cmp: u16,
    /// Mapping type (0 = direct use, 1 = palette mapping).
    pub mtyp: u8,
    /// Palette column index.
    pub pcol: u8,
}

/// JP2 box header (C: opj_jp2_box_t).
#[derive(Debug, Clone, Copy)]
pub struct Jp2Box {
    /// Box length (including header).
    pub length: u32,
    /// Box type (4-byte code).
    pub box_type: u32,
    /// Header length (8 for normal, 16 for extended-length boxes).
    pub header_len: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_type_constants() {
        assert_eq!(JP2_JP, 0x6A50_2020);
        assert_eq!(JP2_FTYP, 0x6674_7970);
        assert_eq!(JP2_JP2H, 0x6A70_3268);
        assert_eq!(JP2_IHDR, 0x6968_6472);
        assert_eq!(JP2_COLR, 0x636F_6C72);
        assert_eq!(JP2_JP2C, 0x6A70_3263);
        assert_eq!(JP2_PCLR, 0x7063_6C72);
        assert_eq!(JP2_CMAP, 0x636D_6170);
        assert_eq!(JP2_CDEF, 0x6364_6566);
        assert_eq!(JP2_BPCC, 0x6270_6363);
        assert_eq!(JP2_JP2_BRAND, 0x6A70_3220);
        assert_eq!(JP2_MAGIC, 0x0D0A_870A);
    }

    #[test]
    fn jp2_state_variants() {
        let s = Jp2State::None;
        assert_eq!(s, Jp2State::None);
        let _ = Jp2State::Signature;
        let _ = Jp2State::FileType;
        let _ = Jp2State::Header;
        let _ = Jp2State::Codestream;
    }

    #[test]
    fn jp2_colour_default() {
        let c = Jp2Colour::default();
        assert_eq!(c.meth, ColourMethod::Enumerated);
        assert_eq!(c.precedence, 0);
        assert_eq!(c.approx, 0);
        assert_eq!(c.enumcs, 0);
        assert!(c.icc_profile.is_empty());
    }

    #[test]
    fn jp2_box_struct() {
        let b = Jp2Box {
            length: 22,
            box_type: JP2_IHDR,
            header_len: 8,
        };
        assert_eq!(b.length, 22);
        assert_eq!(b.box_type, JP2_IHDR);
        assert_eq!(b.header_len, 8);
    }
}
