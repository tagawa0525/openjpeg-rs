// Phase 500: J2K codestream module
//
// Marker constants, decoder/encoder state, and sub-module declarations.

pub mod params;

// ---------------------------------------------------------------------------
// Marker constants (C: J2K_MS_*)
// ---------------------------------------------------------------------------

/// JPEG 2000 marker IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Marker {
    /// Start of codestream.
    Soc = 0xFF4F,
    /// Start of tile-part.
    Sot = 0xFF90,
    /// Start of data.
    Sod = 0xFF93,
    /// End of codestream.
    Eoc = 0xFFD9,
    /// Image and tile size.
    Siz = 0xFF51,
    /// Coding style default.
    Cod = 0xFF52,
    /// Coding style component.
    Coc = 0xFF53,
    /// Region of interest.
    Rgn = 0xFF5E,
    /// Quantization default.
    Qcd = 0xFF5C,
    /// Quantization component.
    Qcc = 0xFF5D,
    /// Progression order change.
    Poc = 0xFF5F,
    /// Tile-part lengths.
    Tlm = 0xFF55,
    /// Packet lengths (main header).
    Plm = 0xFF57,
    /// Packet lengths (tile-part header).
    Plt = 0xFF58,
    /// Packed packet headers (main).
    Ppm = 0xFF60,
    /// Packed packet headers (tile-part).
    Ppt = 0xFF61,
    /// Component registration.
    Crg = 0xFF63,
    /// Comment.
    Com = 0xFF64,
    /// Component bit depth (Part-2).
    Cbd = 0xFF78,
    /// Extended capabilities (Part-2).
    Cap = 0xFF50,
    /// Unknown marker.
    Unk = 0x0000,
}

impl Marker {
    /// Convert a u16 value to a Marker. Unknown values map to `Marker::Unk`.
    pub fn from_u16(val: u16) -> Self {
        match val {
            0xFF4F => Marker::Soc,
            0xFF90 => Marker::Sot,
            0xFF93 => Marker::Sod,
            0xFFD9 => Marker::Eoc,
            0xFF51 => Marker::Siz,
            0xFF52 => Marker::Cod,
            0xFF53 => Marker::Coc,
            0xFF5E => Marker::Rgn,
            0xFF5C => Marker::Qcd,
            0xFF5D => Marker::Qcc,
            0xFF5F => Marker::Poc,
            0xFF55 => Marker::Tlm,
            0xFF57 => Marker::Plm,
            0xFF58 => Marker::Plt,
            0xFF60 => Marker::Ppm,
            0xFF61 => Marker::Ppt,
            0xFF63 => Marker::Crg,
            0xFF64 => Marker::Com,
            0xFF78 => Marker::Cbd,
            0xFF50 => Marker::Cap,
            _ => Marker::Unk,
        }
    }
}

// ---------------------------------------------------------------------------
// Decoder state machine (C: J2K_STATUS)
// ---------------------------------------------------------------------------

/// J2K decoder state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum J2kState {
    /// Initial state.
    None,
    /// Expecting SOC.
    MhSoc,
    /// SOC read, expecting SIZ.
    MhSiz,
    /// Reading main header markers.
    Mh,
    /// Expecting SOT.
    TphSot,
    /// Reading tile-part header markers.
    Tph,
    /// Reading tile data.
    Data,
    /// End of codestream reached.
    Eoc,
    /// Truncated codestream (no explicit EOC).
    Neoc,
    /// Error state.
    Err,
}

impl J2kState {
    /// Returns `true` if in a main header state (MhSoc, MhSiz, Mh).
    pub fn is_main_header(self) -> bool {
        matches!(self, J2kState::MhSoc | J2kState::MhSiz | J2kState::Mh)
    }

    /// Returns `true` if in a tile-part header state (TphSot, Tph).
    pub fn is_tile_header(self) -> bool {
        matches!(self, J2kState::TphSot | J2kState::Tph)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_from_u16_known() {
        assert_eq!(Marker::from_u16(0xFF4F), Marker::Soc);
        assert_eq!(Marker::from_u16(0xFF51), Marker::Siz);
        assert_eq!(Marker::from_u16(0xFF52), Marker::Cod);
        assert_eq!(Marker::from_u16(0xFF5C), Marker::Qcd);
        assert_eq!(Marker::from_u16(0xFF90), Marker::Sot);
        assert_eq!(Marker::from_u16(0xFF93), Marker::Sod);
        assert_eq!(Marker::from_u16(0xFFD9), Marker::Eoc);
        assert_eq!(Marker::from_u16(0xFF64), Marker::Com);
    }

    #[test]
    fn marker_from_u16_unknown() {
        assert_eq!(Marker::from_u16(0xFF00), Marker::Unk);
        assert_eq!(Marker::from_u16(0x0000), Marker::Unk);
    }

    #[test]
    fn marker_as_u16() {
        assert_eq!(Marker::Soc as u16, 0xFF4F);
        assert_eq!(Marker::Eoc as u16, 0xFFD9);
    }

    #[test]
    fn j2k_state_main_header() {
        assert!(J2kState::MhSoc.is_main_header());
        assert!(J2kState::MhSiz.is_main_header());
        assert!(J2kState::Mh.is_main_header());
        assert!(!J2kState::TphSot.is_main_header());
        assert!(!J2kState::Data.is_main_header());
    }

    #[test]
    fn j2k_state_tile_header() {
        assert!(J2kState::TphSot.is_tile_header());
        assert!(J2kState::Tph.is_tile_header());
        assert!(!J2kState::Mh.is_tile_header());
        assert!(!J2kState::Data.is_tile_header());
    }
}
