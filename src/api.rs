// Phase 600d: Public API facade
//
// High-level codec API for JPEG 2000 encoding and decoding.
// Supports both J2K (raw codestream) and JP2 (file format) codecs.

use crate::error::{Error, Result};
use crate::image::Image;
use crate::io::cio::MemoryStream;
use crate::j2k::read::J2kDecoder;
use crate::jp2::read::Jp2Decoder;
use crate::jp2::write::Jp2Encoder;
use crate::jp2::{ColourMethod, JP2_MAGIC, Jp2Colour};
use crate::types::ColorSpace;

/// Codec format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecFormat {
    /// Raw J2K codestream (.j2k, .j2c).
    J2k,
    /// JP2 file format (.jp2).
    Jp2,
}

/// Decode a JPEG 2000 image from a byte buffer.
///
/// Uses the specified `format` to select the appropriate codec.
/// Use [`detect_format`] to determine the format from file contents.
pub fn decode(data: &[u8], format: CodecFormat) -> Result<Image> {
    if data.is_empty() {
        return Err(Error::EndOfStream);
    }
    decode_owned(data.to_vec(), format)
}

/// Decode a JPEG 2000 image from an owned byte buffer (zero-copy).
///
/// Like [`decode`], but takes ownership of the data to avoid copying.
pub fn decode_owned(data: Vec<u8>, format: CodecFormat) -> Result<Image> {
    if data.is_empty() {
        return Err(Error::EndOfStream);
    }
    let mut stream = MemoryStream::new_input(data);

    match format {
        CodecFormat::J2k => {
            let mut dec = J2kDecoder::new();
            dec.read_header(&mut stream)?;
            dec.read_all_tiles(&mut stream)?;
            dec.decode_tiles()?;
            Ok(dec.image)
        }
        CodecFormat::Jp2 => {
            let mut dec = Jp2Decoder::new();
            dec.read_header(&mut stream)?;
            dec.read_codestream(&mut stream)?;
            Ok(dec.j2k.image)
        }
    }
}

/// Encode an image to JPEG 2000 format.
///
/// Returns the encoded bytes. For JP2, wraps in JP2 file format boxes.
/// For J2K, returns raw codestream.
///
/// Currently produces a minimal J2K codestream (header + empty tiles).
/// Full T1/T2 encoding is handled by J2kEncoder when available.
pub fn encode(image: &Image, format: CodecFormat) -> Result<Vec<u8>> {
    use crate::j2k::params::{CodingParameters, TileCodingParameters, TileCompCodingParameters};
    use crate::j2k::write::J2kEncoder;
    use crate::tcd::Tcd;

    // Build coding parameters from image
    let w = image.x1.saturating_sub(image.x0);
    let h = image.y1.saturating_sub(image.y0);
    if w == 0 || h == 0 || image.comps.is_empty() {
        return Err(Error::InvalidInput("Invalid image for encoding".into()));
    }

    let prec = image.comps[0].prec;
    // Validate all components share the same precision (per-component precision not yet supported)
    if image.comps.iter().any(|c| c.prec != prec) {
        return Err(Error::InvalidInput(
            "All components must have the same precision for encoding".into(),
        ));
    }

    let tccps: Vec<_> = image
        .comps
        .iter()
        .map(|comp| {
            let comp_dc = if !comp.sgnd && comp.prec > 0 && comp.prec <= 31 {
                1i32 << (comp.prec - 1)
            } else {
                0
            };
            TileCompCodingParameters {
                numresolutions: 1,
                cblkw: 6,
                cblkh: 6,
                qmfbid: 1,
                m_dc_level_shift: comp_dc,
                ..Default::default()
            }
        })
        .collect();
    let mut tcp = TileCodingParameters {
        numlayers: 1,
        tccps,
        ..Default::default()
    };

    // Compute stepsizes for encoding
    Tcd::calc_explicit_stepsizes(&mut tcp, prec);

    let cp = CodingParameters {
        tx0: image.x0,
        ty0: image.y0,
        tdx: w,
        tdy: h,
        tw: 1,
        th: 1,
        tcps: vec![tcp.clone()],
        ..CodingParameters::new_encoder()
    };

    // Encode tile data via TCD pipeline
    let mut tcd = Tcd::new(true);
    tcd.init_tile(0, image, &cp, &tcp, true)?;

    // Copy image pixel data to tile components
    for (compno, comp) in image.comps.iter().enumerate() {
        if compno < tcd.tile.comps.len() {
            let tile_comp = &mut tcd.tile.comps[compno];
            let expected = tile_comp.numpix;
            if comp.data.len() != expected {
                tile_comp.data.resize(comp.data.len(), 0);
            }
            tile_comp.data.copy_from_slice(&comp.data);
        }
    }

    // Encode: DC shift → MCT → DWT → T1 → makelayer → T2
    let mut buf_size = (w as usize)
        .checked_mul(h as usize)
        .and_then(|v| v.checked_mul(image.comps.len()))
        .and_then(|v| v.checked_mul(4))
        .and_then(|v| v.checked_add(4096))
        .ok_or(Error::BufferTooSmall)?;
    let mut tile_buf = vec![0u8; buf_size];
    let tile_len = loop {
        match tcd.encode_tile(image, &cp, &tcp, &mut tile_buf) {
            Ok(len) => break len,
            Err(Error::BufferTooSmall) => {
                buf_size = buf_size.checked_mul(2).ok_or(Error::BufferTooSmall)?;
                tile_buf = vec![0u8; buf_size];
            }
            Err(e) => return Err(e),
        }
    };

    // Write J2K codestream
    let mut j2k_enc = J2kEncoder::new();
    j2k_enc.write_header(image, &cp, &tcp)?;
    j2k_enc.write_tile(0, &tile_buf[..tile_len], 0, 1)?;
    let j2k_data = j2k_enc.finalize();

    match format {
        CodecFormat::J2k => Ok(j2k_data),
        CodecFormat::Jp2 => {
            let colour = colour_from_image(image);
            let mut jp2_enc = Jp2Encoder::new();
            jp2_enc.write_header(image, &colour, None)?;
            jp2_enc.write_codestream(&j2k_data)?;
            Ok(jp2_enc.finalize())
        }
    }
}

/// Detect codec format from the first bytes of data.
///
/// J2K starts with SOC marker (0xFF4F).
/// JP2 starts with a JP signature box (length=12, type=0x6A502020).
pub fn detect_format(data: &[u8]) -> Option<CodecFormat> {
    // Check J2K: SOC marker (2 bytes)
    if data.len() >= 2 && data[0] == 0xFF && data[1] == 0x4F {
        return Some(CodecFormat::J2k);
    }
    // Check JP2: signature box (length=12, type='jP  ', magic=0x0D0A870A)
    if data.len() >= 12 {
        let box_len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let box_type = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let magic = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
        if box_len == 12 && box_type == crate::jp2::JP2_JP && magic == JP2_MAGIC {
            return Some(CodecFormat::Jp2);
        }
    }
    None
}

/// Derive JP2 colour info from image color space.
fn colour_from_image(image: &Image) -> Jp2Colour {
    let enumcs = match image.color_space {
        ColorSpace::Srgb => 16,
        ColorSpace::Gray => 17,
        ColorSpace::Sycc => 18,
        ColorSpace::Eycc => 24,
        _ => 0,
    };
    if !image.icc_profile.is_empty() {
        Jp2Colour {
            meth: ColourMethod::Icc,
            precedence: 0,
            approx: 0,
            enumcs: 0,
            icc_profile: image.icc_profile.clone(),
        }
    } else {
        Jp2Colour {
            meth: ColourMethod::Enumerated,
            precedence: 0,
            approx: 0,
            enumcs,
            icc_profile: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::ImageCompParam;
    #[allow(unused_imports)]
    use crate::io::cio::read_bytes_be;

    fn build_minimal_j2k(w: u32, h: u32, nc: u16) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0xFF, 0x4F]);

        let mut siz = Vec::new();
        siz.extend_from_slice(&0u16.to_be_bytes());
        siz.extend_from_slice(&w.to_be_bytes());
        siz.extend_from_slice(&h.to_be_bytes());
        siz.extend_from_slice(&0u32.to_be_bytes());
        siz.extend_from_slice(&0u32.to_be_bytes());
        siz.extend_from_slice(&w.to_be_bytes());
        siz.extend_from_slice(&h.to_be_bytes());
        siz.extend_from_slice(&0u32.to_be_bytes());
        siz.extend_from_slice(&0u32.to_be_bytes());
        siz.extend_from_slice(&nc.to_be_bytes());
        for _ in 0..nc {
            siz.push(0x07);
            siz.push(0x01);
            siz.push(0x01);
        }
        let siz_len = (siz.len() + 2) as u16;
        buf.extend_from_slice(&[0xFF, 0x51]);
        buf.extend_from_slice(&siz_len.to_be_bytes());
        buf.extend_from_slice(&siz);

        let cod = vec![0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x04, 0x04, 0x00, 0x01];
        let cod_len = (cod.len() + 2) as u16;
        buf.extend_from_slice(&[0xFF, 0x52]);
        buf.extend_from_slice(&cod_len.to_be_bytes());
        buf.extend_from_slice(&cod);

        let qcd = vec![0x40, 0x40];
        let qcd_len = (qcd.len() + 2) as u16;
        buf.extend_from_slice(&[0xFF, 0x5C]);
        buf.extend_from_slice(&qcd_len.to_be_bytes());
        buf.extend_from_slice(&qcd);

        let tile_data = vec![0u8; 4];
        let psot: u32 = 12 + 2 + tile_data.len() as u32;
        buf.extend_from_slice(&[0xFF, 0x90]);
        buf.extend_from_slice(&10u16.to_be_bytes());
        buf.extend_from_slice(&0u16.to_be_bytes());
        buf.extend_from_slice(&psot.to_be_bytes());
        buf.push(0x00);
        buf.push(0x01);
        buf.extend_from_slice(&[0xFF, 0x93]);
        buf.extend_from_slice(&tile_data);
        buf.extend_from_slice(&[0xFF, 0xD9]);
        buf
    }

    fn build_minimal_jp2() -> Vec<u8> {
        let mut enc = Jp2Encoder::new();
        let params: Vec<_> = (0..1)
            .map(|_| ImageCompParam {
                dx: 1,
                dy: 1,
                w: 8,
                h: 8,
                x0: 0,
                y0: 0,
                prec: 8,
                sgnd: false,
            })
            .collect();
        let mut image = Image::new(&params, ColorSpace::Gray);
        image.x0 = 0;
        image.y0 = 0;
        image.x1 = 8;
        image.y1 = 8;
        let colour = Jp2Colour {
            meth: ColourMethod::Enumerated,
            precedence: 0,
            approx: 0,
            enumcs: 17,
            icc_profile: Vec::new(),
        };
        enc.write_header(&image, &colour, None).unwrap();
        enc.write_codestream(&build_minimal_j2k(8, 8, 1)).unwrap();
        enc.finalize()
    }

    // -----------------------------------------------------------------------
    // Tests: detect_format
    // -----------------------------------------------------------------------

    #[test]
    fn detect_format_j2k() {
        let j2k = build_minimal_j2k(8, 8, 1);
        assert_eq!(detect_format(&j2k), Some(CodecFormat::J2k));
    }

    #[test]
    fn detect_format_jp2() {
        let jp2 = build_minimal_jp2();
        assert_eq!(detect_format(&jp2), Some(CodecFormat::Jp2));
    }

    #[test]
    fn detect_format_unknown() {
        assert_eq!(detect_format(&[0x00, 0x01, 0x02, 0x03]), None);
    }

    #[test]
    fn detect_format_too_short() {
        assert_eq!(detect_format(&[0xFF]), None);
    }

    // -----------------------------------------------------------------------
    // Tests: decode
    // -----------------------------------------------------------------------

    #[test]
    fn decode_j2k_basic() {
        let j2k = build_minimal_j2k(8, 8, 1);
        let image = decode(&j2k, CodecFormat::J2k).unwrap();
        assert_eq!(image.x1, 8);
        assert_eq!(image.y1, 8);
        assert_eq!(image.comps.len(), 1);
    }

    #[test]
    fn decode_j2k_produces_pixels() {
        let j2k = build_minimal_j2k(8, 8, 1);
        let image = decode(&j2k, CodecFormat::J2k).unwrap();
        assert_eq!(image.comps[0].data.len(), 64); // 8*8 pixels
    }

    #[test]
    fn decode_jp2_basic() {
        let jp2 = build_minimal_jp2();
        let image = decode(&jp2, CodecFormat::Jp2).unwrap();
        assert_eq!(image.x1, 8);
        assert_eq!(image.y1, 8);
        assert_eq!(image.comps.len(), 1);
        assert_eq!(image.color_space, ColorSpace::Gray);
    }

    #[test]
    fn decode_empty_fails() {
        assert!(decode(&[], CodecFormat::J2k).is_err());
    }

    // -----------------------------------------------------------------------
    // Tests: encode → decode roundtrip
    // -----------------------------------------------------------------------

    #[test]
    fn encode_decode_roundtrip_jp2() {
        let params: Vec<_> = (0..3)
            .map(|_| ImageCompParam {
                dx: 1,
                dy: 1,
                w: 4,
                h: 4,
                x0: 0,
                y0: 0,
                prec: 8,
                sgnd: false,
            })
            .collect();
        let mut image = Image::new(&params, ColorSpace::Srgb);
        image.x0 = 0;
        image.y0 = 0;
        image.x1 = 4;
        image.y1 = 4;

        let encoded = encode(&image, CodecFormat::Jp2).unwrap();

        // Verify it's valid JP2
        assert_eq!(detect_format(&encoded), Some(CodecFormat::Jp2));

        // Decode and verify
        let decoded = decode(&encoded, CodecFormat::Jp2).unwrap();
        assert_eq!(decoded.x1, 4);
        assert_eq!(decoded.y1, 4);
        assert_eq!(decoded.comps.len(), 3);
        assert_eq!(decoded.color_space, ColorSpace::Srgb);
    }

    #[test]
    fn encode_j2k_produces_decodable_pixels() {
        // Encode a grayscale image with pixel data, then decode and verify
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
        image.x0 = 0;
        image.y0 = 0;
        image.x1 = 8;
        image.y1 = 8;
        // Fill with gradient
        image.comps[0].data = (0..64).map(|i| i * 4).collect();

        let encoded = encode(&image, CodecFormat::J2k).unwrap();
        assert!(encoded.len() > 10, "encoded data should be non-trivial");

        let decoded = decode(&encoded, CodecFormat::J2k).unwrap();
        assert_eq!(decoded.comps.len(), 1);
        assert_eq!(decoded.comps[0].data.len(), 64);

        // Decoded pixels should have a range (not all same value)
        let min = *decoded.comps[0].data.iter().min().unwrap();
        let max = *decoded.comps[0].data.iter().max().unwrap();
        assert!(max > min, "decoded pixels should vary");
    }

    #[test]
    fn encode_decode_roundtrip_jp2_with_pixels() {
        let params = vec![ImageCompParam {
            dx: 1,
            dy: 1,
            w: 4,
            h: 4,
            x0: 0,
            y0: 0,
            prec: 8,
            sgnd: false,
        }];
        let mut image = Image::new(&params, ColorSpace::Gray);
        image.x0 = 0;
        image.y0 = 0;
        image.x1 = 4;
        image.y1 = 4;
        image.comps[0].data = vec![100; 16]; // uniform pixel value

        let encoded = encode(&image, CodecFormat::Jp2).unwrap();
        let decoded = decode(&encoded, CodecFormat::Jp2).unwrap();

        assert_eq!(decoded.comps[0].data.len(), 16);
        // For uniform input, all decoded pixels should be the same
        let unique: std::collections::HashSet<i32> =
            decoded.comps[0].data.iter().copied().collect();
        assert_eq!(
            unique.len(),
            1,
            "uniform input should decode to uniform output"
        );
    }
}
