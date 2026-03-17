// Phase 500b: J2K marker reading functions
//
// Parses individual JPEG 2000 markers from byte buffers.
// Each function takes a byte slice (marker payload after the 2-byte length field)
// and populates the corresponding parameter structures.

use crate::error::{Error, Result};
use crate::image::{Image, ImageComp};
use crate::io::cio::read_bytes_be;
use crate::j2k::params::{
    CodingParameters, Stepsize, TileCodingParameters, TileCompCodingParameters,
};
use crate::types::{
    J2K_CCP_CSTY_PRT, J2K_CCP_QNTSTY_NOQNT, J2K_CCP_QNTSTY_SEQNT, J2K_CCP_QNTSTY_SIQNT,
    J2K_MAXBANDS, J2K_MAXRLVLS, ProgressionOrder,
};

// ---------------------------------------------------------------------------
// SIZ marker (Image and tile size)
// ---------------------------------------------------------------------------

/// Parse SIZ marker data (C: opj_j2k_read_siz).
///
/// `data` is the marker payload (after the 2-byte Lsiz length field).
/// Populates `image` with dimensions/components and `cp` with tile grid info.
pub fn read_siz(data: &[u8], image: &mut Image, cp: &mut CodingParameters) -> Result<()> {
    if data.len() < 36 {
        return Err(Error::InvalidInput("SIZ marker too short".into()));
    }

    let mut pos = 0;
    let rsiz = read_bytes_be(&data[pos..], 2) as u16;
    pos += 2;
    let x1 = read_bytes_be(&data[pos..], 4);
    pos += 4;
    let y1 = read_bytes_be(&data[pos..], 4);
    pos += 4;
    let x0 = read_bytes_be(&data[pos..], 4);
    pos += 4;
    let y0 = read_bytes_be(&data[pos..], 4);
    pos += 4;
    let tdx = read_bytes_be(&data[pos..], 4);
    pos += 4;
    let tdy = read_bytes_be(&data[pos..], 4);
    pos += 4;
    let tx0 = read_bytes_be(&data[pos..], 4);
    pos += 4;
    let ty0 = read_bytes_be(&data[pos..], 4);
    pos += 4;
    let numcomps = read_bytes_be(&data[pos..], 2) as u16;
    pos += 2;

    // Validate
    if numcomps == 0 || numcomps > 16384 {
        return Err(Error::InvalidInput(format!(
            "SIZ: invalid number of components: {numcomps}"
        )));
    }
    if x1 <= x0 || y1 <= y0 {
        return Err(Error::InvalidInput("SIZ: invalid image dimensions".into()));
    }
    if tdx == 0 || tdy == 0 {
        return Err(Error::InvalidInput("SIZ: zero tile dimensions".into()));
    }

    let comp_data_len = numcomps as usize * 3;
    if data.len() < pos + comp_data_len {
        return Err(Error::InvalidInput("SIZ: not enough component data".into()));
    }

    // Store image parameters
    image.x0 = x0;
    image.y0 = y0;
    image.x1 = x1;
    image.y1 = y1;

    // Store coding parameters
    cp.rsiz = rsiz;
    cp.tx0 = tx0;
    cp.ty0 = ty0;
    cp.tdx = tdx;
    cp.tdy = tdy;

    // Compute tile grid (validate tx0/ty0 within image bounds)
    if tx0 > x1 || ty0 > y1 {
        return Err(Error::InvalidInput(
            "SIZ: tile origin beyond image extent".into(),
        ));
    }
    cp.tw = (x1 - tx0).div_ceil(tdx).max(1);
    cp.th = (y1 - ty0).div_ceil(tdy).max(1);

    // Parse components
    image.comps.clear();
    for _ in 0..numcomps {
        let ssiz = data[pos];
        pos += 1;
        let dx = data[pos] as u32;
        pos += 1;
        let dy = data[pos] as u32;
        pos += 1;

        if dx == 0 || dy == 0 {
            return Err(Error::InvalidInput("SIZ: zero subsampling factor".into()));
        }

        let prec = (ssiz & 0x7F) as u32 + 1;
        let sgnd = (ssiz >> 7) != 0;

        // Compute component dimensions from ceil-divided endpoints
        let comp_x0 = x0.div_ceil(dx);
        let comp_y0 = y0.div_ceil(dy);
        let comp_x1 = x1.div_ceil(dx);
        let comp_y1 = y1.div_ceil(dy);
        let w = comp_x1 - comp_x0;
        let h = comp_y1 - comp_y0;

        image.comps.push(ImageComp {
            dx,
            dy,
            w,
            h,
            x0: comp_x0,
            y0: comp_y0,
            prec,
            sgnd,
            resno_decoded: 0,
            factor: 0,
            data: Vec::new(),
            alpha: 0,
        });
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// COD marker (Coding style default)
// ---------------------------------------------------------------------------

/// Parse COD marker data (C: opj_j2k_read_cod).
///
/// `data` is the marker payload (after the 2-byte Lcod length field).
pub fn read_cod(data: &[u8], tcp: &mut TileCodingParameters, numcomps: u32) -> Result<()> {
    if data.len() < 5 {
        return Err(Error::InvalidInput("COD marker too short".into()));
    }

    let mut pos = 0;
    let scod = data[pos];
    pos += 1;
    let prg_byte = data[pos];
    pos += 1;
    let numlayers = read_bytes_be(&data[pos..], 2);
    pos += 2;
    let mct = data[pos];
    pos += 1;

    // Validate
    if prg_byte > 4 {
        return Err(Error::InvalidInput(format!(
            "COD: invalid progression order: {prg_byte}"
        )));
    }
    if numlayers == 0 || numlayers > 65535 {
        return Err(Error::InvalidInput(format!(
            "COD: invalid number of layers: {numlayers}"
        )));
    }
    if mct > 1 {
        return Err(Error::InvalidInput(format!(
            "COD: invalid MCT value: {mct}"
        )));
    }

    tcp.csty = scod as u32;
    tcp.prg = match prg_byte {
        0 => ProgressionOrder::Lrcp,
        1 => ProgressionOrder::Rlcp,
        2 => ProgressionOrder::Rpcl,
        3 => ProgressionOrder::Pcrl,
        4 => ProgressionOrder::Cprl,
        _ => unreachable!(),
    };
    tcp.numlayers = numlayers;
    tcp.num_layers_to_decode = numlayers;
    tcp.mct = mct as u32;

    // Ensure TCCPs exist for all components
    if tcp.tccps.len() < numcomps as usize {
        tcp.tccps
            .resize(numcomps as usize, TileCompCodingParameters::default());
    }

    // Read SPCod into all TCCPs (COD applies to all components)
    let remaining = &data[pos..];
    for tccp in tcp.tccps.iter_mut() {
        read_spcod_spcoc(remaining, tccp, scod as u32)?;
    }

    Ok(())
}

/// Parse SPCod/SPCoc parameters (C: opj_j2k_read_SPCod_SPCoc).
pub fn read_spcod_spcoc(data: &[u8], tccp: &mut TileCompCodingParameters, csty: u32) -> Result<()> {
    if data.len() < 5 {
        return Err(Error::InvalidInput("SPCod/SPCoc too short".into()));
    }

    let num_decomp = data[0] as u32;
    let cblkw_raw = data[1] as u32;
    let cblkh_raw = data[2] as u32;
    let cblksty = data[3] as u32;
    let qmfbid = data[4] as u32;

    let numresolutions = num_decomp + 1;
    if numresolutions as usize > J2K_MAXRLVLS {
        return Err(Error::InvalidInput(format!(
            "SPCod: too many decomposition levels: {num_decomp}"
        )));
    }

    let cblkw = cblkw_raw + 2;
    let cblkh = cblkh_raw + 2;
    if cblkw > 10 || cblkh > 10 || cblkw + cblkh > 12 {
        return Err(Error::InvalidInput(format!(
            "SPCod: invalid codeblock size: w={cblkw}, h={cblkh}"
        )));
    }

    if qmfbid > 1 {
        return Err(Error::InvalidInput(format!(
            "SPCod: invalid wavelet filter: {qmfbid}"
        )));
    }

    tccp.numresolutions = numresolutions;
    tccp.cblkw = cblkw;
    tccp.cblkh = cblkh;
    tccp.cblksty = cblksty;
    tccp.qmfbid = qmfbid;

    // Read precinct sizes if PRT flag is set
    if (csty & J2K_CCP_CSTY_PRT) != 0 {
        let prc_data = &data[5..];
        if prc_data.len() < numresolutions as usize {
            return Err(Error::InvalidInput(
                "SPCod: not enough precinct data".into(),
            ));
        }
        for (i, &byte) in prc_data.iter().enumerate().take(numresolutions as usize) {
            tccp.prcw[i] = (byte & 0x0F) as u32;
            tccp.prch[i] = (byte >> 4) as u32;
        }
    } else {
        // Default: maximum precinct size
        for i in 0..numresolutions as usize {
            tccp.prcw[i] = 15;
            tccp.prch[i] = 15;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// QCD marker (Quantization default)
// ---------------------------------------------------------------------------

/// Parse QCD marker data (C: opj_j2k_read_qcd).
///
/// `data` is the marker payload (after the 2-byte Lqcd length field).
pub fn read_qcd(data: &[u8], tcp: &mut TileCodingParameters, numcomps: u32) -> Result<()> {
    // Ensure TCCPs exist
    if tcp.tccps.len() < numcomps as usize {
        tcp.tccps
            .resize(numcomps as usize, TileCompCodingParameters::default());
    }

    // Read SQcd into all TCCPs (QCD applies to all components)
    for tccp in tcp.tccps.iter_mut() {
        read_sqcd_sqcc(data, tccp)?;
    }

    Ok(())
}

/// Parse SQcd/SQcc quantization parameters (C: opj_j2k_read_SQcd_SQcc).
pub fn read_sqcd_sqcc(data: &[u8], tccp: &mut TileCompCodingParameters) -> Result<()> {
    if data.is_empty() {
        return Err(Error::InvalidInput("SQcd/SQcc: empty data".into()));
    }

    let sqcx = data[0];
    let qntsty = (sqcx & 0x1F) as u32;
    let numgbits = (sqcx >> 5) as u32;

    tccp.qntsty = qntsty;
    tccp.numgbits = numgbits;

    let remaining = &data[1..];

    match qntsty {
        J2K_CCP_QNTSTY_NOQNT => {
            // No quantization: 1 byte per subband (exponent only)
            let num_bands = remaining.len().min(J2K_MAXBANDS);
            for (i, &byte) in remaining.iter().enumerate().take(num_bands) {
                tccp.stepsizes[i] = Stepsize {
                    expn: (byte >> 3) as i32,
                    mant: 0,
                };
            }
        }
        J2K_CCP_QNTSTY_SIQNT => {
            // Scalar implicit: single 2-byte entry, derive rest
            if remaining.len() < 2 {
                return Err(Error::InvalidInput(
                    "SQcd: SIQNT needs at least 2 bytes".into(),
                ));
            }
            let val = read_bytes_be(remaining, 2);
            let expn0 = (val >> 11) as i32;
            let mant0 = (val & 0x7FF) as i32;
            tccp.stepsizes[0] = Stepsize {
                expn: expn0,
                mant: mant0,
            };
            // Derive remaining subbands
            for i in 1..J2K_MAXBANDS {
                let band_idx = (i - 1) / 3;
                tccp.stepsizes[i] = Stepsize {
                    expn: (expn0 - band_idx as i32).max(0),
                    mant: mant0,
                };
            }
        }
        J2K_CCP_QNTSTY_SEQNT => {
            // Scalar explicit: 2 bytes per subband
            if !remaining.len().is_multiple_of(2) {
                return Err(Error::InvalidInput(
                    "SQcd: SEQNT data must be even length".into(),
                ));
            }
            let num_bands = (remaining.len() / 2).min(J2K_MAXBANDS);
            for i in 0..num_bands {
                let val = read_bytes_be(&remaining[i * 2..], 2);
                tccp.stepsizes[i] = Stepsize {
                    expn: (val >> 11) as i32,
                    mant: (val & 0x7FF) as i32,
                };
            }
        }
        _ => {
            return Err(Error::InvalidInput(format!(
                "SQcd: unsupported quantization style: {qntsty}"
            )));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// COM marker (Comment)
// ---------------------------------------------------------------------------

/// Parse COM marker data. Returns (registration_value, comment_bytes).
pub fn read_com(data: &[u8]) -> Result<(u16, Vec<u8>)> {
    if data.len() < 2 {
        return Err(Error::InvalidInput("COM marker too short".into()));
    }
    let rcom = read_bytes_be(data, 2) as u16;
    let comment = data[2..].to_vec();
    Ok((rcom, comment))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- SIZ ---

    fn make_siz_data() -> Vec<u8> {
        let mut d = Vec::new();
        // Rsiz
        d.extend_from_slice(&0x0000u16.to_be_bytes());
        // Xsiz=64, Ysiz=64
        d.extend_from_slice(&64u32.to_be_bytes());
        d.extend_from_slice(&64u32.to_be_bytes());
        // X0siz=0, Y0siz=0
        d.extend_from_slice(&0u32.to_be_bytes());
        d.extend_from_slice(&0u32.to_be_bytes());
        // XTsiz=64, YTsiz=64
        d.extend_from_slice(&64u32.to_be_bytes());
        d.extend_from_slice(&64u32.to_be_bytes());
        // XT0siz=0, YT0siz=0
        d.extend_from_slice(&0u32.to_be_bytes());
        d.extend_from_slice(&0u32.to_be_bytes());
        // Csiz=1
        d.extend_from_slice(&1u16.to_be_bytes());
        // Component 0: prec=8 unsigned, dx=1, dy=1
        d.push(0x07); // Ssiz: (8-1) = 7, unsigned
        d.push(0x01); // XRsiz
        d.push(0x01); // YRsiz
        d
    }

    #[test]
    fn siz_basic_parsing() {
        let data = make_siz_data();
        let mut image = Image::new_tile(&[], crate::types::ColorSpace::Unknown);
        let mut cp = CodingParameters::new_encoder();

        read_siz(&data, &mut image, &mut cp).unwrap();

        assert_eq!(image.x0, 0);
        assert_eq!(image.y0, 0);
        assert_eq!(image.x1, 64);
        assert_eq!(image.y1, 64);
        assert_eq!(image.comps.len(), 1);
        assert_eq!(image.comps[0].prec, 8);
        assert!(!image.comps[0].sgnd);
        assert_eq!(image.comps[0].dx, 1);
        assert_eq!(image.comps[0].dy, 1);
        assert_eq!(image.comps[0].w, 64);
        assert_eq!(image.comps[0].h, 64);
        assert_eq!(cp.tdx, 64);
        assert_eq!(cp.tdy, 64);
        assert_eq!(cp.tw, 1);
        assert_eq!(cp.th, 1);
    }

    #[test]
    fn siz_rejects_too_short() {
        let data = vec![0u8; 10];
        let mut image = Image::new_tile(&[], crate::types::ColorSpace::Unknown);
        let mut cp = CodingParameters::new_encoder();
        assert!(read_siz(&data, &mut image, &mut cp).is_err());
    }

    // --- COD ---

    fn make_cod_data() -> Vec<u8> {
        let mut d = Vec::new();
        d.push(0x00); // Scod: no precincts, no SOP/EPH
        d.push(0x00); // SGcod_A: LRCP
        d.extend_from_slice(&1u16.to_be_bytes()); // SGcod_B: 1 layer
        d.push(0x00); // SGcod_C: no MCT
        // SPCod:
        d.push(0x01); // NumDecomp: 1 → numresolutions=2
        d.push(0x04); // Cblkw: 4 → actual=6 (2^6=64)
        d.push(0x04); // Cblkh: 4 → actual=6
        d.push(0x00); // Cblksty: default
        d.push(0x01); // Qmfbid: 5-3 reversible
        d
    }

    #[test]
    fn cod_basic_parsing() {
        let data = make_cod_data();
        let mut tcp = TileCodingParameters::default();

        read_cod(&data, &mut tcp, 1).unwrap();

        assert_eq!(tcp.csty, 0);
        assert_eq!(tcp.prg, ProgressionOrder::Lrcp);
        assert_eq!(tcp.numlayers, 1);
        assert_eq!(tcp.mct, 0);
        assert_eq!(tcp.tccps.len(), 1);
        assert_eq!(tcp.tccps[0].numresolutions, 2);
        assert_eq!(tcp.tccps[0].cblkw, 6);
        assert_eq!(tcp.tccps[0].cblkh, 6);
        assert_eq!(tcp.tccps[0].qmfbid, 1);
        // Default precincts (no PRT flag)
        assert_eq!(tcp.tccps[0].prcw[0], 15);
        assert_eq!(tcp.tccps[0].prch[0], 15);
    }

    #[test]
    fn cod_with_precincts() {
        let mut data = Vec::new();
        data.push(0x01); // Scod: PRT (precincts defined)
        data.push(0x01); // SGcod_A: RLCP
        data.extend_from_slice(&2u16.to_be_bytes()); // 2 layers
        data.push(0x01); // MCT
        // SPCod:
        data.push(0x01); // NumDecomp: 1 → 2 resolutions
        data.push(0x04); // Cblkw
        data.push(0x04); // Cblkh
        data.push(0x00); // Cblksty
        data.push(0x01); // Qmfbid
        // Precincts: 2 resolutions
        data.push(0x55); // Res 0: prcw=5, prch=5
        data.push(0x66); // Res 1: prcw=6, prch=6

        let mut tcp = TileCodingParameters::default();
        read_cod(&data, &mut tcp, 1).unwrap();

        assert_eq!(tcp.prg, ProgressionOrder::Rlcp);
        assert_eq!(tcp.numlayers, 2);
        assert_eq!(tcp.mct, 1);
        assert_eq!(tcp.tccps[0].prcw[0], 5);
        assert_eq!(tcp.tccps[0].prch[0], 5);
        assert_eq!(tcp.tccps[0].prcw[1], 6);
        assert_eq!(tcp.tccps[0].prch[1], 6);
    }

    // --- QCD ---

    #[test]
    fn qcd_noqnt() {
        // NOQNT: 1 byte per subband
        let data = vec![
            0x40, // Sqcx: qntsty=0 (NOQNT), numgbits=2
            0x40, // Band 0: expn = 8
            0x48, // Band 1: expn = 9
            0x48, // Band 2: expn = 9
            0x50, // Band 3: expn = 10
        ];

        let mut tcp = TileCodingParameters::default();
        read_qcd(&data, &mut tcp, 1).unwrap();

        assert_eq!(tcp.tccps[0].qntsty, J2K_CCP_QNTSTY_NOQNT);
        assert_eq!(tcp.tccps[0].numgbits, 2);
        assert_eq!(tcp.tccps[0].stepsizes[0].expn, 8);
        assert_eq!(tcp.tccps[0].stepsizes[0].mant, 0);
        assert_eq!(tcp.tccps[0].stepsizes[1].expn, 9);
        assert_eq!(tcp.tccps[0].stepsizes[3].expn, 10);
    }

    #[test]
    fn qcd_seqnt() {
        // SEQNT: 2 bytes per subband
        let mut data = Vec::new();
        data.push(0x42); // Sqcx: qntsty=2 (SEQNT), numgbits=2
        // Band 0: expn=8, mant=100
        let val0 = (8u16 << 11) | 100;
        data.extend_from_slice(&val0.to_be_bytes());
        // Band 1: expn=7, mant=200
        let val1 = (7u16 << 11) | 200;
        data.extend_from_slice(&val1.to_be_bytes());

        let mut tcp = TileCodingParameters::default();
        read_qcd(&data, &mut tcp, 1).unwrap();

        assert_eq!(tcp.tccps[0].qntsty, J2K_CCP_QNTSTY_SEQNT);
        assert_eq!(tcp.tccps[0].stepsizes[0].expn, 8);
        assert_eq!(tcp.tccps[0].stepsizes[0].mant, 100);
        assert_eq!(tcp.tccps[0].stepsizes[1].expn, 7);
        assert_eq!(tcp.tccps[0].stepsizes[1].mant, 200);
    }

    #[test]
    fn qcd_siqnt() {
        // SIQNT: single 2-byte entry, rest derived
        let mut data = Vec::new();
        data.push(0x41); // Sqcx: qntsty=1 (SIQNT), numgbits=2
        let val = (10u16 << 11) | 500;
        data.extend_from_slice(&val.to_be_bytes());

        let mut tcp = TileCodingParameters::default();
        read_qcd(&data, &mut tcp, 1).unwrap();

        assert_eq!(tcp.tccps[0].qntsty, J2K_CCP_QNTSTY_SIQNT);
        assert_eq!(tcp.tccps[0].stepsizes[0].expn, 10);
        assert_eq!(tcp.tccps[0].stepsizes[0].mant, 500);
        // Derived: band 1-3 same expn (index 0), band 4-6 expn-1, etc.
        assert_eq!(tcp.tccps[0].stepsizes[1].expn, 10); // (1-1)/3 = 0 → expn-0 = 10
        assert_eq!(tcp.tccps[0].stepsizes[4].expn, 9); // (4-1)/3 = 1 → expn-1 = 9
    }

    // --- COM ---

    #[test]
    fn com_parsing() {
        let mut data = Vec::new();
        data.extend_from_slice(&1u16.to_be_bytes()); // Rcom=1 (Latin text)
        data.extend_from_slice(b"hello");

        let (rcom, comment) = read_com(&data).unwrap();
        assert_eq!(rcom, 1);
        assert_eq!(comment, b"hello");
    }
}
