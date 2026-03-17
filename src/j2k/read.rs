// Phase 500c: J2K decoder
//
// Reads a J2K codestream: parses main header, tile-part headers, and
// orchestrates tile decoding via TCD.

use crate::error::{Error, Result};
use crate::image::Image;
use crate::io::cio::{MemoryStream, read_bytes_be};
use crate::j2k::markers::{read_cod, read_com, read_qcd, read_siz, read_sot};
use crate::j2k::params::{CodingParameters, TileCodingParameters};
use crate::j2k::{J2kState, Marker};
use crate::types::ColorSpace;

/// J2K codestream decoder.
pub struct J2kDecoder {
    /// Current decoder state.
    pub state: J2kState,
    /// Coding parameters (populated from SIZ, COD, QCD).
    pub cp: CodingParameters,
    /// Default tile coding parameters (from main header).
    pub default_tcp: TileCodingParameters,
    /// Decoded image.
    pub image: Image,
    /// Current tile number being processed.
    pub current_tile_no: u32,
    /// Remaining tile-part data length after SOT header.
    pub sot_length: u32,
    /// Per-tile compressed data buffers.
    pub tile_data: Vec<Vec<u8>>,
    /// Whether main header required markers have been seen.
    siz_found: bool,
    cod_found: bool,
    qcd_found: bool,
}

impl Default for J2kDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl J2kDecoder {
    /// Create a new J2K decoder.
    pub fn new() -> Self {
        Self {
            state: J2kState::None,
            cp: CodingParameters::new_decoder(),
            default_tcp: TileCodingParameters::default(),
            image: Image::new_tile(&[], ColorSpace::Unknown),
            current_tile_no: 0,
            sot_length: 0,
            tile_data: Vec::new(),
            siz_found: false,
            cod_found: false,
            qcd_found: false,
        }
    }

    /// Read the main header from a J2K codestream.
    ///
    /// Parses SOC, SIZ, COD, QCD and optional markers until SOT is reached.
    /// After this call, `self.image` and `self.cp` are populated.
    pub fn read_header(&mut self, stream: &mut MemoryStream) -> Result<()> {
        self.state = J2kState::MhSoc;

        // Read SOC marker
        let mut marker_buf = [0u8; 2];
        if stream.read(&mut marker_buf)? < 2 {
            return Err(Error::EndOfStream);
        }
        let marker_val = read_bytes_be(&marker_buf, 2) as u16;
        if Marker::from_u16(marker_val) != Marker::Soc {
            return Err(Error::InvalidInput("Expected SOC marker".into()));
        }
        self.state = J2kState::MhSiz;

        // Main header marker loop
        loop {
            // Read marker ID
            if stream.read(&mut marker_buf)? < 2 {
                return Err(Error::EndOfStream);
            }
            let marker_val = read_bytes_be(&marker_buf, 2) as u16;
            let marker = Marker::from_u16(marker_val);

            // SOT signals end of main header
            if marker == Marker::Sot {
                // Validate required markers
                if !self.siz_found {
                    return Err(Error::InvalidInput("Missing SIZ marker".into()));
                }
                if !self.cod_found {
                    return Err(Error::InvalidInput("Missing COD marker".into()));
                }
                if !self.qcd_found {
                    return Err(Error::InvalidInput("Missing QCD marker".into()));
                }

                // Copy default TCP to per-tile TCPs
                self.copy_default_tcp_to_tiles();
                self.state = J2kState::TphSot;

                // Push back the SOT marker for tile header reading
                stream.skip(-2)?;
                return Ok(());
            }

            // EOC or end of stream
            if marker == Marker::Eoc {
                self.state = J2kState::Eoc;
                return Ok(());
            }

            // Read marker segment length
            let mut len_buf = [0u8; 2];
            if stream.read(&mut len_buf)? < 2 {
                return Err(Error::EndOfStream);
            }
            let seg_len = read_bytes_be(&len_buf, 2) as usize;
            if seg_len < 2 {
                return Err(Error::InvalidInput(format!(
                    "Invalid marker segment length: {seg_len}"
                )));
            }
            let payload_len = seg_len - 2;

            // Read marker payload
            let mut payload = vec![0u8; payload_len];
            if payload_len > 0 && stream.read(&mut payload)? < payload_len {
                return Err(Error::EndOfStream);
            }

            // Dispatch marker
            match marker {
                Marker::Siz => {
                    read_siz(&payload, &mut self.image, &mut self.cp)?;
                    self.siz_found = true;
                    self.state = J2kState::Mh;
                }
                Marker::Cod => {
                    let numcomps = self.image.comps.len() as u32;
                    read_cod(&payload, &mut self.default_tcp, numcomps)?;
                    self.cod_found = true;
                }
                Marker::Qcd => {
                    let numcomps = self.image.comps.len() as u32;
                    read_qcd(&payload, &mut self.default_tcp, numcomps)?;
                    self.qcd_found = true;
                }
                Marker::Com => {
                    let _ = read_com(&payload)?;
                }
                _ => {
                    // Skip unknown markers
                }
            }
        }
    }

    /// Read a tile-part header (SOT + optional markers + SOD) and store tile data.
    ///
    /// Returns the tile number, or `None` if EOC/end of stream.
    pub fn read_tile_part(&mut self, stream: &mut MemoryStream) -> Result<Option<u32>> {
        // Read SOT marker
        let mut marker_buf = [0u8; 2];
        if stream.read(&mut marker_buf)? < 2 {
            self.state = J2kState::Neoc;
            return Ok(None);
        }
        let marker_val = read_bytes_be(&marker_buf, 2) as u16;
        let marker = Marker::from_u16(marker_val);

        if marker == Marker::Eoc {
            self.state = J2kState::Eoc;
            return Ok(None);
        }
        if marker != Marker::Sot {
            return Err(Error::InvalidInput(format!(
                "Expected SOT or EOC, got 0x{marker_val:04X}"
            )));
        }

        // Read SOT length (always 10)
        let mut len_buf = [0u8; 2];
        if stream.read(&mut len_buf)? < 2 {
            return Err(Error::EndOfStream);
        }
        let sot_seg_len = read_bytes_be(&len_buf, 2) as usize;
        if sot_seg_len != 10 {
            return Err(Error::InvalidInput(format!(
                "SOT segment length must be 10, got {sot_seg_len}"
            )));
        }

        // Read SOT payload (8 bytes)
        let mut sot_data = [0u8; 8];
        if stream.read(&mut sot_data)? < 8 {
            return Err(Error::EndOfStream);
        }
        let sot = read_sot(&sot_data)?;
        let total_tiles = self.cp.tw * self.cp.th;
        if sot.tile_no >= total_tiles {
            return Err(Error::InvalidInput(format!(
                "SOT tile_no {} >= total tiles {}",
                sot.tile_no, total_tiles
            )));
        }

        self.current_tile_no = sot.tile_no;
        self.state = J2kState::Tph;

        // Compute tile data length
        // Psot includes the entire SOT segment (12 bytes) + tile header markers + SOD + tile data
        // After reading SOT (12 bytes), remaining = Psot - 12
        let remaining_in_tile_part = if sot.psot == 0 {
            // Last tile-part: read until EOC or end of stream
            stream.bytes_left()
        } else {
            (sot.psot as usize).saturating_sub(12)
        };

        // Skip tile-part header markers until SOD
        let mut header_consumed = 0usize;
        loop {
            if header_consumed + 2 > remaining_in_tile_part {
                return Err(Error::InvalidInput("Tile-part too short for SOD".into()));
            }
            let mut mbuf = [0u8; 2];
            if stream.read(&mut mbuf)? < 2 {
                return Err(Error::EndOfStream);
            }
            header_consumed += 2;
            let m = read_bytes_be(&mbuf, 2) as u16;

            if Marker::from_u16(m) == Marker::Sod {
                self.state = J2kState::Data;
                break;
            }

            // Read and skip this tile-part header marker
            let mut lbuf = [0u8; 2];
            if stream.read(&mut lbuf)? < 2 {
                return Err(Error::EndOfStream);
            }
            header_consumed += 2;
            let mlen = read_bytes_be(&lbuf, 2) as usize;
            if mlen < 2 {
                return Err(Error::InvalidInput("Invalid tile marker length".into()));
            }
            let skip = mlen - 2;
            stream.skip(skip as i64)?;
            header_consumed += skip;
        }

        // Read tile data (compressed packets)
        let data_len = remaining_in_tile_part - header_consumed;
        let mut tile_bytes = vec![0u8; data_len];
        if data_len > 0 {
            let read = stream.read(&mut tile_bytes)?;
            tile_bytes.truncate(read);
        }

        // Store/append tile data
        let tile_idx = sot.tile_no as usize;
        if self.tile_data.len() <= tile_idx {
            self.tile_data.resize(tile_idx + 1, Vec::new());
        }
        self.tile_data[tile_idx].extend_from_slice(&tile_bytes);

        self.state = J2kState::TphSot;
        Ok(Some(sot.tile_no))
    }

    /// Read all tile-parts from the stream.
    pub fn read_all_tiles(&mut self, stream: &mut MemoryStream) -> Result<()> {
        while self.read_tile_part(stream)?.is_some() {}
        Ok(())
    }

    /// Copy default TCP to all per-tile TCPs.
    fn copy_default_tcp_to_tiles(&mut self) {
        let num_tiles = (self.cp.tw * self.cp.th) as usize;
        self.cp.tcps.clear();
        for _ in 0..num_tiles {
            self.cp.tcps.push(self.default_tcp.clone());
        }
    }

    /// Number of tiles in the image.
    pub fn num_tiles(&self) -> u32 {
        self.cp.tw * self.cp.th
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal J2K codestream:
    /// SOC + SIZ(1x1 grayscale) + COD + QCD + SOT + SOD + [empty tile data] + EOC
    fn build_minimal_j2k() -> Vec<u8> {
        let mut buf = Vec::new();

        // SOC
        buf.extend_from_slice(&[0xFF, 0x4F]);

        // SIZ marker
        let mut siz_payload = Vec::new();
        siz_payload.extend_from_slice(&0u16.to_be_bytes()); // Rsiz
        siz_payload.extend_from_slice(&1u32.to_be_bytes()); // Xsiz=1
        siz_payload.extend_from_slice(&1u32.to_be_bytes()); // Ysiz=1
        siz_payload.extend_from_slice(&0u32.to_be_bytes()); // X0siz=0
        siz_payload.extend_from_slice(&0u32.to_be_bytes()); // Y0siz=0
        siz_payload.extend_from_slice(&1u32.to_be_bytes()); // XTsiz=1
        siz_payload.extend_from_slice(&1u32.to_be_bytes()); // YTsiz=1
        siz_payload.extend_from_slice(&0u32.to_be_bytes()); // XT0siz=0
        siz_payload.extend_from_slice(&0u32.to_be_bytes()); // YT0siz=0
        siz_payload.extend_from_slice(&1u16.to_be_bytes()); // Csiz=1
        siz_payload.push(0x07); // Ssiz: 8-bit unsigned
        siz_payload.push(0x01); // XRsiz=1
        siz_payload.push(0x01); // YRsiz=1
        let siz_len = (siz_payload.len() + 2) as u16;
        buf.extend_from_slice(&[0xFF, 0x51]);
        buf.extend_from_slice(&siz_len.to_be_bytes());
        buf.extend_from_slice(&siz_payload);

        // COD marker
        let mut cod_payload = Vec::new();
        cod_payload.push(0x00); // Scod
        cod_payload.push(0x00); // LRCP
        cod_payload.extend_from_slice(&1u16.to_be_bytes()); // 1 layer
        cod_payload.push(0x00); // no MCT
        cod_payload.push(0x00); // 0 decomp → 1 resolution
        cod_payload.push(0x04); // cblkw
        cod_payload.push(0x04); // cblkh
        cod_payload.push(0x00); // cblksty
        cod_payload.push(0x01); // 5-3 reversible
        let cod_len = (cod_payload.len() + 2) as u16;
        buf.extend_from_slice(&[0xFF, 0x52]);
        buf.extend_from_slice(&cod_len.to_be_bytes());
        buf.extend_from_slice(&cod_payload);

        // QCD marker
        let qcd_payload = vec![0x40, 0x40]; // NOQNT numgbits=2, band0 expn=8
        let qcd_len = (qcd_payload.len() + 2) as u16;
        buf.extend_from_slice(&[0xFF, 0x5C]);
        buf.extend_from_slice(&qcd_len.to_be_bytes());
        buf.extend_from_slice(&qcd_payload);

        // SOT marker (tile 0)
        let tile_data = vec![0u8; 4]; // fake tile data
        let psot = 12 + 2 + tile_data.len() as u32; // SOT(12) + SOD(2) + data
        buf.extend_from_slice(&[0xFF, 0x90]); // SOT marker
        buf.extend_from_slice(&10u16.to_be_bytes()); // Lsot=10
        buf.extend_from_slice(&0u16.to_be_bytes()); // Isot=0
        buf.extend_from_slice(&psot.to_be_bytes()); // Psot
        buf.push(0x00); // TPsot=0
        buf.push(0x01); // TNsot=1

        // SOD marker
        buf.extend_from_slice(&[0xFF, 0x93]);
        buf.extend_from_slice(&tile_data);

        // EOC
        buf.extend_from_slice(&[0xFF, 0xD9]);

        buf
    }

    #[test]
    fn decode_minimal_header() {
        let data = build_minimal_j2k();
        let mut stream = MemoryStream::new_input(data);
        let mut dec = J2kDecoder::new();

        dec.read_header(&mut stream).unwrap();

        assert_eq!(dec.image.x1, 1);
        assert_eq!(dec.image.y1, 1);
        assert_eq!(dec.image.comps.len(), 1);
        assert_eq!(dec.image.comps[0].prec, 8);
        assert_eq!(dec.cp.tw, 1);
        assert_eq!(dec.cp.th, 1);
        assert_eq!(dec.cp.tcps.len(), 1);
        assert_eq!(dec.state, J2kState::TphSot);
    }

    #[test]
    fn decode_minimal_tile_part() {
        let data = build_minimal_j2k();
        let mut stream = MemoryStream::new_input(data);
        let mut dec = J2kDecoder::new();

        dec.read_header(&mut stream).unwrap();
        let tile_no = dec.read_tile_part(&mut stream).unwrap();

        assert_eq!(tile_no, Some(0));
        assert_eq!(dec.tile_data.len(), 1);
        assert_eq!(dec.tile_data[0].len(), 4); // 4 bytes of fake tile data
    }

    #[test]
    fn decode_all_tiles_then_eoc() {
        let data = build_minimal_j2k();
        let mut stream = MemoryStream::new_input(data);
        let mut dec = J2kDecoder::new();

        dec.read_header(&mut stream).unwrap();
        dec.read_all_tiles(&mut stream).unwrap();

        assert_eq!(dec.tile_data.len(), 1);
        assert!(matches!(dec.state, J2kState::Eoc));
    }

    #[test]
    fn decode_missing_siz_fails() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0xFF, 0x4F]); // SOC
        // Skip directly to SOT without SIZ
        buf.extend_from_slice(&[0xFF, 0x90]); // SOT

        let mut stream = MemoryStream::new_input(buf);
        let mut dec = J2kDecoder::new();
        assert!(dec.read_header(&mut stream).is_err());
    }

    #[test]
    fn sot_marker_parsing() {
        let data = vec![
            0x00, 0x02, // Isot = 2
            0x00, 0x00, 0x00, 0x20, // Psot = 32
            0x01, // TPsot = 1
            0x03, // TNsot = 3
        ];
        let sot = read_sot(&data).unwrap();
        assert_eq!(sot.tile_no, 2);
        assert_eq!(sot.psot, 32);
        assert_eq!(sot.tp_idx, 1);
        assert_eq!(sot.nb_parts, 3);
    }
}
