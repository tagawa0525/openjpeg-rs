// Phase 500d: J2K encoder
//
// Writes a J2K codestream: main header markers, tile-part headers, tile data.

use crate::error::Result;
use crate::image::Image;
use crate::j2k::markers::{
    patch_psot, write_cod, write_eoc, write_qcd, write_siz, write_soc, write_sod, write_sot,
};
use crate::j2k::params::{CodingParameters, TileCodingParameters};

/// J2K codestream encoder.
pub struct J2kEncoder {
    /// Output buffer (accumulated codestream bytes).
    pub output: Vec<u8>,
}

impl Default for J2kEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl J2kEncoder {
    /// Create a new J2K encoder.
    pub fn new() -> Self {
        Self { output: Vec::new() }
    }

    /// Write the main header (SOC + SIZ + COD + QCD).
    pub fn write_header(
        &mut self,
        image: &Image,
        cp: &CodingParameters,
        tcp: &TileCodingParameters,
    ) -> Result<()> {
        write_soc(&mut self.output);
        write_siz(&mut self.output, image, cp);
        write_cod(&mut self.output, tcp);
        write_qcd(&mut self.output, tcp);
        Ok(())
    }

    /// Write a tile-part: SOT + SOD + tile data. Patches Psot after writing.
    pub fn write_tile(
        &mut self,
        tile_no: u32,
        tile_data: &[u8],
        tp_idx: u8,
        nb_parts: u8,
    ) -> Result<()> {
        let psot_offset = write_sot(&mut self.output, tile_no, tp_idx, nb_parts);
        write_sod(&mut self.output);
        self.output.extend_from_slice(tile_data);

        // Patch Psot: SOT(12) + SOD(2) + tile_data.len()
        let psot = 12 + 2 + tile_data.len() as u32;
        patch_psot(&mut self.output, psot_offset, psot);

        Ok(())
    }

    /// Write EOC marker and return the complete codestream.
    pub fn finalize(mut self) -> Vec<u8> {
        write_eoc(&mut self.output);
        self.output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::ImageCompParam;
    use crate::io::cio::{MemoryStream, read_bytes_be};
    use crate::j2k::params::TileCompCodingParameters;
    use crate::j2k::read::J2kDecoder;
    use crate::types::ColorSpace;

    fn create_test_image_and_params() -> (Image, CodingParameters, TileCodingParameters) {
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

        let tccp = TileCompCodingParameters {
            numresolutions: 2,
            cblkw: 6,
            cblkh: 6,
            qmfbid: 1,
            ..Default::default()
        };
        let tcp = TileCodingParameters {
            numlayers: 1,
            tccps: vec![tccp],
            ..Default::default()
        };
        let cp = CodingParameters {
            tdx: 8,
            tdy: 8,
            tw: 1,
            th: 1,
            ..CodingParameters::new_encoder()
        };

        (image, cp, tcp)
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn encode_header_starts_with_soc_siz() {
        let (image, cp, tcp) = create_test_image_and_params();
        let mut enc = J2kEncoder::new();
        enc.write_header(&image, &cp, &tcp).unwrap();

        // SOC
        assert_eq!(read_bytes_be(&enc.output[0..], 2), 0xFF4F);
        // SIZ
        assert_eq!(read_bytes_be(&enc.output[2..], 2), 0xFF51);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn encode_tile_patches_psot() {
        let (image, cp, tcp) = create_test_image_and_params();
        let mut enc = J2kEncoder::new();
        enc.write_header(&image, &cp, &tcp).unwrap();

        let tile_data = vec![0xAB; 10];
        enc.write_tile(0, &tile_data, 0, 1).unwrap();

        // Find SOT marker in output
        let sot_pos = enc
            .output
            .windows(2)
            .position(|w| w == [0xFF, 0x90])
            .unwrap();
        // Psot is at sot_pos + 6
        let psot = read_bytes_be(&enc.output[sot_pos + 6..], 4);
        assert_eq!(psot, 12 + 2 + 10); // SOT(12) + SOD(2) + data(10)
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn encode_finalize_ends_with_eoc() {
        let (image, cp, tcp) = create_test_image_and_params();
        let mut enc = J2kEncoder::new();
        enc.write_header(&image, &cp, &tcp).unwrap();
        enc.write_tile(0, &[0u8; 4], 0, 1).unwrap();
        let output = enc.finalize();

        let len = output.len();
        assert_eq!(read_bytes_be(&output[len - 2..], 2), 0xFFD9);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn encode_decode_roundtrip_header() {
        let (image, cp, tcp) = create_test_image_and_params();
        let mut enc = J2kEncoder::new();
        enc.write_header(&image, &cp, &tcp).unwrap();
        enc.write_tile(0, &[0u8; 4], 0, 1).unwrap();
        let output = enc.finalize();

        // Decode the header
        let mut stream = MemoryStream::new_input(output);
        let mut dec = J2kDecoder::new();
        dec.read_header(&mut stream).unwrap();

        // Verify decoded values match
        assert_eq!(dec.image.x1, 8);
        assert_eq!(dec.image.y1, 8);
        assert_eq!(dec.image.comps.len(), 1);
        assert_eq!(dec.image.comps[0].prec, 8);
        assert_eq!(dec.cp.tdx, 8);
        assert_eq!(dec.cp.tdy, 8);
        assert_eq!(dec.cp.tw, 1);
        assert_eq!(dec.cp.th, 1);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn encode_decode_roundtrip_tiles() {
        let (image, cp, tcp) = create_test_image_and_params();
        let mut enc = J2kEncoder::new();
        enc.write_header(&image, &cp, &tcp).unwrap();
        let tile_data = vec![0x42; 16];
        enc.write_tile(0, &tile_data, 0, 1).unwrap();
        let output = enc.finalize();

        // Decode
        let mut stream = MemoryStream::new_input(output);
        let mut dec = J2kDecoder::new();
        dec.read_header(&mut stream).unwrap();
        dec.read_all_tiles(&mut stream).unwrap();

        assert_eq!(dec.tile_data.len(), 1);
        assert_eq!(dec.tile_data[0], tile_data);
    }
}
