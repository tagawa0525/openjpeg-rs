// Phase 400c: Tier-2 packet encoding/decoding (C: t2.c)
//
// Encodes and decodes packets: the basic unit of a JPEG 2000 codestream.
// Each packet contains header (tag tree inclusion, zero-bitplane info,
// pass counts, segment lengths) and body (code block compressed data).

use crate::error::{Error, Result};
use crate::io::bio::Bio;
use crate::tcd::{TcdCodeBlocks, TcdSeg, TcdSegDataChunk, TcdTile};
use crate::types::{J2K_CCP_CBLKSTY_LAZY, J2K_CCP_CBLKSTY_TERMALL, uint_floorlog2};

// ---------------------------------------------------------------------------
// Comma code (unary coding)
// ---------------------------------------------------------------------------

/// Encode a value as a comma code (unary): `n` ones followed by a zero.
/// (C: opj_t2_putcommacode)
pub fn t2_putcommacode(bio: &mut Bio, n: u32) -> Result<()> {
    for _ in 0..n {
        bio.write(1, 1)?;
    }
    bio.write(0, 1)?;
    Ok(())
}

/// Decode a comma code (unary): count ones until a zero is read.
/// (C: opj_t2_getcommacode)
pub fn t2_getcommacode(bio: &mut Bio) -> Result<u32> {
    let mut n = 0u32;
    loop {
        let bit = bio.read(1)?;
        if bit == 0 {
            break;
        }
        n += 1;
    }
    Ok(n)
}

// ---------------------------------------------------------------------------
// Number of passes (variable-length coding per JPEG 2000 spec)
// ---------------------------------------------------------------------------

/// Encode the number of coding passes with JPEG 2000 variable-length code.
/// (C: opj_t2_putnumpasses)
///
/// Encoding table:
/// - 1     → `0`              (1 bit)
/// - 2     → `10`             (2 bits)
/// - 3-5   → `11` + 2 bits    (4 bits)
/// - 6-36  → `1111` + 5 bits  (9 bits)
/// - 37-164→ `1111111` + 7 bits (16 bits, split as 9+7)
pub fn t2_putnumpasses(bio: &mut Bio, n: u32) -> Result<()> {
    if n == 1 {
        bio.write(0, 1)?;
    } else if n == 2 {
        bio.write(2, 2)?;
    } else if n <= 5 {
        bio.write(0xc | (n - 3), 4)?;
    } else if n <= 36 {
        bio.write(0x1e0 | (n - 6), 9)?;
    } else if n <= 164 {
        bio.write(0xff80 | (n - 37), 16)?;
    }
    Ok(())
}

/// Decode the number of coding passes from JPEG 2000 variable-length code.
/// (C: opj_t2_getnumpasses)
pub fn t2_getnumpasses(bio: &mut Bio) -> Result<u32> {
    if bio.read(1)? == 0 {
        return Ok(1);
    }
    if bio.read(1)? == 0 {
        return Ok(2);
    }
    let n = bio.read(2)?;
    if n != 3 {
        return Ok(3 + n);
    }
    let n = bio.read(5)?;
    if n != 31 {
        return Ok(6 + n);
    }
    Ok(37 + bio.read(7)?)
}

// ---------------------------------------------------------------------------
// Segment initialization
// ---------------------------------------------------------------------------

/// Initialize a decoding segment for a code block.
/// (C: opj_t2_init_seg)
///
/// Sets the segment's `maxpasses` based on the code block style:
/// - TERMALL: 1 pass per segment
/// - LAZY (bypass): alternates between 10 and 2/1 passes
/// - Default: 109 passes (per spec: (37-1)*3+1)
pub fn t2_init_seg(segs: &mut Vec<TcdSeg>, index: usize, cblksty: u32, first: bool) {
    // Ensure capacity
    while segs.len() <= index {
        segs.push(TcdSeg::default());
    }

    // Read previous segment's maxpasses before taking a mutable borrow
    let prev_maxpasses = if index > 0 && index <= segs.len() {
        segs[index - 1].maxpasses
    } else {
        0
    };

    let seg = &mut segs[index];
    seg.len = 0;
    seg.numpasses = 0;
    seg.real_num_passes = 0;
    seg.numnewpasses = 0;
    seg.newlen = 0;

    if (cblksty & J2K_CCP_CBLKSTY_TERMALL) != 0 {
        seg.maxpasses = 1;
    } else if (cblksty & J2K_CCP_CBLKSTY_LAZY) != 0 {
        if first {
            seg.maxpasses = 10;
        } else {
            seg.maxpasses = if prev_maxpasses == 1 || prev_maxpasses == 10 {
                2
            } else {
                1
            };
        }
    } else {
        seg.maxpasses = 109; // (37-1)*3+1 per JPEG 2000 spec
    }
}

/// Returns the number of bits needed to represent `numpasses` length indicators.
/// (C: inline in opj_t2_read_packet_header and opj_t2_encode_packet)
#[inline]
pub fn t2_getpassbits(numpasses: u32) -> u32 {
    if numpasses < 2 {
        1
    } else if numpasses < 6 {
        2
    } else if numpasses < 37 {
        4
    } else if numpasses < 165 {
        6
    } else {
        8
    }
}

// ---------------------------------------------------------------------------
// Packet decode (C: opj_t2_read_packet_header / opj_t2_read_packet_data)
// ---------------------------------------------------------------------------

/// Read a packet header and update code block state.
/// Returns `(data_present, header_bytes_read)`.
/// (C: opj_t2_read_packet_header)
///
/// Parses inclusion/IMSB tag trees, number of coding passes, length bits,
/// and segment lengths for each code block in the packet's precinct.
pub fn t2_read_packet_header(
    tile: &mut TcdTile,
    compno: u32,
    resno: u32,
    precno: u32,
    layno: u32,
    cblksty: u32,
    data: &mut [u8],
) -> Result<(bool, usize)> {
    if data.is_empty() {
        return Err(Error::EndOfStream);
    }

    let comp = tile
        .comps
        .get_mut(compno as usize)
        .ok_or_else(|| Error::InvalidInput(format!("compno {compno} out of range")))?;
    let res = comp
        .resolutions
        .get_mut(resno as usize)
        .ok_or_else(|| Error::InvalidInput(format!("resno {resno} out of range")))?;

    // On first layer, reset tag trees and code block state
    if layno == 0 {
        for band in &mut res.bands {
            if band.is_empty() {
                continue;
            }
            let prec = match band.precincts.get_mut(precno as usize) {
                Some(p) => p,
                None => continue,
            };
            if let Some(ref mut incl) = prec.incltree {
                incl.reset();
            }
            if let Some(ref mut imsb) = prec.imsbtree {
                imsb.reset();
            }
            if let TcdCodeBlocks::Dec(ref mut cblks) = prec.cblks {
                for cblk in cblks.iter_mut() {
                    cblk.numsegs = 0;
                    cblk.real_num_segs = 0;
                }
            }
        }
    }

    let mut bio = Bio::decoder(data);

    // Read present bit
    let present = bio.read(1)?;
    if present == 0 {
        bio.inalign()?;
        return Ok((false, bio.num_bytes()));
    }

    // Process each band's code blocks
    let numbands = res.numbands;
    for bandno in 0..numbands {
        let band = &mut res.bands[bandno as usize];
        if band.is_empty() {
            continue;
        }
        let band_numbps = band.numbps;
        let prec = band
            .precincts
            .get_mut(precno as usize)
            .ok_or_else(|| Error::InvalidInput(format!("precno {precno} out of range")))?;
        let num_cblks = (prec.cw * prec.ch) as usize;

        for cblkno in 0..num_cblks {
            // --- Inclusion ---
            let numsegs = match &prec.cblks {
                TcdCodeBlocks::Dec(cblks) => cblks[cblkno].numsegs,
                _ => 0,
            };
            let included = if numsegs == 0 {
                // First inclusion: use tag tree
                let incltree = prec
                    .incltree
                    .as_mut()
                    .ok_or_else(|| Error::InvalidInput("missing inclusion tag tree".into()))?;
                incltree.decode(&mut bio, cblkno as u32, (layno + 1) as i32)? != 0
            } else {
                // Already included: read 1 bit
                bio.read(1)? != 0
            };

            if !included {
                if let TcdCodeBlocks::Dec(ref mut cblks) = prec.cblks {
                    cblks[cblkno].numnewpasses = 0;
                }
                continue;
            }

            // --- IMSB (first inclusion only) ---
            if numsegs == 0 {
                let imsbtree = prec
                    .imsbtree
                    .as_mut()
                    .ok_or_else(|| Error::InvalidInput("missing IMSB tag tree".into()))?;
                // IMSB max is bounded by band bit-planes + 1 (JPEG 2000 T.800 B.10.5)
                let imsb_max = (band_numbps as u32).saturating_add(2);
                let mut i = 0u32;
                while imsbtree.decode(&mut bio, cblkno as u32, i as i32)? == 0 {
                    i += 1;
                    if i > imsb_max {
                        return Err(Error::InvalidInput(format!(
                            "IMSB value {i} exceeds band bit-planes {band_numbps}"
                        )));
                    }
                }
                let numbps = (band_numbps as u32 + 1).saturating_sub(i);
                if let TcdCodeBlocks::Dec(ref mut cblks) = prec.cblks {
                    cblks[cblkno].numbps = numbps;
                    cblks[cblkno].numlenbits = 3;
                }
            }

            // --- Number of new passes ---
            let numnewpasses = t2_getnumpasses(&mut bio)?;

            // --- Length bits increment ---
            let increment = t2_getcommacode(&mut bio)?;

            if let TcdCodeBlocks::Dec(ref mut cblks) = prec.cblks {
                let cblk = &mut cblks[cblkno];
                cblk.numnewpasses = numnewpasses;
                cblk.numlenbits = cblk.numlenbits.checked_add(increment).ok_or_else(|| {
                    Error::InvalidInput("numlenbits overflow from corrupted input".into())
                })?;
                if cblk.numlenbits > 32 {
                    return Err(Error::InvalidInput(format!(
                        "numlenbits {} exceeds 32",
                        cblk.numlenbits
                    )));
                }

                // Initialize first segment if needed
                if cblk.numsegs == 0 {
                    t2_init_seg(&mut cblk.segs, 0, cblksty, true);
                    cblk.numsegs = 1;
                } else {
                    let seg_idx = (cblk.numsegs - 1) as usize;
                    let seg_full = cblk.segs[seg_idx].numpasses == cblk.segs[seg_idx].maxpasses;
                    if seg_full {
                        let new_idx = cblk.numsegs as usize;
                        t2_init_seg(&mut cblk.segs, new_idx, cblksty, false);
                        cblk.numsegs += 1;
                    }
                }

                // Read segment lengths
                let mut remaining_passes = numnewpasses;
                let mut seg_idx = (cblk.numsegs - 1) as usize;
                while remaining_passes > 0 {
                    let seg = &mut cblk.segs[seg_idx];
                    seg.numnewpasses = remaining_passes.min(seg.maxpasses - seg.numpasses);
                    let bit_number = cblk.numlenbits + uint_floorlog2(seg.numnewpasses);
                    if bit_number > 32 {
                        return Err(Error::InvalidInput(
                            "segment length bit count exceeds 32".into(),
                        ));
                    }
                    seg.newlen = bio.read(bit_number)?;
                    remaining_passes -= seg.numnewpasses;
                    if remaining_passes > 0 {
                        seg_idx += 1;
                        t2_init_seg(&mut cblk.segs, seg_idx, cblksty, false);
                        cblk.numsegs = (seg_idx + 1) as u32;
                    }
                }
            }
        }
    }

    bio.inalign()?;
    Ok((true, bio.num_bytes()))
}

/// Read packet body data and accumulate into code block segments.
/// Returns the number of bytes consumed.
/// (C: opj_t2_read_packet_data)
pub fn t2_read_packet_data(
    tile: &mut TcdTile,
    compno: u32,
    resno: u32,
    precno: u32,
    data: &mut [u8],
) -> Result<usize> {
    let comp = tile
        .comps
        .get_mut(compno as usize)
        .ok_or_else(|| Error::InvalidInput(format!("compno {compno} out of range")))?;
    let res = comp
        .resolutions
        .get_mut(resno as usize)
        .ok_or_else(|| Error::InvalidInput(format!("resno {resno} out of range")))?;
    let mut offset = 0usize;

    for bandno in 0..res.numbands {
        let band = &mut res.bands[bandno as usize];
        if band.is_empty() {
            continue;
        }
        let prec = band
            .precincts
            .get_mut(precno as usize)
            .ok_or_else(|| Error::InvalidInput(format!("precno {precno} out of range")))?;
        let num_cblks = (prec.cw * prec.ch) as usize;

        if let TcdCodeBlocks::Dec(ref mut cblks) = prec.cblks {
            for cblk in cblks.iter_mut().take(num_cblks) {
                if cblk.numnewpasses == 0 {
                    continue;
                }

                let mut remaining_passes = cblk.numnewpasses;
                let start_seg = if cblk.numsegs == 0 {
                    0
                } else {
                    cblk.numsegs - 1
                };
                let mut seg_idx = start_seg as usize;

                while remaining_passes > 0 && seg_idx < cblk.segs.len() {
                    let seg = &cblk.segs[seg_idx];
                    if seg.numnewpasses == 0 {
                        seg_idx += 1;
                        continue;
                    }
                    let newlen = seg.newlen as usize;
                    if offset + newlen > data.len() {
                        cblk.corrupted = true;
                        return Err(Error::EndOfStream);
                    }

                    // Store data chunk
                    cblk.chunks.push(TcdSegDataChunk {
                        data: data[offset..offset + newlen].to_vec(),
                        len: newlen as u32,
                    });
                    offset += newlen;

                    // Update segment
                    let seg = &mut cblk.segs[seg_idx];
                    seg.len += seg.newlen;
                    seg.numpasses += seg.numnewpasses;
                    seg.real_num_passes += seg.numnewpasses;
                    remaining_passes -= seg.numnewpasses;
                    seg.numnewpasses = 0;
                    seg.newlen = 0;

                    seg_idx += 1;
                }
                cblk.real_num_segs = cblk.numsegs;
                cblk.numnewpasses = 0;
            }
        }
    }

    Ok(offset)
}

/// Decode a single packet (header + data).
/// Returns total bytes consumed.
/// (C: opj_t2_decode_packet)
/// Decode all packets for a tile (C: opj_t2_decode_packets).
///
/// Iterates through packets using PacketIterators and decodes each one.
pub fn t2_decode_packets(
    tile: &mut TcdTile,
    tcp: &crate::j2k::params::TileCodingParameters,
    pis: &mut crate::tier2::pi::PacketIterators,
    data: &mut [u8],
    max_layers: u32,
) -> Result<usize> {
    let mut total_read = 0usize;

    for pino in 0..pis.len() {
        while pis.next(pino) {
            let pi = pis.get(pino);
            let layno = pi.layno;
            let compno = pi.compno;
            let resno = pi.resno;
            let precno = pi.precno;

            // Skip layers beyond the decode limit
            if layno >= max_layers {
                continue;
            }

            // Get cblksty from TCCP — error if missing (inconsistent setup)
            let cblksty = tcp
                .tccps
                .get(compno as usize)
                .ok_or_else(|| Error::InvalidInput(format!("missing TCCP for component {compno}")))?
                .cblksty;

            if total_read >= data.len() {
                return Err(Error::EndOfStream);
            }

            let bytes = t2_decode_packet(
                tile,
                compno,
                resno,
                precno,
                layno,
                cblksty,
                &mut data[total_read..],
            )?;
            total_read += bytes;
        }
    }

    Ok(total_read)
}

// ---------------------------------------------------------------------------
// Packet encode (C: opj_t2_encode_packet / opj_t2_encode_packets)
// ---------------------------------------------------------------------------

/// Encode a single packet (header + body) for one (compno, resno, precno, layno).
/// Returns the number of bytes written.
/// (C: opj_t2_encode_packet)
pub fn t2_encode_packet(
    tile: &mut TcdTile,
    compno: u32,
    resno: u32,
    precno: u32,
    layno: u32,
    cblksty: u32,
    dest: &mut [u8],
) -> Result<usize> {
    let comp = tile
        .comps
        .get_mut(compno as usize)
        .ok_or_else(|| Error::InvalidInput(format!("compno {compno} out of range")))?;
    let res = comp
        .resolutions
        .get_mut(resno as usize)
        .ok_or_else(|| Error::InvalidInput(format!("resno {resno} out of range")))?;

    // --- On first layer, reset tag trees and cblk state ---
    if layno == 0 {
        for band in &mut res.bands {
            if band.is_empty() {
                continue;
            }
            let prec = match band.precincts.get_mut(precno as usize) {
                Some(p) => p,
                None => continue,
            };
            if let Some(ref mut incl) = prec.incltree {
                incl.reset();
            }
            if let Some(ref mut imsb) = prec.imsbtree {
                imsb.reset();
            }
            let band_numbps = band.numbps;
            if let TcdCodeBlocks::Enc(ref mut cblks) = prec.cblks {
                for (cblkno, cblk) in cblks.iter_mut().enumerate() {
                    cblk.numpasses = 0;
                    cblk.numlenbits = 3;
                    // Set IMSB value: band_numbps - cblk_numbps (C: opj_tgt_setvalue)
                    if let Some(ref mut imsb) = prec.imsbtree {
                        imsb.set_value(
                            cblkno as u32,
                            (band_numbps as u32).saturating_sub(cblk.numbps) as i32,
                        );
                    }
                }
            }
        }
    }

    // --- Check if any cblk has data in this layer ---
    let numbands = res.numbands;
    let mut packet_empty = true;
    for bandno in 0..numbands {
        let band = &res.bands[bandno as usize];
        if band.is_empty() {
            continue;
        }
        if let Some(prec) = band.precincts.get(precno as usize) {
            if let TcdCodeBlocks::Enc(ref cblks) = prec.cblks {
                let num_cblks = (prec.cw * prec.ch) as usize;
                for cblk in cblks.iter().take(num_cblks) {
                    if cblk
                        .layers
                        .get(layno as usize)
                        .is_some_and(|l| l.numpasses > 0)
                    {
                        packet_empty = false;
                        break;
                    }
                }
            }
            if !packet_empty {
                break;
            }
        }
    }

    // --- Write packet header via BIO ---
    let header_len;
    {
        let mut bio = Bio::encoder(dest);

        if packet_empty {
            // Empty packet: present bit = 0
            bio.write(0, 1)?;
            bio.flush()?;
            return Ok(bio.num_bytes());
        }

        // Present bit = 1
        bio.write(1, 1)?;

        // Re-borrow res after mutable phase
        let comp = &mut tile.comps[compno as usize];
        let res = &mut comp.resolutions[resno as usize];

        // Process each band's code blocks
        for bandno in 0..numbands {
            let band = &mut res.bands[bandno as usize];
            if band.is_empty() {
                continue;
            }
            let prec = match band.precincts.get_mut(precno as usize) {
                Some(p) => p,
                None => continue,
            };
            let num_cblks = (prec.cw * prec.ch) as usize;

            if let TcdCodeBlocks::Enc(ref mut cblks) = prec.cblks {
                #[allow(clippy::needless_range_loop)]
                for cblkno in 0..num_cblks {
                    let cblk = &cblks[cblkno];
                    let layer = match cblk.layers.get(layno as usize) {
                        Some(l) => l,
                        None => continue,
                    };

                    // --- Inclusion ---
                    if cblk.numpasses == 0 {
                        // First inclusion: use tag tree
                        if let Some(ref mut incltree) = prec.incltree {
                            incltree.set_value(
                                cblkno as u32,
                                if layer.numpasses > 0 {
                                    layno as i32
                                } else {
                                    (layno + 1) as i32
                                },
                            );
                            incltree.encode(&mut bio, cblkno as u32, (layno + 1) as i32)?;
                        }
                    } else {
                        // Already included: write 1 bit
                        bio.write(if layer.numpasses > 0 { 1 } else { 0 }, 1)?;
                    }

                    if layer.numpasses == 0 {
                        continue;
                    }

                    // --- IMSB (first inclusion only) ---
                    if cblk.numpasses == 0
                        && let Some(ref mut imsbtree) = prec.imsbtree
                    {
                        imsbtree.encode(&mut bio, cblkno as u32, 999)?;
                    }

                    // --- Number of passes ---
                    t2_putnumpasses(&mut bio, layer.numpasses)?;

                    let cblk = &cblks[cblkno];

                    // --- Length increment ---
                    // Compute minimum numlenbits increment needed for all segment lengths
                    let first_pass_in_layer = cblk.numpasses;
                    let last_pass_in_layer = first_pass_in_layer + layer.numpasses;
                    let mut increment = 0u32;
                    let mut seg_start = first_pass_in_layer;

                    while seg_start < last_pass_in_layer {
                        // Find segment end: at terminating pass or end of layer
                        let mut seg_end = seg_start + 1;
                        while seg_end < last_pass_in_layer {
                            if (cblksty
                                & (crate::types::J2K_CCP_CBLKSTY_TERMALL
                                    | crate::types::J2K_CCP_CBLKSTY_LAZY))
                                != 0
                            {
                                let pass_idx = seg_end as usize - 1;
                                if pass_idx < cblk.passes.len() && cblk.passes[pass_idx].term {
                                    break;
                                }
                            }
                            seg_end += 1;
                        }
                        let numpasses_in_seg = seg_end - seg_start;

                        // Compute segment data length
                        let seg_data_len = if seg_end > 0 && (seg_end as usize) <= cblk.passes.len()
                        {
                            let end_rate = cblk.passes[seg_end as usize - 1].rate;
                            let start_rate =
                                if seg_start > 0 && (seg_start as usize) <= cblk.passes.len() {
                                    cblk.passes[seg_start as usize - 1].rate
                                } else {
                                    0
                                };
                            end_rate.saturating_sub(start_rate)
                        } else {
                            0
                        };

                        // Compute bits needed: we need enough bits to represent seg_data_len
                        // bit_width = numlenbits + floorlog2(numpasses_in_seg)
                        let passno_bits = uint_floorlog2(numpasses_in_seg);
                        let available_bits = cblk.numlenbits + passno_bits;
                        let needed_bits = if seg_data_len > 0 {
                            uint_floorlog2(seg_data_len) + 1
                        } else {
                            0
                        };
                        if needed_bits > available_bits {
                            let new_inc = needed_bits - available_bits;
                            if new_inc > increment {
                                increment = new_inc;
                            }
                        }

                        seg_start = seg_end;
                    }

                    t2_putcommacode(&mut bio, increment)?;

                    // --- Segment lengths ---
                    let cblk = &mut cblks[cblkno];
                    cblk.numlenbits += increment;
                    let mut seg_start = first_pass_in_layer;

                    while seg_start < last_pass_in_layer {
                        let mut seg_end = seg_start + 1;
                        while seg_end < last_pass_in_layer {
                            if (cblksty
                                & (crate::types::J2K_CCP_CBLKSTY_TERMALL
                                    | crate::types::J2K_CCP_CBLKSTY_LAZY))
                                != 0
                            {
                                let pass_idx = seg_end as usize - 1;
                                if pass_idx < cblk.passes.len() && cblk.passes[pass_idx].term {
                                    break;
                                }
                            }
                            seg_end += 1;
                        }
                        let numpasses_in_seg = seg_end - seg_start;
                        let seg_data_len = if seg_end > 0 && (seg_end as usize) <= cblk.passes.len()
                        {
                            let end_rate = cblk.passes[seg_end as usize - 1].rate;
                            let start_rate =
                                if seg_start > 0 && (seg_start as usize) <= cblk.passes.len() {
                                    cblk.passes[seg_start as usize - 1].rate
                                } else {
                                    0
                                };
                            end_rate.saturating_sub(start_rate)
                        } else {
                            0
                        };

                        let bit_width = cblk.numlenbits + uint_floorlog2(numpasses_in_seg);
                        bio.write(seg_data_len, bit_width)?;

                        seg_start = seg_end;
                    }
                }
            }
        }

        bio.flush()?;
        header_len = bio.num_bytes();
    }

    // --- Write packet body: code block data ---
    let mut offset = header_len;
    let comp = &mut tile.comps[compno as usize];
    let res = &mut comp.resolutions[resno as usize];

    for bandno in 0..numbands {
        let band = &mut res.bands[bandno as usize];
        if band.is_empty() {
            continue;
        }
        let prec = match band.precincts.get_mut(precno as usize) {
            Some(p) => p,
            None => continue,
        };
        let num_cblks = (prec.cw * prec.ch) as usize;

        if let TcdCodeBlocks::Enc(ref mut cblks) = prec.cblks {
            for cblk in cblks.iter_mut().take(num_cblks) {
                let layer = match cblk.layers.get(layno as usize) {
                    Some(l) => l.clone(),
                    None => continue,
                };
                if layer.numpasses == 0 {
                    continue;
                }

                let data_start = layer.data_offset as usize;
                let data_end = data_start + layer.len as usize;
                if data_end > cblk.data.len() {
                    return Err(Error::InvalidInput(format!(
                        "cblk data range {}..{} exceeds data len {}",
                        data_start,
                        data_end,
                        cblk.data.len()
                    )));
                }
                if offset + layer.len as usize > dest.len() {
                    return Err(Error::EndOfStream);
                }
                dest[offset..offset + layer.len as usize]
                    .copy_from_slice(&cblk.data[data_start..data_end]);
                offset += layer.len as usize;

                // Update cumulative pass count
                cblk.numpasses += layer.numpasses;
            }
        }
    }

    Ok(offset)
}

/// Encode all packets for a tile using packet iterators.
/// Returns total bytes written.
/// (C: opj_t2_encode_packets)
pub fn t2_encode_packets(
    tile: &mut TcdTile,
    tcp: &crate::j2k::params::TileCodingParameters,
    pis: &mut crate::tier2::pi::PacketIterators,
    dest: &mut [u8],
) -> Result<usize> {
    let mut total_written = 0usize;

    for pino in 0..pis.len() {
        while pis.next(pino) {
            let pi = pis.get(pino);
            let layno = pi.layno;
            let compno = pi.compno;
            let resno = pi.resno;
            let precno = pi.precno;

            // Skip layers beyond numlayers
            if layno >= tcp.numlayers {
                continue;
            }

            let cblksty = tcp
                .tccps
                .get(compno as usize)
                .ok_or_else(|| Error::InvalidInput(format!("missing TCCP for component {compno}")))?
                .cblksty;

            if total_written >= dest.len() {
                return Err(Error::EndOfStream);
            }

            let bytes = t2_encode_packet(
                tile,
                compno,
                resno,
                precno,
                layno,
                cblksty,
                &mut dest[total_written..],
            )?;
            total_written += bytes;
        }
    }

    Ok(total_written)
}

pub fn t2_decode_packet(
    tile: &mut TcdTile,
    compno: u32,
    resno: u32,
    precno: u32,
    layno: u32,
    cblksty: u32,
    data: &mut [u8],
) -> Result<usize> {
    let (data_present, header_bytes) =
        t2_read_packet_header(tile, compno, resno, precno, layno, cblksty, data)?;
    if !data_present {
        return Ok(header_bytes);
    }
    let data_bytes = t2_read_packet_data(tile, compno, resno, precno, &mut data[header_bytes..])?;
    Ok(header_bytes + data_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coding::t1::TcdPass;
    use crate::coding::tgt::TagTree;
    use crate::tcd::{
        TcdBand, TcdCblkDec, TcdCblkEnc, TcdCodeBlocks, TcdLayer, TcdPrecinct, TcdResolution,
        TcdTileComp,
    };
    use crate::types::{J2K_MAXLAYERS, uint_floorlog2};

    // ---------------------------------------------------------------------------
    // Helper: build a minimal TcdTile with a single precinct/codeblock
    // ---------------------------------------------------------------------------

    /// Create a 1-component, 1-resolution, 1-band, 1-precinct, 1-codeblock tile.
    fn make_tile_1cblk(band_numbps: i32) -> TcdTile {
        let cblk = TcdCblkDec::default();
        let mut incltree = TagTree::new(1, 1);
        incltree.reset();
        let mut imsbtree = TagTree::new(1, 1);
        imsbtree.reset();
        let prec = TcdPrecinct {
            x0: 0,
            y0: 0,
            x1: 64,
            y1: 64,
            cw: 1,
            ch: 1,
            cblks: TcdCodeBlocks::Dec(vec![cblk]),
            incltree: Some(incltree),
            imsbtree: Some(imsbtree),
        };
        let band = TcdBand {
            x0: 0,
            y0: 0,
            x1: 64,
            y1: 64,
            bandno: 0,
            precincts: vec![prec],
            numbps: band_numbps,
            stepsize: 1.0,
        };
        let res = TcdResolution {
            x0: 0,
            y0: 0,
            x1: 64,
            y1: 64,
            pw: 1,
            ph: 1,
            numbands: 1,
            bands: vec![band],
            win_x0: 0,
            win_y0: 0,
            win_x1: 64,
            win_y1: 64,
        };
        let comp = TcdTileComp {
            x0: 0,
            y0: 0,
            x1: 64,
            y1: 64,
            compno: 0,
            numresolutions: 1,
            minimum_num_resolutions: 1,
            resolutions: vec![res],
            data: vec![],
            numpix: 64 * 64,
            win_x0: 0,
            win_y0: 0,
            win_x1: 64,
            win_y1: 64,
            data_win: None,
        };
        TcdTile {
            x0: 0,
            y0: 0,
            x1: 64,
            y1: 64,
            comps: vec![comp],
            numpix: 64 * 64,
            distotile: 0.0,
            distolayer: [0.0; J2K_MAXLAYERS],
            packno: 0,
        }
    }

    /// Encode a test packet for a 1-cblk tile:
    /// - inclusion in layer `layno`
    /// - `imsb_value` zero bit-planes
    /// - `numpasses` coding passes
    /// - `data_len` bytes of dummy data
    fn encode_test_packet(
        layno: u32,
        band_numbps: i32,
        imsb_value: i32,
        numpasses: u32,
        data_len: u32,
        _cblksty: u32,
    ) -> Vec<u8> {
        let mut buf = vec![0u8; 256];

        // -- Packet header --
        let header_len;
        {
            let mut bio = Bio::encoder(&mut buf);
            // Present bit
            bio.write(1, 1).unwrap();
            // Inclusion tag tree: leaf 0, value = layno (included at this layer)
            let mut incl = TagTree::new(1, 1);
            incl.set_value(0, layno as i32);
            incl.encode(&mut bio, 0, (layno + 1) as i32).unwrap();
            // IMSB tag tree: leaf 0, value = imsb_value
            let mut imsb = TagTree::new(1, 1);
            imsb.set_value(0, imsb_value);
            imsb.encode(&mut bio, 0, band_numbps + 1).unwrap();
            // Number of passes
            t2_putnumpasses(&mut bio, numpasses).unwrap();
            // Comma code increment (0 = no extra length bits)
            t2_putcommacode(&mut bio, 0).unwrap();
            // Segment length: numlenbits(3) + floorlog2(numpasses)
            let bit_number = 3 + uint_floorlog2(numpasses);
            bio.write(data_len, bit_number).unwrap();
            bio.flush().unwrap();
            header_len = bio.num_bytes();
        }

        // Truncate to header + data
        buf.truncate(header_len + data_len as usize);
        // Fill data with pattern
        for i in 0..data_len as usize {
            buf[header_len + i] = (i as u8).wrapping_add(0xAA);
        }
        buf
    }

    // ---------------------------------------------------------------------------
    // T2 packet decode tests
    // ---------------------------------------------------------------------------

    #[test]
    fn t2_decode_empty_packet() {
        let mut tile = make_tile_1cblk(8);
        // Empty packet: present bit = 0
        let mut data = [0x00u8; 1];
        let (data_present, bytes_read) =
            t2_read_packet_header(&mut tile, 0, 0, 0, 0, 0, &mut data).unwrap();
        assert!(!data_present);
        assert_eq!(bytes_read, 1);
    }

    #[test]
    fn t2_decode_single_cblk_packet() {
        let band_numbps = 8;
        let imsb_value = 0;
        let numpasses = 1u32;
        let data_len = 5u32;
        let mut packet = encode_test_packet(0, band_numbps, imsb_value, numpasses, data_len, 0);

        let mut tile = make_tile_1cblk(band_numbps);
        let (data_present, header_bytes) =
            t2_read_packet_header(&mut tile, 0, 0, 0, 0, 0, &mut packet).unwrap();
        assert!(data_present);

        // After header: code block should have numnewpasses set
        let cblk = match &tile.comps[0].resolutions[0].bands[0].precincts[0].cblks {
            TcdCodeBlocks::Dec(cblks) => &cblks[0],
            _ => panic!("expected Dec cblks"),
        };
        assert_eq!(cblk.numnewpasses, numpasses);
        assert_eq!(cblk.numbps, band_numbps as u32);

        // Read packet data
        let data_bytes =
            t2_read_packet_data(&mut tile, 0, 0, 0, &mut packet[header_bytes..]).unwrap();
        assert_eq!(data_bytes, data_len as usize);

        // Verify segment data was stored
        let cblk = match &tile.comps[0].resolutions[0].bands[0].precincts[0].cblks {
            TcdCodeBlocks::Dec(cblks) => &cblks[0],
            _ => panic!("expected Dec cblks"),
        };
        assert_eq!(cblk.chunks.len(), 1);
        assert_eq!(cblk.chunks[0].len, data_len);
    }

    #[test]
    fn t2_decode_packet_combines_header_and_data() {
        let band_numbps = 8;
        let mut packet = encode_test_packet(0, band_numbps, 0, 1, 5, 0);
        let mut tile = make_tile_1cblk(band_numbps);
        let total_bytes = t2_decode_packet(&mut tile, 0, 0, 0, 0, 0, &mut packet).unwrap();
        assert!(total_bytes > 0);
        assert!(total_bytes <= packet.len());
    }

    // --- t2_decode_packets ---

    #[test]
    fn t2_decode_packets_single_layer() {
        use crate::j2k::params::Poc;
        use crate::tier2::pi::{PacketIterators, PiComp, PiIterator, PiResolution};
        use crate::types::ProgressionOrder;

        let band_numbps = 8;
        let numpasses = 1u32;
        let data_len = 5u32;
        let mut packet = encode_test_packet(0, band_numbps, 0, numpasses, data_len, 0);
        let mut tile = make_tile_1cblk(band_numbps);

        // Create a simple PacketIterators: 1 component, 1 res, 1 layer, 1 precinct
        let pi_res = PiResolution {
            pdx: 15,
            pdy: 15,
            pw: 1,
            ph: 1,
        };
        let pi_comp = PiComp {
            dx: 1,
            dy: 1,
            numresolutions: 1,
            resolutions: vec![pi_res],
        };
        let poc = Poc {
            layno1: 1,
            resno1: 1,
            compno1: 1,
            precno1: 1,
            prg: ProgressionOrder::Lrcp,
            tx1: 64,
            ty1: 64,
            ..Default::default()
        };
        let pi = PiIterator {
            tp_on: false,
            step_l: 1,
            step_r: 1,
            step_c: 1,
            step_p: 1,
            compno: 0,
            resno: 0,
            precno: 0,
            layno: 0,
            first: true,
            poc,
            numcomps: 1,
            comps: vec![pi_comp],
            tx0: 0,
            ty0: 0,
            tx1: 64,
            ty1: 64,
            x: 0,
            y: 0,
            dx: 0,
            dy: 0,
        };
        let mut pis = PacketIterators {
            iterators: vec![pi],
            include: vec![0i16; 1],
        };

        let tcp = crate::j2k::params::TileCodingParameters {
            numlayers: 1,
            tccps: vec![crate::j2k::params::TileCompCodingParameters::default()],
            ..Default::default()
        };

        let bytes_read = t2_decode_packets(&mut tile, &tcp, &mut pis, &mut packet, 1).unwrap();
        assert_eq!(bytes_read, packet.len());

        // Verify codeblock has segment data
        let cblk = match &tile.comps[0].resolutions[0].bands[0].precincts[0].cblks {
            TcdCodeBlocks::Dec(cblks) => &cblks[0],
            _ => panic!("expected Dec cblks"),
        };
        assert!(!cblk.chunks.is_empty());
    }

    // --- Comma code ---

    #[test]
    fn comma_code_roundtrip() {
        for n in 0..20 {
            let mut buf = vec![0u8; 16];
            {
                let mut bio = Bio::encoder(&mut buf);
                t2_putcommacode(&mut bio, n).unwrap();
                bio.flush().unwrap();
            }
            {
                let mut bio = Bio::decoder(&mut buf);
                let decoded = t2_getcommacode(&mut bio).unwrap();
                assert_eq!(decoded, n, "comma code roundtrip failed for n={n}");
            }
        }
    }

    #[test]
    fn comma_code_zero() {
        let mut buf = vec![0u8; 4];
        {
            let mut bio = Bio::encoder(&mut buf);
            t2_putcommacode(&mut bio, 0).unwrap();
            bio.flush().unwrap();
        }
        {
            let mut bio = Bio::decoder(&mut buf);
            assert_eq!(t2_getcommacode(&mut bio).unwrap(), 0);
        }
    }

    // --- Number of passes ---

    #[test]
    fn numpasses_roundtrip() {
        for n in 1..=164 {
            let mut buf = vec![0u8; 16];
            {
                let mut bio = Bio::encoder(&mut buf);
                t2_putnumpasses(&mut bio, n).unwrap();
                bio.flush().unwrap();
            }
            {
                let mut bio = Bio::decoder(&mut buf);
                let decoded = t2_getnumpasses(&mut bio).unwrap();
                assert_eq!(decoded, n, "numpasses roundtrip failed for n={n}");
            }
        }
    }

    #[test]
    fn numpasses_boundary_values() {
        for n in [1, 2, 3, 5, 6, 36, 37, 164] {
            let mut buf = vec![0u8; 16];
            {
                let mut bio = Bio::encoder(&mut buf);
                t2_putnumpasses(&mut bio, n).unwrap();
                bio.flush().unwrap();
            }
            {
                let mut bio = Bio::decoder(&mut buf);
                let decoded = t2_getnumpasses(&mut bio).unwrap();
                assert_eq!(decoded, n, "numpasses boundary failed for n={n}");
            }
        }
    }

    // --- Segment initialization ---

    #[test]
    fn init_seg_default_maxpasses() {
        let mut segs = Vec::new();
        t2_init_seg(&mut segs, 0, 0, true);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].maxpasses, 109);
        assert_eq!(segs[0].len, 0);
        assert_eq!(segs[0].numpasses, 0);
    }

    #[test]
    fn init_seg_termall() {
        let mut segs = Vec::new();
        t2_init_seg(&mut segs, 0, J2K_CCP_CBLKSTY_TERMALL, true);
        assert_eq!(segs[0].maxpasses, 1);
    }

    #[test]
    fn init_seg_lazy_alternation() {
        let mut segs = Vec::new();
        // First segment: 10 passes
        t2_init_seg(&mut segs, 0, J2K_CCP_CBLKSTY_LAZY, true);
        assert_eq!(segs[0].maxpasses, 10);
        // Second segment: 2 (since previous was 10)
        t2_init_seg(&mut segs, 1, J2K_CCP_CBLKSTY_LAZY, false);
        assert_eq!(segs[1].maxpasses, 2);
        // Third segment: 1 (since previous was 2)
        t2_init_seg(&mut segs, 2, J2K_CCP_CBLKSTY_LAZY, false);
        assert_eq!(segs[2].maxpasses, 1);
        // Fourth segment: 2 (since previous was 1)
        t2_init_seg(&mut segs, 3, J2K_CCP_CBLKSTY_LAZY, false);
        assert_eq!(segs[3].maxpasses, 2);
        // Fifth segment: 1 (since previous was 2)
        t2_init_seg(&mut segs, 4, J2K_CCP_CBLKSTY_LAZY, false);
        assert_eq!(segs[4].maxpasses, 1);
    }

    #[test]
    fn init_seg_grows_vector() {
        let mut segs = Vec::new();
        t2_init_seg(&mut segs, 5, 0, true);
        assert_eq!(segs.len(), 6);
        assert_eq!(segs[5].maxpasses, 109);
    }

    // ---------------------------------------------------------------------------
    // Helper: build encoding tile with TcdCblkEnc
    // ---------------------------------------------------------------------------

    /// Create a 1-component, 1-resolution, 1-band, 1-precinct, 1-codeblock
    /// encoding tile.
    fn make_enc_tile_1cblk(
        band_numbps: i32,
        cblk_numbps: u32,
        cblk_data: Vec<u8>,
        passes: Vec<TcdPass>,
        layers: Vec<TcdLayer>,
    ) -> TcdTile {
        let cblk = TcdCblkEnc {
            data: cblk_data,
            layers,
            passes,
            x0: 0,
            y0: 0,
            x1: 64,
            y1: 64,
            numbps: cblk_numbps,
            numlenbits: 3,
            numpasses: 0,
            numpassesinlayers: 0,
            totalpasses: 0,
        };
        let mut incltree = TagTree::new(1, 1);
        incltree.reset();
        let mut imsbtree = TagTree::new(1, 1);
        imsbtree.reset();
        // Set IMSB value: band_numbps - cblk_numbps
        imsbtree.set_value(0, band_numbps - cblk_numbps as i32);
        let prec = TcdPrecinct {
            x0: 0,
            y0: 0,
            x1: 64,
            y1: 64,
            cw: 1,
            ch: 1,
            cblks: TcdCodeBlocks::Enc(vec![cblk]),
            incltree: Some(incltree),
            imsbtree: Some(imsbtree),
        };
        let band = TcdBand {
            x0: 0,
            y0: 0,
            x1: 64,
            y1: 64,
            bandno: 0,
            precincts: vec![prec],
            numbps: band_numbps,
            stepsize: 1.0,
        };
        let res = TcdResolution {
            x0: 0,
            y0: 0,
            x1: 64,
            y1: 64,
            pw: 1,
            ph: 1,
            numbands: 1,
            bands: vec![band],
            win_x0: 0,
            win_y0: 0,
            win_x1: 64,
            win_y1: 64,
        };
        let comp = TcdTileComp {
            x0: 0,
            y0: 0,
            x1: 64,
            y1: 64,
            compno: 0,
            numresolutions: 1,
            minimum_num_resolutions: 1,
            resolutions: vec![res],
            data: vec![],
            numpix: 64 * 64,
            win_x0: 0,
            win_y0: 0,
            win_x1: 64,
            win_y1: 64,
            data_win: None,
        };
        TcdTile {
            x0: 0,
            y0: 0,
            x1: 64,
            y1: 64,
            comps: vec![comp],
            numpix: 64 * 64,
            distotile: 0.0,
            distolayer: [0.0; J2K_MAXLAYERS],
            packno: 0,
        }
    }

    // ---------------------------------------------------------------------------
    // T2 packet encode tests
    // ---------------------------------------------------------------------------

    #[test]
    fn t2_encode_empty_packet() {
        // Layer with 0 passes — should write a single "not present" bit
        let layers = vec![TcdLayer {
            numpasses: 0,
            len: 0,
            disto: 0.0,
            data_offset: 0,
        }];
        let mut tile = make_enc_tile_1cblk(8, 8, vec![0u8; 16], vec![], layers);
        let mut buf = vec![0u8; 256];
        let bytes_written = t2_encode_packet(&mut tile, 0, 0, 0, 0, 0, &mut buf).unwrap();
        // Empty packet: "not present" bit + alignment = 1 byte
        assert_eq!(bytes_written, 1);
        // "not present" bit = 0 in MSB
        assert_eq!(buf[0] & 0x80, 0);
    }

    #[test]
    fn t2_encode_single_cblk_packet() {
        // 1 cblk, 1 layer with 1 pass, 5 bytes of data
        let cblk_data = vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE];
        let passes = vec![TcdPass {
            rate: 5,
            distortion_decrease: 100.0,
            len: 5,
            term: false,
        }];
        let layers = vec![TcdLayer {
            numpasses: 1,
            len: 5,
            disto: 100.0,
            data_offset: 0,
        }];
        let mut tile = make_enc_tile_1cblk(8, 8, cblk_data.clone(), passes, layers);
        let mut buf = vec![0u8; 256];
        let bytes_written = t2_encode_packet(&mut tile, 0, 0, 0, 0, 0, &mut buf).unwrap();
        // Should write header + 5 bytes of data
        assert!(bytes_written > 5);
        // Data should appear at the end of the packet
        assert_eq!(&buf[bytes_written - 5..bytes_written], &cblk_data[..5]);
    }

    #[test]
    fn t2_encode_decode_roundtrip() {
        // Encode a packet, then decode it and verify the data matches
        let cblk_data = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let passes = vec![TcdPass {
            rate: 8,
            distortion_decrease: 50.0,
            len: 8,
            term: false,
        }];
        let layers = vec![TcdLayer {
            numpasses: 1,
            len: 8,
            disto: 50.0,
            data_offset: 0,
        }];
        let band_numbps = 8i32;
        let cblk_numbps = 8u32;

        // Encode
        let mut enc_tile =
            make_enc_tile_1cblk(band_numbps, cblk_numbps, cblk_data.clone(), passes, layers);
        let mut buf = vec![0u8; 256];
        let bytes_written = t2_encode_packet(&mut enc_tile, 0, 0, 0, 0, 0, &mut buf).unwrap();

        // Decode with a fresh decode tile
        let mut dec_tile = make_tile_1cblk(band_numbps);
        let total_bytes =
            t2_decode_packet(&mut dec_tile, 0, 0, 0, 0, 0, &mut buf[..bytes_written]).unwrap();
        assert_eq!(total_bytes, bytes_written);

        // Verify decoded cblk has the same data
        let dec_cblk = match &dec_tile.comps[0].resolutions[0].bands[0].precincts[0].cblks {
            TcdCodeBlocks::Dec(cblks) => &cblks[0],
            _ => panic!("expected Dec cblks"),
        };
        assert_eq!(dec_cblk.chunks.len(), 1);
        assert_eq!(&dec_cblk.chunks[0].data, &cblk_data);
    }

    #[test]
    fn t2_encode_packets_single_layer() {
        use crate::j2k::params::Poc;
        use crate::tier2::pi::{PacketIterators, PiComp, PiIterator, PiResolution};
        use crate::types::ProgressionOrder;

        let cblk_data = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let passes = vec![TcdPass {
            rate: 4,
            distortion_decrease: 10.0,
            len: 4,
            term: false,
        }];
        let layers = vec![TcdLayer {
            numpasses: 1,
            len: 4,
            disto: 10.0,
            data_offset: 0,
        }];
        let mut tile = make_enc_tile_1cblk(8, 8, cblk_data, passes, layers);

        // Build PacketIterators (same pattern as decode test)
        let pi_res = PiResolution {
            pdx: 15,
            pdy: 15,
            pw: 1,
            ph: 1,
        };
        let pi_comp = PiComp {
            dx: 1,
            dy: 1,
            numresolutions: 1,
            resolutions: vec![pi_res],
        };
        let poc = Poc {
            layno1: 1,
            resno1: 1,
            compno1: 1,
            precno1: 1,
            prg: ProgressionOrder::Lrcp,
            tx1: 64,
            ty1: 64,
            ..Default::default()
        };
        let pi = PiIterator {
            tp_on: false,
            step_l: 1,
            step_r: 1,
            step_c: 1,
            step_p: 1,
            compno: 0,
            resno: 0,
            precno: 0,
            layno: 0,
            first: true,
            poc,
            numcomps: 1,
            comps: vec![pi_comp],
            tx0: 0,
            ty0: 0,
            tx1: 64,
            ty1: 64,
            x: 0,
            y: 0,
            dx: 0,
            dy: 0,
        };
        let mut pis = PacketIterators {
            iterators: vec![pi],
            include: vec![0i16; 1],
        };

        let tcp = crate::j2k::params::TileCodingParameters {
            numlayers: 1,
            tccps: vec![crate::j2k::params::TileCompCodingParameters::default()],
            ..Default::default()
        };

        let mut buf = vec![0u8; 256];
        let bytes_written = t2_encode_packets(&mut tile, &tcp, &mut pis, &mut buf).unwrap();
        assert!(bytes_written > 4); // header + 4 bytes data
    }

    #[test]
    fn t2_encode_multi_pass_single_layer() {
        // Cblk with 3 passes, all in layer 0
        let cblk_data = vec![0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xA0];
        let passes = vec![
            TcdPass {
                rate: 3,
                distortion_decrease: 30.0,
                len: 3,
                term: false,
            },
            TcdPass {
                rate: 7,
                distortion_decrease: 20.0,
                len: 4,
                term: false,
            },
            TcdPass {
                rate: 10,
                distortion_decrease: 10.0,
                len: 3,
                term: false,
            },
        ];
        let layers = vec![TcdLayer {
            numpasses: 3,
            len: 10,
            disto: 60.0,
            data_offset: 0,
        }];
        let mut tile = make_enc_tile_1cblk(8, 8, cblk_data.clone(), passes, layers);
        let mut buf = vec![0u8; 256];
        let bytes_written = t2_encode_packet(&mut tile, 0, 0, 0, 0, 0, &mut buf).unwrap();
        // Should include all 10 bytes of data
        assert_eq!(&buf[bytes_written - 10..bytes_written], &cblk_data[..]);
    }

    // --- Pass bits ---

    #[test]
    fn pass_bits_ranges() {
        assert_eq!(t2_getpassbits(0), 1);
        assert_eq!(t2_getpassbits(1), 1);
        assert_eq!(t2_getpassbits(2), 2);
        assert_eq!(t2_getpassbits(5), 2);
        assert_eq!(t2_getpassbits(6), 4);
        assert_eq!(t2_getpassbits(36), 4);
        assert_eq!(t2_getpassbits(37), 6);
        assert_eq!(t2_getpassbits(164), 6);
        assert_eq!(t2_getpassbits(165), 8);
    }
}
