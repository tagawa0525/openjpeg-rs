// Phase 600a: JP2 decoder
//
// Reads a JP2 file: parses boxes (JP, FTYP, JP2H, IHDR, COLR, BPCC, JP2C),
// then delegates J2K codestream decoding to J2kDecoder.

use crate::error::{Error, Result};
use crate::io::cio::{MemoryStream, read_bytes_be};
use crate::j2k::read::J2kDecoder;
use crate::jp2::{
    ColourMethod, JP2_BPCC, JP2_COLR, JP2_FTYP, JP2_IHDR, JP2_JP, JP2_JP2_BRAND, JP2_JP2C,
    JP2_JP2H, JP2_MAGIC, Jp2Box, Jp2Colour, Jp2CompInfo, Jp2State,
};
use crate::types::ColorSpace;

/// JP2 file decoder.
pub struct Jp2Decoder {
    /// Current decoder state.
    pub state: Jp2State,
    /// Embedded J2K decoder.
    pub j2k: J2kDecoder,
    /// Image width (from IHDR).
    pub width: u32,
    /// Image height (from IHDR).
    pub height: u32,
    /// Number of components (from IHDR).
    pub numcomps: u32,
    /// Bits per component (from IHDR; 255 = varies per component).
    pub bpc: u8,
    /// Compression type (from IHDR; should be 7).
    pub compression_type: u8,
    /// Colour specification.
    pub colour: Jp2Colour,
    /// Per-component bit depth info.
    pub comp_info: Vec<Jp2CompInfo>,
    /// Whether IHDR was found.
    ihdr_found: bool,
    /// Whether COLR was found.
    colr_found: bool,
}

impl Default for Jp2Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Jp2Decoder {
    /// Create a new JP2 decoder.
    pub fn new() -> Self {
        Self {
            state: Jp2State::None,
            j2k: J2kDecoder::new(),
            width: 0,
            height: 0,
            numcomps: 0,
            bpc: 0,
            compression_type: 0,
            colour: Jp2Colour::default(),
            comp_info: Vec::new(),
            ihdr_found: false,
            colr_found: false,
        }
    }

    /// Read JP2 file header (all boxes up to JP2C).
    ///
    /// After this call, the stream is positioned at the start of the J2K
    /// codestream inside the JP2C box. Call `read_codestream()` next.
    pub fn read_header(&mut self, stream: &mut MemoryStream) -> Result<()> {
        loop {
            let box_hdr = read_box_header(stream)?;

            // JP2C box: codestream found
            if box_hdr.box_type == JP2_JP2C {
                if self.state != Jp2State::Header {
                    return Err(Error::InvalidInput(
                        "JP2C box found before JP2 header".into(),
                    ));
                }
                self.state = Jp2State::Codestream;
                // Stream is now at start of J2K codestream
                return Ok(());
            }

            // Read box payload
            let payload_len = box_hdr.length.saturating_sub(8) as usize;
            if payload_len > stream.bytes_left() {
                return Err(Error::InvalidInput(format!(
                    "Box payload {} exceeds available data",
                    payload_len
                )));
            }
            let mut payload = vec![0u8; payload_len];
            if payload_len > 0 && stream.read(&mut payload)? < payload_len {
                return Err(Error::EndOfStream);
            }

            match box_hdr.box_type {
                JP2_JP => self.read_jp(&payload)?,
                JP2_FTYP => self.read_ftyp(&payload)?,
                JP2_JP2H => self.read_jp2h(&payload)?,
                _ => {
                    // Skip unknown boxes
                }
            }
        }
    }

    /// Read the J2K codestream from inside the JP2C box.
    ///
    /// Must be called after `read_header()`.
    pub fn read_codestream(&mut self, stream: &mut MemoryStream) -> Result<()> {
        if self.state != Jp2State::Codestream {
            return Err(Error::InvalidInput(
                "Must call read_header() before read_codestream()".into(),
            ));
        }

        self.j2k.read_header(stream)?;

        // Apply JP2 colour info to the image
        self.apply_colour();

        self.j2k.read_all_tiles(stream)?;
        Ok(())
    }

    /// Read JP signature box payload.
    fn read_jp(&mut self, data: &[u8]) -> Result<()> {
        if self.state != Jp2State::None {
            return Err(Error::InvalidInput(
                "JP signature box must be first box".into(),
            ));
        }
        if data.len() != 4 {
            return Err(Error::InvalidInput(format!(
                "JP signature box payload must be 4 bytes, got {}",
                data.len()
            )));
        }
        let magic = read_bytes_be(data, 4);
        if magic != JP2_MAGIC {
            return Err(Error::InvalidInput(format!(
                "Bad JP2 magic number: 0x{magic:08X}"
            )));
        }
        self.state = Jp2State::Signature;
        Ok(())
    }

    /// Read FTYP (File Type) box payload.
    fn read_ftyp(&mut self, data: &[u8]) -> Result<()> {
        if self.state != Jp2State::Signature {
            return Err(Error::InvalidInput(
                "FTYP box must follow JP signature box".into(),
            ));
        }
        if data.len() < 8 {
            return Err(Error::InvalidInput(format!(
                "FTYP box too short: {} bytes",
                data.len()
            )));
        }
        let brand = read_bytes_be(data, 4);
        if brand != JP2_JP2_BRAND {
            return Err(Error::InvalidInput(format!(
                "Unsupported JP2 brand: 0x{brand:08X}"
            )));
        }
        // MinV (4 bytes) + compatibility list (4 bytes each) — skip for now
        self.state = Jp2State::FileType;
        Ok(())
    }

    /// Read JP2H (JP2 Header) super-box payload.
    fn read_jp2h(&mut self, data: &[u8]) -> Result<()> {
        if self.state != Jp2State::FileType {
            return Err(Error::InvalidInput("JP2H box must follow FTYP box".into()));
        }

        let mut offset = 0usize;
        let mut has_ihdr = false;

        while offset < data.len() {
            if offset + 8 > data.len() {
                return Err(Error::InvalidInput("JP2H: truncated sub-box header".into()));
            }
            let sub_len = read_bytes_be(&data[offset..], 4);
            let sub_type = read_bytes_be(&data[offset + 4..], 4);

            if sub_len < 8 || (offset + sub_len as usize) > data.len() {
                return Err(Error::InvalidInput(format!(
                    "JP2H: invalid sub-box length {sub_len}"
                )));
            }

            let sub_payload = &data[offset + 8..offset + sub_len as usize];

            match sub_type {
                JP2_IHDR => {
                    self.read_ihdr(sub_payload)?;
                    has_ihdr = true;
                }
                JP2_COLR => {
                    self.read_colr(sub_payload)?;
                }
                JP2_BPCC => {
                    self.read_bpcc(sub_payload)?;
                }
                _ => {
                    // Skip unknown sub-boxes (CDEF, CMAP, PCLR handled in 600b)
                }
            }

            offset += sub_len as usize;
        }

        if !has_ihdr {
            return Err(Error::InvalidInput("JP2H: missing IHDR box".into()));
        }

        self.state = Jp2State::Header;
        Ok(())
    }

    /// Read IHDR (Image Header) box payload.
    fn read_ihdr(&mut self, data: &[u8]) -> Result<()> {
        if self.ihdr_found {
            // Per spec, ignore duplicate IHDR boxes
            return Ok(());
        }
        if data.len() != 14 {
            return Err(Error::InvalidInput(format!(
                "IHDR box must be 14 bytes, got {}",
                data.len()
            )));
        }

        self.height = read_bytes_be(data, 4);
        self.width = read_bytes_be(&data[4..], 4);
        self.numcomps = read_bytes_be(&data[8..], 2);
        self.bpc = data[10];
        self.compression_type = data[11];
        // data[12] = UnkC, data[13] = IPR — not needed for basic decoding

        if self.height == 0 || self.width == 0 || self.numcomps == 0 {
            return Err(Error::InvalidInput(format!(
                "IHDR: invalid dimensions {}x{} with {} components",
                self.width, self.height, self.numcomps
            )));
        }
        if self.numcomps > 16384 {
            return Err(Error::InvalidInput(format!(
                "IHDR: too many components: {}",
                self.numcomps
            )));
        }

        // Initialize per-component info
        self.comp_info = vec![Jp2CompInfo::default(); self.numcomps as usize];
        if self.bpc != 255 {
            // Uniform BPC
            for ci in &mut self.comp_info {
                ci.bpcc = self.bpc;
            }
        }

        self.ihdr_found = true;
        Ok(())
    }

    /// Read COLR (Colour Specification) box payload.
    fn read_colr(&mut self, data: &[u8]) -> Result<()> {
        // Per spec, ignore duplicate COLR boxes
        if self.colr_found {
            return Ok(());
        }
        if data.len() < 3 {
            return Err(Error::InvalidInput("COLR box too short".into()));
        }

        let meth = data[0];
        self.colour.precedence = data[1];
        self.colour.approx = data[2];

        match meth {
            1 => {
                // Enumerated colour space
                if data.len() < 7 {
                    return Err(Error::InvalidInput(
                        "COLR box too short for enumerated colour space".into(),
                    ));
                }
                self.colour.meth = ColourMethod::Enumerated;
                self.colour.enumcs = read_bytes_be(&data[3..], 4);
            }
            2 => {
                // ICC profile
                self.colour.meth = ColourMethod::Icc;
                self.colour.icc_profile = data[3..].to_vec();
            }
            _ => {
                return Err(Error::InvalidInput(format!(
                    "COLR: unsupported method {meth}"
                )));
            }
        }

        self.colr_found = true;
        Ok(())
    }

    /// Read BPCC (Bits Per Component) box payload.
    fn read_bpcc(&mut self, data: &[u8]) -> Result<()> {
        if !self.ihdr_found {
            return Err(Error::InvalidInput("BPCC box found before IHDR".into()));
        }
        if data.len() != self.numcomps as usize {
            return Err(Error::InvalidInput(format!(
                "BPCC: expected {} bytes, got {}",
                self.numcomps,
                data.len()
            )));
        }
        for (i, &b) in data.iter().enumerate() {
            self.comp_info[i].bpcc = b;
        }
        Ok(())
    }

    /// Map JP2 EnumCS to ColorSpace and apply to the J2K image.
    fn apply_colour(&mut self) {
        let cs = match self.colour.meth {
            ColourMethod::Enumerated => match self.colour.enumcs {
                16 => ColorSpace::Srgb,
                17 => ColorSpace::Gray,
                18 => ColorSpace::Sycc,
                24 => ColorSpace::Eycc,
                _ => ColorSpace::Unknown,
            },
            ColourMethod::Icc => {
                self.j2k.image.icc_profile = self.colour.icc_profile.clone();
                ColorSpace::Unknown
            }
        };
        self.j2k.image.color_space = cs;
    }
}

/// Read a JP2 box header (8 bytes: length + type) from the stream.
fn read_box_header(stream: &mut MemoryStream) -> Result<Jp2Box> {
    let mut buf = [0u8; 8];
    if stream.read(&mut buf)? < 8 {
        return Err(Error::EndOfStream);
    }
    let length = read_bytes_be(&buf[0..], 4);
    let box_type = read_bytes_be(&buf[4..], 4);

    if length == 0 {
        // Last box: extends to end of stream
        let remaining = stream.bytes_left() as u32;
        return Ok(Jp2Box {
            length: remaining + 8,
            box_type,
        });
    }

    if length == 1 {
        // Extended length (8 bytes) — we only support up to 4GB
        let mut xlbuf = [0u8; 8];
        if stream.read(&mut xlbuf)? < 8 {
            return Err(Error::EndOfStream);
        }
        let xl_high = read_bytes_be(&xlbuf[0..], 4);
        if xl_high != 0 {
            return Err(Error::InvalidInput(
                "Box size exceeds 4GB, not supported".into(),
            ));
        }
        let xl_low = read_bytes_be(&xlbuf[4..], 4);
        return Ok(Jp2Box {
            length: xl_low,
            box_type,
        });
    }

    Ok(Jp2Box { length, box_type })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::cio::MemoryStream;

    // -----------------------------------------------------------------------
    // Helper: build a minimal valid JP2 file
    // -----------------------------------------------------------------------

    /// Build a JP2 signature box (12 bytes).
    fn build_jp_box() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&12u32.to_be_bytes()); // length
        b.extend_from_slice(&JP2_JP.to_be_bytes()); // type
        b.extend_from_slice(&JP2_MAGIC.to_be_bytes()); // magic
        b
    }

    /// Build a FTYP box (20 bytes).
    fn build_ftyp_box() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&20u32.to_be_bytes()); // length
        b.extend_from_slice(&JP2_FTYP.to_be_bytes()); // type
        b.extend_from_slice(&JP2_JP2_BRAND.to_be_bytes()); // brand
        b.extend_from_slice(&0u32.to_be_bytes()); // minversion
        b.extend_from_slice(&JP2_JP2_BRAND.to_be_bytes()); // CL[0]
        b
    }

    /// Build an IHDR sub-box (22 bytes).
    fn build_ihdr_box(w: u32, h: u32, nc: u16, bpc: u8) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&22u32.to_be_bytes()); // length
        b.extend_from_slice(&JP2_IHDR.to_be_bytes()); // type
        b.extend_from_slice(&h.to_be_bytes()); // HEIGHT
        b.extend_from_slice(&w.to_be_bytes()); // WIDTH
        b.extend_from_slice(&nc.to_be_bytes()); // NC
        b.push(bpc); // BPC
        b.push(7); // C = 7 (JPEG 2000)
        b.push(0); // UnkC
        b.push(0); // IPR
        b
    }

    /// Build a COLR sub-box with enumerated colour space.
    fn build_colr_enumcs_box(enumcs: u32) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&15u32.to_be_bytes()); // length = 8 + 7
        b.extend_from_slice(&JP2_COLR.to_be_bytes()); // type
        b.push(1); // METH = enumerated
        b.push(0); // PREC
        b.push(0); // APPROX
        b.extend_from_slice(&enumcs.to_be_bytes()); // EnumCS
        b
    }

    /// Build a JP2H super-box containing IHDR and COLR.
    fn build_jp2h_box(w: u32, h: u32, nc: u16, bpc: u8, enumcs: u32) -> Vec<u8> {
        let ihdr = build_ihdr_box(w, h, nc, bpc);
        let colr = build_colr_enumcs_box(enumcs);
        let total_len = 8 + ihdr.len() + colr.len();
        let mut b = Vec::new();
        b.extend_from_slice(&(total_len as u32).to_be_bytes()); // length
        b.extend_from_slice(&JP2_JP2H.to_be_bytes()); // type
        b.extend_from_slice(&ihdr);
        b.extend_from_slice(&colr);
        b
    }

    /// Build a minimal J2K codestream (SOC + SIZ + COD + QCD + SOT + SOD + data + EOC).
    fn build_minimal_j2k(w: u32, h: u32, nc: u16) -> Vec<u8> {
        let mut buf = Vec::new();

        // SOC
        buf.extend_from_slice(&[0xFF, 0x4F]);

        // SIZ marker
        let mut siz_payload = Vec::new();
        siz_payload.extend_from_slice(&0u16.to_be_bytes()); // Rsiz
        siz_payload.extend_from_slice(&w.to_be_bytes()); // Xsiz
        siz_payload.extend_from_slice(&h.to_be_bytes()); // Ysiz
        siz_payload.extend_from_slice(&0u32.to_be_bytes()); // X0siz
        siz_payload.extend_from_slice(&0u32.to_be_bytes()); // Y0siz
        siz_payload.extend_from_slice(&w.to_be_bytes()); // XTsiz
        siz_payload.extend_from_slice(&h.to_be_bytes()); // YTsiz
        siz_payload.extend_from_slice(&0u32.to_be_bytes()); // XT0siz
        siz_payload.extend_from_slice(&0u32.to_be_bytes()); // YT0siz
        siz_payload.extend_from_slice(&nc.to_be_bytes()); // Csiz
        for _ in 0..nc {
            siz_payload.push(0x07); // Ssiz: 8-bit unsigned
            siz_payload.push(0x01); // XRsiz
            siz_payload.push(0x01); // YRsiz
        }
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
        let tile_data = vec![0u8; 4];
        let psot: u32 = 12 + 2 + tile_data.len() as u32;
        buf.extend_from_slice(&[0xFF, 0x90]);
        buf.extend_from_slice(&10u16.to_be_bytes());
        buf.extend_from_slice(&0u16.to_be_bytes());
        buf.extend_from_slice(&psot.to_be_bytes());
        buf.push(0x00);
        buf.push(0x01);

        // SOD
        buf.extend_from_slice(&[0xFF, 0x93]);
        buf.extend_from_slice(&tile_data);

        // EOC
        buf.extend_from_slice(&[0xFF, 0xD9]);

        buf
    }

    /// Build a JP2C box wrapping a J2K codestream.
    fn build_jp2c_box(j2k: &[u8]) -> Vec<u8> {
        let total_len = 8 + j2k.len();
        let mut b = Vec::new();
        b.extend_from_slice(&(total_len as u32).to_be_bytes());
        b.extend_from_slice(&JP2_JP2C.to_be_bytes());
        b.extend_from_slice(j2k);
        b
    }

    /// Build a complete minimal JP2 file.
    fn build_minimal_jp2() -> Vec<u8> {
        let mut file = Vec::new();
        file.extend_from_slice(&build_jp_box());
        file.extend_from_slice(&build_ftyp_box());
        file.extend_from_slice(&build_jp2h_box(8, 8, 1, 8, 17)); // 8x8 gray
        file.extend_from_slice(&build_jp2c_box(&build_minimal_j2k(8, 8, 1)));
        file
    }

    // -----------------------------------------------------------------------
    // Tests: box header reading
    // -----------------------------------------------------------------------

    #[test]
    fn read_box_header_basic() {
        let mut data = Vec::new();
        data.extend_from_slice(&22u32.to_be_bytes());
        data.extend_from_slice(&JP2_IHDR.to_be_bytes());
        data.extend_from_slice(&[0u8; 14]); // dummy payload

        let mut stream = MemoryStream::new_input(data);
        let hdr = read_box_header(&mut stream).unwrap();
        assert_eq!(hdr.length, 22);
        assert_eq!(hdr.box_type, JP2_IHDR);
    }

    #[test]
    fn read_box_header_last_box() {
        // length=0 means "extends to end of stream"
        let mut data = Vec::new();
        data.extend_from_slice(&0u32.to_be_bytes()); // length=0
        data.extend_from_slice(&JP2_JP2C.to_be_bytes());
        data.extend_from_slice(&[0xAA; 20]); // 20 bytes of data

        let mut stream = MemoryStream::new_input(data);
        let hdr = read_box_header(&mut stream).unwrap();
        assert_eq!(hdr.box_type, JP2_JP2C);
        assert_eq!(hdr.length, 20 + 8); // remaining + header
    }

    // -----------------------------------------------------------------------
    // Tests: individual box readers
    // -----------------------------------------------------------------------

    #[test]
    fn read_jp_signature_valid() {
        let mut dec = Jp2Decoder::new();
        dec.read_jp(&JP2_MAGIC.to_be_bytes()).unwrap();
        assert_eq!(dec.state, Jp2State::Signature);
    }

    #[test]
    fn read_jp_signature_bad_magic() {
        let mut dec = Jp2Decoder::new();
        let result = dec.read_jp(&0xDEADBEEFu32.to_be_bytes());
        assert!(result.is_err());
    }

    #[test]
    fn read_jp_signature_wrong_state() {
        let mut dec = Jp2Decoder::new();
        dec.state = Jp2State::Signature; // already past initial state
        let result = dec.read_jp(&JP2_MAGIC.to_be_bytes());
        assert!(result.is_err());
    }

    #[test]
    fn read_ftyp_valid() {
        let mut dec = Jp2Decoder::new();
        dec.state = Jp2State::Signature;
        let mut data = Vec::new();
        data.extend_from_slice(&JP2_JP2_BRAND.to_be_bytes()); // brand
        data.extend_from_slice(&0u32.to_be_bytes()); // minversion
        data.extend_from_slice(&JP2_JP2_BRAND.to_be_bytes()); // CL[0]
        dec.read_ftyp(&data).unwrap();
        assert_eq!(dec.state, Jp2State::FileType);
    }

    #[test]
    fn read_ftyp_wrong_state() {
        let mut dec = Jp2Decoder::new();
        // state is None, not Signature
        let data = vec![0u8; 12];
        let result = dec.read_ftyp(&data);
        assert!(result.is_err());
    }

    #[test]
    fn read_ihdr_valid() {
        let mut dec = Jp2Decoder::new();
        let data = {
            let mut d = Vec::new();
            d.extend_from_slice(&100u32.to_be_bytes()); // HEIGHT
            d.extend_from_slice(&200u32.to_be_bytes()); // WIDTH
            d.extend_from_slice(&3u16.to_be_bytes()); // NC
            d.push(8); // BPC
            d.push(7); // C
            d.push(0); // UnkC
            d.push(0); // IPR
            d
        };
        dec.read_ihdr(&data).unwrap();
        assert_eq!(dec.height, 100);
        assert_eq!(dec.width, 200);
        assert_eq!(dec.numcomps, 3);
        assert_eq!(dec.bpc, 8);
        assert_eq!(dec.compression_type, 7);
        assert!(dec.ihdr_found);
        assert_eq!(dec.comp_info.len(), 3);
        // Uniform BPC: all comp_info should have bpcc=8
        for ci in &dec.comp_info {
            assert_eq!(ci.bpcc, 8);
        }
    }

    #[test]
    fn read_ihdr_zero_dimensions_fails() {
        let mut dec = Jp2Decoder::new();
        let mut data = Vec::new();
        data.extend_from_slice(&0u32.to_be_bytes()); // HEIGHT=0
        data.extend_from_slice(&200u32.to_be_bytes());
        data.extend_from_slice(&3u16.to_be_bytes());
        data.push(8);
        data.push(7);
        data.push(0);
        data.push(0);
        assert!(dec.read_ihdr(&data).is_err());
    }

    #[test]
    fn read_ihdr_bad_size_fails() {
        let mut dec = Jp2Decoder::new();
        let data = vec![0u8; 10]; // too short
        assert!(dec.read_ihdr(&data).is_err());
    }

    #[test]
    fn read_colr_enumerated() {
        let mut dec = Jp2Decoder::new();
        let mut data = Vec::new();
        data.push(1); // METH = enumerated
        data.push(0); // PREC
        data.push(0); // APPROX
        data.extend_from_slice(&16u32.to_be_bytes()); // EnumCS = sRGB
        dec.read_colr(&data).unwrap();
        assert_eq!(dec.colour.meth, ColourMethod::Enumerated);
        assert_eq!(dec.colour.enumcs, 16);
        assert!(dec.colr_found);
    }

    #[test]
    fn read_colr_icc() {
        let mut dec = Jp2Decoder::new();
        let mut data = Vec::new();
        data.push(2); // METH = ICC
        data.push(0); // PREC
        data.push(0); // APPROX
        data.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]); // fake ICC
        dec.read_colr(&data).unwrap();
        assert_eq!(dec.colour.meth, ColourMethod::Icc);
        assert_eq!(dec.colour.icc_profile, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn read_colr_duplicate_ignored() {
        let mut dec = Jp2Decoder::new();
        let mut data = Vec::new();
        data.push(1);
        data.push(0);
        data.push(0);
        data.extend_from_slice(&16u32.to_be_bytes());
        dec.read_colr(&data).unwrap();
        assert_eq!(dec.colour.enumcs, 16);

        // Second COLR should be ignored
        let mut data2 = Vec::new();
        data2.push(1);
        data2.push(0);
        data2.push(0);
        data2.extend_from_slice(&17u32.to_be_bytes());
        dec.read_colr(&data2).unwrap();
        assert_eq!(dec.colour.enumcs, 16); // still 16, not 17
    }

    #[test]
    fn read_bpcc_valid() {
        let mut dec = Jp2Decoder::new();
        // First set up IHDR with bpc=255 (varies)
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&10u32.to_be_bytes());
        ihdr.extend_from_slice(&10u32.to_be_bytes());
        ihdr.extend_from_slice(&3u16.to_be_bytes());
        ihdr.push(255); // BPC=255 → per-component
        ihdr.push(7);
        ihdr.push(0);
        ihdr.push(0);
        dec.read_ihdr(&ihdr).unwrap();

        // Now read BPCC
        let bpcc_data = vec![8, 10, 12]; // 3 components: 8, 10, 12 bits
        dec.read_bpcc(&bpcc_data).unwrap();
        assert_eq!(dec.comp_info[0].bpcc, 8);
        assert_eq!(dec.comp_info[1].bpcc, 10);
        assert_eq!(dec.comp_info[2].bpcc, 12);
    }

    #[test]
    fn read_bpcc_before_ihdr_fails() {
        let mut dec = Jp2Decoder::new();
        let data = vec![8, 10, 12];
        assert!(dec.read_bpcc(&data).is_err());
    }

    #[test]
    fn read_bpcc_wrong_size_fails() {
        let mut dec = Jp2Decoder::new();
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&10u32.to_be_bytes());
        ihdr.extend_from_slice(&10u32.to_be_bytes());
        ihdr.extend_from_slice(&3u16.to_be_bytes());
        ihdr.push(255);
        ihdr.push(7);
        ihdr.push(0);
        ihdr.push(0);
        dec.read_ihdr(&ihdr).unwrap();

        let data = vec![8, 10]; // 2 bytes for 3 components
        assert!(dec.read_bpcc(&data).is_err());
    }

    // -----------------------------------------------------------------------
    // Tests: JP2H super-box parsing
    // -----------------------------------------------------------------------

    #[test]
    fn read_jp2h_with_ihdr_and_colr() {
        let mut dec = Jp2Decoder::new();
        dec.state = Jp2State::FileType;

        let ihdr = build_ihdr_box(8, 8, 1, 8);
        let colr = build_colr_enumcs_box(17); // gray
        let mut payload = Vec::new();
        payload.extend_from_slice(&ihdr);
        payload.extend_from_slice(&colr);

        dec.read_jp2h(&payload).unwrap();
        assert_eq!(dec.state, Jp2State::Header);
        assert_eq!(dec.width, 8);
        assert_eq!(dec.height, 8);
        assert_eq!(dec.numcomps, 1);
        assert!(dec.colr_found);
    }

    #[test]
    fn read_jp2h_missing_ihdr_fails() {
        let mut dec = Jp2Decoder::new();
        dec.state = Jp2State::FileType;

        // JP2H with only COLR, no IHDR
        let colr = build_colr_enumcs_box(17);
        assert!(dec.read_jp2h(&colr).is_err());
    }

    // -----------------------------------------------------------------------
    // Tests: full JP2 header reading
    // -----------------------------------------------------------------------

    #[test]
    fn read_jp2_header_minimal() {
        let jp2_data = build_minimal_jp2();
        let mut stream = MemoryStream::new_input(jp2_data);
        let mut dec = Jp2Decoder::new();

        dec.read_header(&mut stream).unwrap();

        assert_eq!(dec.state, Jp2State::Codestream);
        assert_eq!(dec.width, 8);
        assert_eq!(dec.height, 8);
        assert_eq!(dec.numcomps, 1);
        assert_eq!(dec.bpc, 8);
        assert!(dec.ihdr_found);
        assert!(dec.colr_found);
    }

    #[test]
    fn read_jp2_header_then_codestream() {
        let jp2_data = build_minimal_jp2();
        let mut stream = MemoryStream::new_input(jp2_data);
        let mut dec = Jp2Decoder::new();

        dec.read_header(&mut stream).unwrap();
        dec.read_codestream(&mut stream).unwrap();

        // J2K decoder should have parsed the codestream
        assert_eq!(dec.j2k.image.x1, 8);
        assert_eq!(dec.j2k.image.y1, 8);
        assert_eq!(dec.j2k.image.comps.len(), 1);
        assert_eq!(dec.j2k.tile_data.len(), 1);
    }

    #[test]
    fn read_jp2_colour_applied_to_image() {
        let jp2_data = build_minimal_jp2(); // gray (enumcs=17)
        let mut stream = MemoryStream::new_input(jp2_data);
        let mut dec = Jp2Decoder::new();

        dec.read_header(&mut stream).unwrap();
        dec.read_codestream(&mut stream).unwrap();

        assert_eq!(dec.j2k.image.color_space, ColorSpace::Gray);
    }

    #[test]
    fn read_jp2_srgb_colour() {
        let mut file = Vec::new();
        file.extend_from_slice(&build_jp_box());
        file.extend_from_slice(&build_ftyp_box());
        file.extend_from_slice(&build_jp2h_box(8, 8, 3, 8, 16)); // sRGB
        file.extend_from_slice(&build_jp2c_box(&build_minimal_j2k(8, 8, 3)));

        let mut stream = MemoryStream::new_input(file);
        let mut dec = Jp2Decoder::new();
        dec.read_header(&mut stream).unwrap();
        dec.read_codestream(&mut stream).unwrap();

        assert_eq!(dec.j2k.image.color_space, ColorSpace::Srgb);
        assert_eq!(dec.j2k.image.comps.len(), 3);
    }

    #[test]
    fn read_jp2_jp2c_before_header_fails() {
        // JP2C without JP/FTYP/JP2H
        let j2k = build_minimal_j2k(8, 8, 1);
        let mut file = Vec::new();
        file.extend_from_slice(&build_jp_box());
        file.extend_from_slice(&build_ftyp_box());
        // Skip JP2H, go straight to JP2C
        file.extend_from_slice(&build_jp2c_box(&j2k));

        let mut stream = MemoryStream::new_input(file);
        let mut dec = Jp2Decoder::new();
        assert!(dec.read_header(&mut stream).is_err());
    }

    #[test]
    fn read_codestream_before_header_fails() {
        let jp2_data = build_minimal_jp2();
        let mut stream = MemoryStream::new_input(jp2_data);
        let mut dec = Jp2Decoder::new();
        // Don't call read_header first
        assert!(dec.read_codestream(&mut stream).is_err());
    }
}
