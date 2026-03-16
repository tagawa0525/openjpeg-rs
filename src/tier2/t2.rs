// Phase 400c: Tier-2 packet encoding/decoding (C: t2.c)
//
// Encodes and decodes packets: the basic unit of a JPEG 2000 codestream.
// Each packet contains header (tag tree inclusion, zero-bitplane info,
// pass counts, segment lengths) and body (code block compressed data).

use crate::error::Result;
use crate::io::bio::Bio;
use crate::tcd::TcdSeg;
use crate::types::{J2K_CCP_CBLKSTY_LAZY, J2K_CCP_CBLKSTY_TERMALL};

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

#[cfg(test)]
mod tests {
    use super::*;

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
