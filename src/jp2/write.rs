// Phase 600c: JP2 encoder
//
// Writes a JP2 file: JP, FTYP, JP2H (IHDR, BPCC, COLR, CDEF), JP2C boxes,
// wrapping a J2K codestream.

use crate::error::Result;
use crate::image::Image;
use crate::jp2::{
    CdefEntry, ColourMethod, JP2_BPCC, JP2_CDEF, JP2_COLR, JP2_FTYP, JP2_IHDR, JP2_JP,
    JP2_JP2_BRAND, JP2_JP2C, JP2_JP2H, JP2_MAGIC, Jp2Colour,
};

/// JP2 file encoder.
pub struct Jp2Encoder {
    /// Output buffer (accumulated JP2 file bytes).
    pub output: Vec<u8>,
}

impl Default for Jp2Encoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Jp2Encoder {
    /// Create a new JP2 encoder.
    pub fn new() -> Self {
        Self { output: Vec::new() }
    }

    /// Write JP2 file header boxes (JP, FTYP, JP2H).
    ///
    /// `cdef` is optional channel definition entries (for alpha channels).
    pub fn write_header(
        &mut self,
        image: &Image,
        colour: &Jp2Colour,
        cdef: Option<&[CdefEntry]>,
    ) -> Result<()> {
        self.write_jp();
        self.write_ftyp();
        self.write_jp2h(image, colour, cdef);
        Ok(())
    }

    /// Write JP2C box wrapping a pre-encoded J2K codestream.
    pub fn write_codestream(&mut self, j2k_data: &[u8]) {
        let length = 8 + j2k_data.len() as u32;
        self.write_u32(length);
        self.write_u32(JP2_JP2C);
        self.output.extend_from_slice(j2k_data);
    }

    /// Return the complete JP2 file data.
    pub fn finalize(self) -> Vec<u8> {
        self.output
    }

    /// Write JP signature box (12 bytes).
    fn write_jp(&mut self) {
        self.write_u32(12); // length
        self.write_u32(JP2_JP); // type
        self.write_u32(JP2_MAGIC); // magic
    }

    /// Write FTYP (File Type) box (20 bytes).
    fn write_ftyp(&mut self) {
        self.write_u32(20); // length
        self.write_u32(JP2_FTYP); // type
        self.write_u32(JP2_JP2_BRAND); // brand
        self.write_u32(0); // minversion
        self.write_u32(JP2_JP2_BRAND); // CL[0]
    }

    /// Write JP2H (JP2 Header) super-box.
    fn write_jp2h(&mut self, image: &Image, colour: &Jp2Colour, cdef: Option<&[CdefEntry]>) {
        // Build sub-boxes into a temporary buffer to compute total size
        let mut sub = Vec::new();

        // Determine uniform BPC
        let bpc = compute_bpc(image);

        // IHDR (22 bytes)
        write_ihdr(&mut sub, image, bpc);

        // BPCC (only if variable BPC)
        if bpc == 255 {
            write_bpcc(&mut sub, image);
        }

        // COLR
        write_colr(&mut sub, colour);

        // CDEF (optional)
        if let Some(entries) = cdef {
            write_cdef(&mut sub, entries);
        }

        // Write JP2H box header + sub-boxes
        let jp2h_len = 8 + sub.len() as u32;
        self.write_u32(jp2h_len);
        self.write_u32(JP2_JP2H);
        self.output.extend_from_slice(&sub);
    }

    /// Write a big-endian u32.
    fn write_u32(&mut self, val: u32) {
        self.output.extend_from_slice(&val.to_be_bytes());
    }
}

/// Compute uniform BPC from image components.
/// Returns 255 if components have different (prec, sgnd).
fn compute_bpc(image: &Image) -> u8 {
    if image.comps.is_empty() {
        return 0;
    }
    let first_prec = image.comps[0].prec;
    let first_sgnd = image.comps[0].sgnd;
    let uniform = image
        .comps
        .iter()
        .all(|c| c.prec == first_prec && c.sgnd == first_sgnd);
    if uniform {
        encode_bpc(first_prec, first_sgnd)
    } else {
        255
    }
}

/// Write IHDR sub-box (22 bytes) to buffer.
fn write_ihdr(buf: &mut Vec<u8>, image: &Image, bpc: u8) {
    buf.extend_from_slice(&22u32.to_be_bytes()); // length
    buf.extend_from_slice(&JP2_IHDR.to_be_bytes()); // type
    buf.extend_from_slice(&(image.y1 - image.y0).to_be_bytes()); // height
    buf.extend_from_slice(&(image.x1 - image.x0).to_be_bytes()); // width
    buf.extend_from_slice(&(image.comps.len() as u16).to_be_bytes()); // numcomps
    buf.push(bpc); // BPC
    buf.push(7); // compression type (JPEG 2000)
    buf.push(0); // UnkC (colourspace known)
    buf.push(0); // IPR (no intellectual property)
}

/// Write BPCC sub-box to buffer.
fn write_bpcc(buf: &mut Vec<u8>, image: &Image) {
    let length = 8 + image.comps.len() as u32;
    buf.extend_from_slice(&length.to_be_bytes());
    buf.extend_from_slice(&JP2_BPCC.to_be_bytes());
    for comp in &image.comps {
        buf.push(encode_bpc(comp.prec, comp.sgnd));
    }
}

/// Write COLR sub-box to buffer.
fn write_colr(buf: &mut Vec<u8>, colour: &Jp2Colour) {
    match colour.meth {
        ColourMethod::Enumerated => {
            let length = 15u32; // 8 + 3 + 4
            buf.extend_from_slice(&length.to_be_bytes());
            buf.extend_from_slice(&JP2_COLR.to_be_bytes());
            buf.push(1); // METH = enumerated
            buf.push(colour.precedence);
            buf.push(colour.approx);
            buf.extend_from_slice(&colour.enumcs.to_be_bytes());
        }
        ColourMethod::Icc => {
            let length = 11 + colour.icc_profile.len() as u32;
            buf.extend_from_slice(&length.to_be_bytes());
            buf.extend_from_slice(&JP2_COLR.to_be_bytes());
            buf.push(2); // METH = ICC
            buf.push(colour.precedence);
            buf.push(colour.approx);
            buf.extend_from_slice(&colour.icc_profile);
        }
    }
}

/// Write CDEF sub-box to buffer.
fn write_cdef(buf: &mut Vec<u8>, entries: &[CdefEntry]) {
    let length = 8 + 2 + entries.len() as u32 * 6;
    buf.extend_from_slice(&length.to_be_bytes());
    buf.extend_from_slice(&JP2_CDEF.to_be_bytes());
    buf.extend_from_slice(&(entries.len() as u16).to_be_bytes());
    for e in entries {
        buf.extend_from_slice(&e.cn.to_be_bytes());
        buf.extend_from_slice(&e.typ.to_be_bytes());
        buf.extend_from_slice(&e.asoc.to_be_bytes());
    }
}

/// Encode a raw BPC value from (precision, signed).
///
/// JP2 encoding: bit 7 = signedness, bits 0-6 = (precision - 1).
fn encode_bpc(prec: u32, sgnd: bool) -> u8 {
    let mut val = (prec.saturating_sub(1) & 0x7F) as u8;
    if sgnd {
        val |= 0x80;
    }
    val
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::ImageCompParam;
    use crate::io::cio::{MemoryStream, read_bytes_be};
    use crate::jp2::read::Jp2Decoder;
    use crate::types::ColorSpace;

    fn create_test_image(nc: usize, prec: u32, sgnd: bool) -> Image {
        let params: Vec<_> = (0..nc)
            .map(|_| ImageCompParam {
                dx: 1,
                dy: 1,
                w: 8,
                h: 8,
                x0: 0,
                y0: 0,
                prec,
                sgnd,
            })
            .collect();
        let cs = if nc == 1 {
            ColorSpace::Gray
        } else {
            ColorSpace::Srgb
        };
        let mut image = Image::new(&params, cs);
        image.x0 = 0;
        image.y0 = 0;
        image.x1 = 8;
        image.y1 = 8;
        image
    }

    fn create_gray_colour() -> Jp2Colour {
        Jp2Colour {
            meth: ColourMethod::Enumerated,
            precedence: 0,
            approx: 0,
            enumcs: 17,
            icc_profile: Vec::new(),
        }
    }

    fn create_srgb_colour() -> Jp2Colour {
        Jp2Colour {
            meth: ColourMethod::Enumerated,
            precedence: 0,
            approx: 0,
            enumcs: 16,
            icc_profile: Vec::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Tests: encode_bpc helper
    // -----------------------------------------------------------------------

    #[test]
    fn encode_bpc_8bit_unsigned() {
        assert_eq!(encode_bpc(8, false), 0x07);
    }

    #[test]
    fn encode_bpc_16bit_signed() {
        assert_eq!(encode_bpc(16, true), 0x8F);
    }

    // -----------------------------------------------------------------------
    // Tests: box writing
    // -----------------------------------------------------------------------

    #[test]

    fn write_header_starts_with_jp_box() {
        let image = create_test_image(1, 8, false);
        let colour = create_gray_colour();
        let mut enc = Jp2Encoder::new();
        enc.write_header(&image, &colour, None).unwrap();

        // JP signature box: length=12, type=JP2_JP, magic
        assert_eq!(read_bytes_be(&enc.output[0..], 4), 12);
        assert_eq!(read_bytes_be(&enc.output[4..], 4), JP2_JP);
        assert_eq!(read_bytes_be(&enc.output[8..], 4), JP2_MAGIC);
    }

    #[test]

    fn write_header_has_ftyp_box() {
        let image = create_test_image(1, 8, false);
        let colour = create_gray_colour();
        let mut enc = Jp2Encoder::new();
        enc.write_header(&image, &colour, None).unwrap();

        // FTYP follows JP at offset 12
        assert_eq!(read_bytes_be(&enc.output[12..], 4), 20); // length
        assert_eq!(read_bytes_be(&enc.output[16..], 4), JP2_FTYP);
        assert_eq!(read_bytes_be(&enc.output[20..], 4), JP2_JP2_BRAND);
    }

    #[test]

    fn write_header_has_jp2h_with_ihdr() {
        let image = create_test_image(1, 8, false);
        let colour = create_gray_colour();
        let mut enc = Jp2Encoder::new();
        enc.write_header(&image, &colour, None).unwrap();

        // JP2H follows FTYP at offset 32
        let jp2h_off = 32;
        assert_eq!(read_bytes_be(&enc.output[jp2h_off + 4..], 4), JP2_JP2H);

        // IHDR is first sub-box at jp2h_off + 8
        let ihdr_off = jp2h_off + 8;
        assert_eq!(read_bytes_be(&enc.output[ihdr_off..], 4), 22); // IHDR length
        assert_eq!(read_bytes_be(&enc.output[ihdr_off + 4..], 4), JP2_IHDR);
        assert_eq!(read_bytes_be(&enc.output[ihdr_off + 8..], 4), 8); // height
        assert_eq!(read_bytes_be(&enc.output[ihdr_off + 12..], 4), 8); // width
        assert_eq!(read_bytes_be(&enc.output[ihdr_off + 16..], 2), 1); // numcomps
        assert_eq!(enc.output[ihdr_off + 18], 0x07); // BPC (8-bit unsigned)
        assert_eq!(enc.output[ihdr_off + 19], 7); // compression type
    }

    #[test]

    fn write_header_has_colr_enumerated() {
        let image = create_test_image(1, 8, false);
        let colour = create_gray_colour();
        let mut enc = Jp2Encoder::new();
        enc.write_header(&image, &colour, None).unwrap();

        // Find COLR box after IHDR
        let colr_off = 32 + 8 + 22; // JP2H header + IHDR
        assert_eq!(read_bytes_be(&enc.output[colr_off + 4..], 4), JP2_COLR);
        assert_eq!(enc.output[colr_off + 8], 1); // METH=1 (enumerated)
        assert_eq!(read_bytes_be(&enc.output[colr_off + 11..], 4), 17); // EnumCS=gray
    }

    #[test]

    fn write_header_with_icc_profile() {
        let image = create_test_image(3, 8, false);
        let colour = Jp2Colour {
            meth: ColourMethod::Icc,
            precedence: 0,
            approx: 0,
            enumcs: 0,
            icc_profile: vec![0xDE, 0xAD, 0xBE, 0xEF],
        };
        let mut enc = Jp2Encoder::new();
        enc.write_header(&image, &colour, None).unwrap();

        // Find COLR box: offset = JP(12) + FTYP(20) + JP2H_hdr(8) + IHDR(22)
        let colr_off = 12 + 20 + 8 + 22;
        assert_eq!(read_bytes_be(&enc.output[colr_off + 4..], 4), JP2_COLR);
        assert_eq!(enc.output[colr_off + 8], 2); // METH=2 (ICC)
        // ICC profile data follows at offset + 11
        assert_eq!(
            &enc.output[colr_off + 11..colr_off + 15],
            &[0xDE, 0xAD, 0xBE, 0xEF]
        );
    }

    #[test]

    fn write_header_variable_bpc_writes_bpcc() {
        // 3 components with different precisions
        let mut image = create_test_image(3, 8, false);
        image.comps[1].prec = 10;
        image.comps[2].prec = 12;
        let colour = create_srgb_colour();
        let mut enc = Jp2Encoder::new();
        enc.write_header(&image, &colour, None).unwrap();

        // IHDR BPC should be 255
        let ihdr_off = 32 + 8;
        assert_eq!(enc.output[ihdr_off + 18], 255);

        // BPCC box follows IHDR
        let bpcc_off = ihdr_off + 22;
        assert_eq!(read_bytes_be(&enc.output[bpcc_off + 4..], 4), JP2_BPCC);
        assert_eq!(enc.output[bpcc_off + 8], 0x07); // comp 0: 8-bit unsigned
        assert_eq!(enc.output[bpcc_off + 9], 0x09); // comp 1: 10-bit unsigned
        assert_eq!(enc.output[bpcc_off + 10], 0x0B); // comp 2: 12-bit unsigned
    }

    #[test]

    fn write_header_with_cdef() {
        let image = create_test_image(3, 8, false);
        let colour = create_srgb_colour();
        let cdef = vec![
            CdefEntry {
                cn: 0,
                typ: 0,
                asoc: 1,
            },
            CdefEntry {
                cn: 1,
                typ: 0,
                asoc: 2,
            },
            CdefEntry {
                cn: 2,
                typ: 0,
                asoc: 3,
            },
        ];
        let mut enc = Jp2Encoder::new();
        enc.write_header(&image, &colour, Some(&cdef)).unwrap();

        // Find CDEF in output (after JP2H header + IHDR + COLR)
        let found = enc
            .output
            .windows(4)
            .any(|w| read_bytes_be(w, 4) == JP2_CDEF);
        assert!(found, "CDEF box not found in output");
    }

    // -----------------------------------------------------------------------
    // Tests: JP2C codestream box
    // -----------------------------------------------------------------------

    #[test]

    fn write_codestream_wraps_in_jp2c() {
        let j2k_data = vec![0xFF, 0x4F, 0xAA, 0xBB]; // fake J2K
        let mut enc = Jp2Encoder::new();
        enc.write_codestream(&j2k_data);

        assert_eq!(read_bytes_be(&enc.output[0..], 4), 12); // length = 8 + 4
        assert_eq!(read_bytes_be(&enc.output[4..], 4), JP2_JP2C);
        assert_eq!(&enc.output[8..], &j2k_data);
    }

    // -----------------------------------------------------------------------
    // Tests: roundtrip (encode → decode)
    // -----------------------------------------------------------------------

    #[test]

    fn roundtrip_gray_jp2() {
        let image = create_test_image(1, 8, false);
        let colour = create_gray_colour();

        // Build minimal J2K codestream (reuse from read tests)
        let j2k = build_minimal_j2k(8, 8, 1);

        let mut enc = Jp2Encoder::new();
        enc.write_header(&image, &colour, None).unwrap();
        enc.write_codestream(&j2k);
        let jp2_data = enc.finalize();

        // Decode
        let mut stream = MemoryStream::new_input(jp2_data);
        let mut dec = Jp2Decoder::new();
        dec.read_header(&mut stream).unwrap();

        assert_eq!(dec.width, 8);
        assert_eq!(dec.height, 8);
        assert_eq!(dec.numcomps, 1);
        assert_eq!(dec.colour.enumcs, 17); // Gray
    }

    #[test]

    fn roundtrip_srgb_jp2() {
        let image = create_test_image(3, 8, false);
        let colour = create_srgb_colour();

        let j2k = build_minimal_j2k(8, 8, 3);

        let mut enc = Jp2Encoder::new();
        enc.write_header(&image, &colour, None).unwrap();
        enc.write_codestream(&j2k);
        let jp2_data = enc.finalize();

        // Decode
        let mut stream = MemoryStream::new_input(jp2_data);
        let mut dec = Jp2Decoder::new();
        dec.read_header(&mut stream).unwrap();
        dec.read_codestream(&mut stream).unwrap();

        assert_eq!(dec.j2k.image.color_space, ColorSpace::Srgb);
        assert_eq!(dec.j2k.image.comps.len(), 3);
    }

    /// Build a minimal J2K codestream (SOC + SIZ + COD + QCD + SOT + SOD + data + EOC).
    fn build_minimal_j2k(w: u32, h: u32, nc: u16) -> Vec<u8> {
        let mut buf = Vec::new();

        // SOC
        buf.extend_from_slice(&[0xFF, 0x4F]);

        // SIZ
        let mut siz_payload = Vec::new();
        siz_payload.extend_from_slice(&0u16.to_be_bytes());
        siz_payload.extend_from_slice(&w.to_be_bytes());
        siz_payload.extend_from_slice(&h.to_be_bytes());
        siz_payload.extend_from_slice(&0u32.to_be_bytes());
        siz_payload.extend_from_slice(&0u32.to_be_bytes());
        siz_payload.extend_from_slice(&w.to_be_bytes());
        siz_payload.extend_from_slice(&h.to_be_bytes());
        siz_payload.extend_from_slice(&0u32.to_be_bytes());
        siz_payload.extend_from_slice(&0u32.to_be_bytes());
        siz_payload.extend_from_slice(&nc.to_be_bytes());
        for _ in 0..nc {
            siz_payload.push(0x07);
            siz_payload.push(0x01);
            siz_payload.push(0x01);
        }
        let siz_len = (siz_payload.len() + 2) as u16;
        buf.extend_from_slice(&[0xFF, 0x51]);
        buf.extend_from_slice(&siz_len.to_be_bytes());
        buf.extend_from_slice(&siz_payload);

        // COD
        let mut cod_payload = Vec::new();
        cod_payload.push(0x00);
        cod_payload.push(0x00);
        cod_payload.extend_from_slice(&1u16.to_be_bytes());
        cod_payload.push(0x00);
        cod_payload.push(0x00);
        cod_payload.push(0x04);
        cod_payload.push(0x04);
        cod_payload.push(0x00);
        cod_payload.push(0x01);
        let cod_len = (cod_payload.len() + 2) as u16;
        buf.extend_from_slice(&[0xFF, 0x52]);
        buf.extend_from_slice(&cod_len.to_be_bytes());
        buf.extend_from_slice(&cod_payload);

        // QCD
        let qcd_payload = vec![0x40, 0x40];
        let qcd_len = (qcd_payload.len() + 2) as u16;
        buf.extend_from_slice(&[0xFF, 0x5C]);
        buf.extend_from_slice(&qcd_len.to_be_bytes());
        buf.extend_from_slice(&qcd_payload);

        // SOT
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
}
