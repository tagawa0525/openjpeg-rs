// Phase 700a: HTJ2K decoder - MEL decoder and bitstream readers
//
// Implements the low-level bitstream reading primitives for HT JPEG 2000
// codeblock decoding (ITU-T T.814).

use crate::error::{Error, Result};

// ---------------------------------------------------------------------------
// MEL (Magnitude Event Limiter) decoder
// ---------------------------------------------------------------------------

/// MEL exponent table: maps state k (0..12) to the number of bits for a run.
const MEL_EXP: [u32; 13] = [0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 4, 5];

/// MEL decoder state (C: dec_mel_t).
///
/// Decodes run-length encoded events from the MEL bitstream.
/// Uses a 13-state machine (k=0..12) with exponential Golomb-like coding.
pub struct MelDecoder<'a> {
    /// MEL bitstream data.
    data: &'a [u8],
    /// Current byte position.
    pos: usize,
    /// Temporary bit buffer (up to 64 bits, MSB-first).
    tmp: u64,
    /// Number of valid bits in tmp.
    bits: u32,
    /// Whether next byte needs unstuffing (MSB stripped after 0xFF).
    unstuff: bool,
    /// State machine index (0..12).
    k: u32,
    /// Queue of decoded runs (7 bits per run, LSB first).
    runs: u64,
    /// Number of valid runs in queue.
    num_runs: u32,
}

impl<'a> MelDecoder<'a> {
    /// Create a new MEL decoder from bitstream data.
    pub fn new(data: &'a [u8]) -> Self {
        let mut dec = MelDecoder {
            data,
            pos: 0,
            tmp: 0,
            bits: 0,
            unstuff: false,
            k: 0,
            runs: 0,
            num_runs: 0,
        };

        // Read initial bytes (up to 4) to fill the buffer.
        // This mirrors mel_init's initial byte reading loop.
        let num = data.len().min(4);
        for _ in 0..num {
            let d = if dec.pos < dec.data.len() {
                let b = dec.data[dec.pos] as u64;
                dec.pos += 1;
                b
            } else {
                0xFF
            };
            let d_bits = 8 - u32::from(dec.unstuff);
            dec.tmp = (dec.tmp << d_bits) | d;
            dec.bits += d_bits;
            dec.unstuff = (d & 0xFF) == 0xFF;
        }
        // Push bits to MSB so the first bit to decode is at bit 63.
        if dec.bits > 0 {
            dec.tmp <<= 64 - dec.bits;
        }
        dec
    }

    /// Read and unstuff 4 bytes from the MEL bitstream into the buffer.
    ///
    /// Mirrors C `mel_read`. Bytes are read forward. After 0xFF, the MSB of
    /// the next byte is stripped (unstuffing).
    fn mel_read(&mut self) {
        if self.bits > 32 {
            return;
        }

        // Read up to 4 raw bytes into val (little-endian order).
        let mut val: u32 = 0xFFFF_FFFF;
        let remaining = self.data.len() - self.pos;
        if remaining >= 4 {
            val = u32::from_le_bytes([
                self.data[self.pos],
                self.data[self.pos + 1],
                self.data[self.pos + 2],
                self.data[self.pos + 3],
            ]);
            self.pos += 4;
        } else if remaining > 0 {
            // Read available bytes, leave rest as 0xFF
            for i in 0..remaining {
                let v = self.data[self.pos] as u32;
                let m = !(0xFFu32 << (i * 8));
                val = (val & m) | (v << (i * 8));
                self.pos += 1;
            }
        }

        // Unstuff the 4 bytes and accumulate in t.
        // bits counts the total number of usable bits.
        let mut bits = 32u32 - u32::from(self.unstuff);

        let mut t: u32 = val & 0xFF;
        let mut unstuff = (val & 0xFF) == 0xFF;
        bits -= u32::from(unstuff);
        t <<= 8 - u32::from(unstuff);

        t |= (val >> 8) & 0xFF;
        unstuff = ((val >> 8) & 0xFF) == 0xFF;
        bits -= u32::from(unstuff);
        t <<= 8 - u32::from(unstuff);

        t |= (val >> 16) & 0xFF;
        unstuff = ((val >> 16) & 0xFF) == 0xFF;
        bits -= u32::from(unstuff);
        t <<= 8 - u32::from(unstuff);

        t |= (val >> 24) & 0xFF;
        self.unstuff = ((val >> 24) & 0xFF) == 0xFF;

        // Merge into tmp at the correct position (MSB-first).
        self.tmp |= (t as u64) << (64 - bits - self.bits);
        self.bits += bits;
    }

    /// Decode multiple runs from the MEL bitstream into the runs queue.
    ///
    /// Mirrors C `mel_decode`. Fills the runs queue (up to 8 entries).
    fn mel_decode(&mut self) {
        if self.bits < 6 {
            self.mel_read();
        }

        while self.bits >= 6 && self.num_runs < 8 {
            let eval = MEL_EXP[self.k as usize];
            let run;
            if self.tmp & (1u64 << 63) != 0 {
                // MSB is 1: run of all-zeros not terminating with a one event.
                run = (((1u32 << eval) - 1) << 1) as u64;
                self.k = (self.k + 1).min(12);
                self.tmp <<= 1;
                self.bits -= 1;
            } else {
                // MSB is 0: extract eval bits for the run value, terminating.
                let r = ((self.tmp >> (63 - eval)) as u32) & ((1 << eval) - 1);
                run = ((r << 1) + 1) as u64;
                self.k = self.k.saturating_sub(1);
                self.tmp <<= eval + 1;
                self.bits -= eval + 1;
            }
            let shift = self.num_runs * 7;
            self.runs &= !(0x7Fu64 << shift);
            self.runs |= run << shift;
            self.num_runs += 1;
        }
    }

    /// Decode a single run value from the MEL stream.
    ///
    /// Returns the run length (number of all-zero quads before next event).
    pub fn decode_run(&mut self) -> Result<u32> {
        if self.num_runs == 0 {
            self.mel_decode();
        }
        let t = (self.runs & 0x7F) as u32;
        self.runs >>= 7;
        self.num_runs = self.num_runs.saturating_sub(1);
        Ok(t)
    }
}

// ---------------------------------------------------------------------------
// Reverse bitstream reader (for VLC and MRP)
// ---------------------------------------------------------------------------

/// Reverse (backward) bitstream reader (C: rev_struct_t).
///
/// Reads from the end of a buffer backward. Used for VLC and MRP streams
/// which are stored in reverse byte order.
pub struct RevReader<'a> {
    /// Source data.
    data: &'a [u8],
    /// Current byte position (decreasing). Points to the next byte to read.
    pos: isize,
    /// Temporary bit buffer (bits accumulate from LSB).
    tmp: u64,
    /// Number of valid bits in tmp.
    bits: u32,
    /// Whether next byte (backward) needs unstuffing.
    unstuff: bool,
}

impl<'a> RevReader<'a> {
    /// Create a reverse reader starting from the end of `data`.
    ///
    /// Simplified version for Phase 700a: reads backward from the last byte.
    /// The first byte read has its upper 4 bits as data, lower 4 discarded
    /// (they contain segment length info), matching `rev_init`.
    pub fn new(data: &'a [u8]) -> Self {
        let mut reader = RevReader {
            data,
            pos: 0, // will be set below
            tmp: 0,
            bits: 0,
            unstuff: false,
        };

        if data.is_empty() {
            return reader;
        }

        // Start at the last byte (mirrors vlcp->data = data + lcup - 2,
        // but simplified: just start at end).
        let last = data.len() - 1;
        reader.pos = last as isize;

        // Read the first byte: upper 4 bits are data, lower 4 are discarded.
        let d = data[last] as u64;
        reader.tmp = d >> 4;
        // Check if the upper nibble is all 1s (standard check).
        reader.bits = 4 - u32::from((reader.tmp & 7) == 7);
        reader.unstuff = (d | 0xF) > 0x8F;
        reader.pos -= 1;

        // Read a few initial bytes to fill the buffer.
        let num = (reader.pos + 1).min(4) as usize;
        for _ in 0..num {
            if reader.pos < 0 {
                break;
            }
            let d = data[reader.pos as usize] as u64;
            let d_bits = 8u32 - u32::from(reader.unstuff && ((d & 0x7F) == 0x7F));
            reader.tmp |= d << reader.bits;
            reader.bits += d_bits;
            reader.unstuff = d > 0x8F;
            reader.pos -= 1;
        }

        // Read another batch to ensure at least 32 bits available.
        reader.read();

        reader
    }

    /// Read more bytes backward into the buffer.
    fn read(&mut self) {
        if self.bits > 32 {
            return;
        }

        // Read up to 4 bytes backward into val (MSB-first order).
        let mut val: u32 = 0;
        let remaining = (self.pos + 1) as usize;
        if remaining >= 4 {
            // Read 4 bytes: positions pos, pos-1, pos-2, pos-3.
            // In the C code, read_le_uint32(vlcp->data - 3) reads 4 bytes
            // ending at vlcp->data, with MSB at vlcp->data.
            let p = self.pos as usize;
            val = u32::from_be_bytes([
                self.data[p - 3],
                self.data[p - 2],
                self.data[p - 1],
                self.data[p],
            ]);
            self.pos -= 4;
        } else if remaining > 0 {
            let mut i = 24i32;
            let mut r = remaining;
            while r > 0 {
                let v = self.data[self.pos as usize] as u32;
                val |= v << i;
                self.pos -= 1;
                r -= 1;
                i -= 8;
            }
        }

        // Unstuff and accumulate (byte-by-byte from MSB of val).
        let mut tmp: u32 = val >> 24;
        let mut bits = 8u32 - u32::from(self.unstuff && (((val >> 24) & 0x7F) == 0x7F));
        let mut unstuff = (val >> 24) > 0x8F;

        tmp |= ((val >> 16) & 0xFF) << bits;
        bits += 8u32 - u32::from(unstuff && (((val >> 16) & 0x7F) == 0x7F));
        unstuff = ((val >> 16) & 0xFF) > 0x8F;

        tmp |= ((val >> 8) & 0xFF) << bits;
        bits += 8u32 - u32::from(unstuff && (((val >> 8) & 0x7F) == 0x7F));
        unstuff = ((val >> 8) & 0xFF) > 0x8F;

        tmp |= (val & 0xFF) << bits;
        bits += 8u32 - u32::from(unstuff && ((val & 0x7F) == 0x7F));
        unstuff = (val & 0xFF) > 0x8F;

        self.tmp |= (tmp as u64) << self.bits;
        self.bits += bits;
        self.unstuff = unstuff;
    }

    /// Fetch up to 32 bits without consuming them.
    pub fn fetch(&self) -> u32 {
        self.tmp as u32
    }

    /// Consume `num_bits` from the buffer.
    pub fn advance(&mut self, num_bits: u32) -> Result<()> {
        if num_bits > self.bits || num_bits >= 64 {
            return Err(Error::InvalidInput(format!(
                "RevReader::advance: num_bits ({num_bits}) exceeds available bits ({}) or >= 64",
                self.bits
            )));
        }
        self.tmp = self.tmp.checked_shr(num_bits).unwrap_or(0);
        self.bits -= num_bits;
        if self.bits < 32 {
            self.read();
            if self.bits < 32 {
                self.read();
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Forward bitstream reader (for MagSgn and SPP)
// ---------------------------------------------------------------------------

/// Forward bitstream reader (C: frwd_struct_t).
///
/// Reads from the current position forward. Used for MagSgn and SPP streams.
/// Fills with `fill_byte` when data is exhausted.
pub struct FrwdReader<'a> {
    /// Source data.
    data: &'a [u8],
    /// Current byte position.
    pos: usize,
    /// Temporary bit buffer (bits accumulate from LSB).
    tmp: u64,
    /// Number of valid bits in tmp.
    bits: u32,
    /// Whether next byte needs unstuffing.
    unstuff: bool,
    /// Fill byte when exhausted (0xFF for MagSgn, 0x00 for SPP).
    fill_byte: u8,
}

impl<'a> FrwdReader<'a> {
    /// Create a forward reader.
    ///
    /// `fill_byte` is 0xFF for MagSgn streams, 0x00 for SPP streams.
    pub fn new(data: &'a [u8], fill_byte: u8) -> Self {
        let mut reader = FrwdReader {
            data,
            pos: 0,
            tmp: 0,
            bits: 0,
            unstuff: false,
            fill_byte,
        };

        // Read initial bytes (up to 4) to bootstrap the buffer.
        // Mirrors frwd_init's initial byte-by-byte loop.
        let num = data.len().min(4);
        for _ in 0..num {
            let d = if reader.pos < reader.data.len() {
                let b = reader.data[reader.pos] as u64;
                reader.pos += 1;
                b
            } else {
                reader.fill_byte as u64
            };
            reader.tmp |= d << reader.bits;
            reader.bits += 8 - u32::from(reader.unstuff);
            reader.unstuff = (d & 0xFF) == 0xFF;
        }

        // Read another 32 bits.
        reader.read();

        reader
    }

    /// Read and unstuff up to 32 more bits from the forward stream.
    ///
    /// Mirrors C `frwd_read`. When data is exhausted, fills with `fill_byte`.
    fn read(&mut self) {
        if self.bits > 32 {
            return;
        }

        let fill = if self.fill_byte != 0 {
            0xFFFF_FFFFu32
        } else {
            0u32
        };

        let remaining = self.data.len() - self.pos;
        let val: u32;
        if remaining >= 4 {
            val = u32::from_le_bytes([
                self.data[self.pos],
                self.data[self.pos + 1],
                self.data[self.pos + 2],
                self.data[self.pos + 3],
            ]);
            self.pos += 4;
        } else if remaining > 0 {
            let mut v = fill;
            for i in 0..remaining {
                let b = self.data[self.pos] as u32;
                let m = !(0xFFu32 << (i * 8));
                v = (v & m) | (b << (i * 8));
                self.pos += 1;
            }
            val = v;
        } else {
            val = fill;
        }

        // Unstuff 4 bytes and accumulate in t.
        let mut bits = 8u32 - u32::from(self.unstuff);
        let mut t: u32 = val & 0xFF;
        let mut unstuff = (val & 0xFF) == 0xFF;

        t |= ((val >> 8) & 0xFF) << bits;
        bits += 8u32 - u32::from(unstuff);
        unstuff = ((val >> 8) & 0xFF) == 0xFF;

        t |= ((val >> 16) & 0xFF) << bits;
        bits += 8u32 - u32::from(unstuff);
        unstuff = ((val >> 16) & 0xFF) == 0xFF;

        t |= ((val >> 24) & 0xFF) << bits;
        bits += 8u32 - u32::from(unstuff);
        self.unstuff = ((val >> 24) & 0xFF) == 0xFF;

        self.tmp |= (t as u64) << self.bits;
        self.bits += bits;
    }

    /// Fetch up to 32 bits without consuming them.
    pub fn fetch(&self) -> u32 {
        self.tmp as u32
    }

    /// Consume `num_bits` from the buffer.
    pub fn advance(&mut self, num_bits: u32) -> Result<()> {
        if num_bits > self.bits || num_bits >= 64 {
            return Err(Error::InvalidInput(format!(
                "FrwdReader::advance: num_bits ({num_bits}) exceeds available bits ({}) or >= 64",
                self.bits
            )));
        }
        self.tmp = self.tmp.checked_shr(num_bits).unwrap_or(0);
        self.bits -= num_bits;
        if self.bits < 32 {
            self.read();
            if self.bits < 32 {
                self.read();
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Count leading zeros in a 32-bit value.
#[inline]
pub fn count_leading_zeros(val: u32) -> u32 {
    val.leading_zeros()
}

/// Population count (number of set bits) in a 32-bit value.
#[inline]
pub fn population_count(val: u32) -> u32 {
    val.count_ones()
}

// ---------------------------------------------------------------------------
// UVLC decoders (ITU-T T.814, Table 3)
// ---------------------------------------------------------------------------

/// UVLC prefix decoder table.
///
/// 8 entries for prefix codewords: xx1, x10, 100, 000.
/// Each entry packs:
///   - bits [1:0]: prefix length (number of consumed bits)
///   - bits [4:2]: suffix length
///   - bits [7:5]: prefix value (u_pfx)
static UVLC_DEC: [u32; 8] = [
    0xB3, // 000 → prefix "000": len=3, suffix_len=5, u_pfx=5
    0x21, // 001 → prefix "1":   len=1, suffix_len=0, u_pfx=1
    0x42, // 010 → prefix "01":  len=2, suffix_len=0, u_pfx=2
    0x21, // 011 → prefix "1":   len=1, suffix_len=0, u_pfx=1
    0x67, // 100 → prefix "001": len=3, suffix_len=1, u_pfx=3
    0x21, // 101 → prefix "1":   len=1, suffix_len=0, u_pfx=1
    0x42, // 110 → prefix "01":  len=2, suffix_len=0, u_pfx=2
    0x21, // 111 → prefix "1":   len=1, suffix_len=0, u_pfx=1
];

/// Decode initial UVLC to get the u values for a quad pair (first 2 rows).
///
/// Returns `(consumed_bits, [u0, u1])` where u values include +1 for kappa.
///
/// Modes:
/// - 0: both u_off are 0 → u = [1, 1]
/// - 1: u_off = [1, 0] → decode one symbol for u[0]
/// - 2: u_off = [0, 1] → decode one symbol for u[1]
/// - 3: both u_off = 1, MEL event = 0 → decode two symbols (space-saving branch)
/// - 4: both u_off = 1, MEL event = 1 → decode two symbols, add +2 each
pub fn decode_init_uvlc(vlc: u32, mode: u32) -> (u32, [u32; 2]) {
    let mut consumed_bits = 0u32;
    let mut u = [1u32, 1u32];

    if mode == 0 {
        // Both u_off are 0; kappa is 1 for initial line.
        // consumed_bits = 0, u = [1, 1]
    } else if mode <= 2 {
        // u_off are either 01 or 10: decode one symbol.
        let d = UVLC_DEC[(vlc & 0x7) as usize];
        let prefix_len = d & 0x3;
        let suffix_len = (d >> 2) & 0x7;
        consumed_bits = prefix_len + suffix_len;
        let d_val = (d >> 5) + ((vlc >> prefix_len) & ((1u32 << suffix_len) - 1));
        if mode == 1 {
            u[0] = d_val + 1;
            u[1] = 1;
        } else {
            u[0] = 1;
            u[1] = d_val + 1;
        }
    } else if mode == 3 {
        // Both u_off are 1, MEL event is 0.
        let d1 = UVLC_DEC[(vlc & 0x7) as usize];
        let prefix_len1 = d1 & 0x3;
        let mut vlc = vlc >> prefix_len1;
        consumed_bits += prefix_len1;

        if prefix_len1 > 2 {
            // Space-saving branch: u[1] prefix is encoded as a single bit.
            u[1] = (vlc & 1) + 1 + 1; // +1 for kappa
            consumed_bits += 1;
            vlc >>= 1;

            let suffix_len1 = (d1 >> 2) & 0x7;
            consumed_bits += suffix_len1;
            let d1_val = (d1 >> 5) + (vlc & ((1u32 << suffix_len1) - 1));
            u[0] = d1_val + 1; // +1 for kappa
        } else {
            // Decode two separate symbols.
            let d2 = UVLC_DEC[(vlc & 0x7) as usize];
            let prefix_len2 = d2 & 0x3;
            vlc >>= prefix_len2;
            consumed_bits += prefix_len2;

            let suffix_len1 = (d1 >> 2) & 0x7;
            consumed_bits += suffix_len1;
            let d1_val = (d1 >> 5) + (vlc & ((1u32 << suffix_len1) - 1));
            u[0] = d1_val + 1;
            vlc >>= suffix_len1;

            let suffix_len2 = (d2 >> 2) & 0x7;
            consumed_bits += suffix_len2;
            let d2_val = (d2 >> 5) + (vlc & ((1u32 << suffix_len2) - 1));
            u[1] = d2_val + 1;
        }
    } else if mode == 4 {
        // Both u_off are 1, MEL event is 1.
        let d1 = UVLC_DEC[(vlc & 0x7) as usize];
        let prefix_len1 = d1 & 0x3;
        let mut vlc = vlc >> prefix_len1;
        consumed_bits += prefix_len1;

        let d2 = UVLC_DEC[(vlc & 0x7) as usize];
        let prefix_len2 = d2 & 0x3;
        vlc >>= prefix_len2;
        consumed_bits += prefix_len2;

        let suffix_len1 = (d1 >> 2) & 0x7;
        consumed_bits += suffix_len1;
        let d1_val = (d1 >> 5) + (vlc & ((1u32 << suffix_len1) - 1));
        u[0] = d1_val + 3; // add 2 + kappa(=1)
        vlc >>= suffix_len1;

        let suffix_len2 = (d2 >> 2) & 0x7;
        consumed_bits += suffix_len2;
        let d2_val = (d2 >> 5) + (vlc & ((1u32 << suffix_len2) - 1));
        u[1] = d2_val + 3; // add 2 + kappa(=1)
    }

    (consumed_bits, u)
}

/// Decode non-initial UVLC to get the u values for a quad pair (rows > 2).
///
/// Returns `(consumed_bits, [u0, u1])` where u values include +1 for kappa.
///
/// Modes 0-3 only (no mode 4). Mode 3 always decodes two separate symbols
/// (no space-saving branch).
pub fn decode_noninit_uvlc(vlc: u32, mode: u32) -> (u32, [u32; 2]) {
    let mut consumed_bits = 0u32;
    let mut u = [1u32, 1u32];

    if mode == 0 {
        // Both u_off are 0; u = [1, 1] for kappa.
    } else if mode <= 2 {
        // u_off are either 01 or 10: decode one symbol.
        let d = UVLC_DEC[(vlc & 0x7) as usize];
        let prefix_len = d & 0x3;
        let suffix_len = (d >> 2) & 0x7;
        consumed_bits = prefix_len + suffix_len;
        let d_val = (d >> 5) + ((vlc >> prefix_len) & ((1u32 << suffix_len) - 1));
        if mode == 1 {
            u[0] = d_val + 1;
            u[1] = 1;
        } else {
            u[0] = 1;
            u[1] = d_val + 1;
        }
    } else if mode == 3 {
        // Both u_off are 1: decode two separate symbols (no space-saving).
        let d1 = UVLC_DEC[(vlc & 0x7) as usize];
        let prefix_len1 = d1 & 0x3;
        let mut vlc = vlc >> prefix_len1;
        consumed_bits += prefix_len1;

        let d2 = UVLC_DEC[(vlc & 0x7) as usize];
        let prefix_len2 = d2 & 0x3;
        vlc >>= prefix_len2;
        consumed_bits += prefix_len2;

        let suffix_len1 = (d1 >> 2) & 0x7;
        consumed_bits += suffix_len1;
        let d1_val = (d1 >> 5) + (vlc & ((1u32 << suffix_len1) - 1));
        u[0] = d1_val + 1;
        vlc >>= suffix_len1;

        let suffix_len2 = (d2 >> 2) & 0x7;
        consumed_bits += suffix_len2;
        let d2_val = (d2 >> 5) + (vlc & ((1u32 << suffix_len2) - 1));
        u[1] = d2_val + 1;
    }

    (consumed_bits, u)
}

// ---------------------------------------------------------------------------
// HT codeblock decode (cleanup pass)
// ---------------------------------------------------------------------------

use crate::coding::ht_luts::{VLC_TBL0, VLC_TBL1};

/// Decode a single MagSgn sample and return the decoded coefficient value.
///
/// Given significance bit `sigma`, EMB bit `emb_k`, EMB bit `emb_1`,
/// the U_q value and bit-depth parameter p, this reads the MagSgn
/// bitstream and returns the coefficient.
#[inline]
fn decode_one_sample(
    magsgn: &mut FrwdReader<'_>,
    u_q: u32,
    emb_k_bit: u32,
    emb_1_bit: u32,
    p: u32,
) -> Result<u32> {
    let ms_val = magsgn.fetch();
    let m_n = u_q - emb_k_bit;
    magsgn.advance(m_n)?;
    let val = ms_val << 31; // sign bit
    let mut v_n = ms_val & ((1u32 << m_n) - 1); // keep only m_n bits
    v_n |= emb_1_bit << m_n; // add EMB e_1 as MSB
    v_n |= 1; // add center of bin
    // v_n = 2*(mu-1)+0.5, add 2 to get 2*mu+0.5, shift up by (p-1)
    Ok(val | ((v_n + 2) << (p - 1)))
}

/// Decode one HT codeblock (cleanup pass).
///
/// # Arguments
/// * `data`         - Codeblock compressed data (all passes concatenated)
/// * `width`        - Codeblock width in samples
/// * `height`       - Codeblock height in samples
/// * `num_passes`   - Number of coding passes (1-3)
/// * `lengths`      - Pass lengths: `[cleanup_len, optional_spp_len, optional_mrp_len]`
/// * `zero_bplanes` - Number of zero bit planes
/// * `p`            - Bit depth parameter (numbps)
///
/// # Returns
/// Decoded coefficient array of size `width * height`, stored row-major.
///
/// Currently implements the cleanup pass only. SPP/MRP passes will be
/// added in Phase 700c.
pub fn ht_decode_cblk(
    data: &[u8],
    width: u32,
    height: u32,
    num_passes: u32,
    lengths: &[u32],
    zero_bplanes: u32,
    p: u32,
) -> Result<Vec<i32>> {
    // --- Validate inputs ---
    if num_passes == 0 || num_passes > 3 {
        return Err(Error::InvalidInput(format!(
            "ht_decode_cblk: num_passes ({num_passes}) must be 1, 2, or 3"
        )));
    }
    if width == 0 || height == 0 {
        return Err(Error::InvalidInput(
            "ht_decode_cblk: width and height must be > 0".to_string(),
        ));
    }
    if p > 31 {
        return Err(Error::InvalidInput(format!(
            "ht_decode_cblk: bit depth p ({p}) must be <= 31"
        )));
    }
    if p == 0 {
        // No bits to decode; return all zeros.
        return Ok(vec![0i32; (width * height) as usize]);
    }
    if lengths.is_empty() {
        return Err(Error::InvalidInput(
            "ht_decode_cblk: lengths must not be empty".to_string(),
        ));
    }

    let lcup = lengths[0] as usize; // cleanup pass length
    let zero_bplanes_p1 = zero_bplanes + 1;
    let stride = width as usize;

    // --- Parse scup (last 2 bytes of cleanup segment) ---
    if lcup < 2 || lcup > data.len() {
        return Err(Error::InvalidInput(format!(
            "ht_decode_cblk: invalid cleanup length ({lcup})"
        )));
    }
    let scup = ((data[lcup - 1] as usize) << 4) + ((data[lcup - 2] as usize) & 0xF);
    if scup < 2 || scup > lcup || scup > 4079 {
        return Err(Error::InvalidInput(format!(
            "ht_decode_cblk: invalid scup ({scup}), must be 2 <= scup <= min(lcup={lcup}, 4079)"
        )));
    }

    // --- Initialize bitstream readers ---
    // MEL bitstream: starts at beginning of data, runs for lcup-scup bytes.
    let mel_data = &data[..lcup - scup];
    let mut mel = MelDecoder::new(mel_data);

    // VLC bitstream: reverse reader from end of cleanup segment.
    // The VLC data is the last scup bytes of the cleanup segment.
    let vlc_data = &data[lcup - scup..lcup];
    let mut vlc = RevReader::new(vlc_data);

    // MagSgn bitstream: forward reader, same data as MEL, fill = 0xFF.
    let magsgn_data = &data[..lcup - scup];
    let mut magsgn = FrwdReader::new(magsgn_data, 0xFF);

    // --- Allocate output and working buffers ---
    let total_samples = (width * height) as usize;
    let mut coeffs = vec![0u32; total_samples];

    // sigma buffers: each entry covers 8 columns x 4 rows packed into nibbles.
    // We need ceil(width/8) + 1 entries per buffer. Use 132 (enough for 1024 cols).
    let sigma_buf_len = 132usize;
    let mut sigma1 = vec![0u32; sigma_buf_len];
    let mut sigma2 = vec![0u32; sigma_buf_len];

    // Line state: one byte per 2 columns (quad), need width/2 + 2.
    let line_state_len = (width as usize) / 2 + 2;
    let mut line_state = vec![0u8; line_state_len];

    // --- Cleanup pass: initial 2 rows ---
    let mut lsp_idx: usize = 0; // index into line_state
    line_state[0] = 0;
    let mut run = mel.decode_run()? as i32;
    let mut qinf = [0u32; 2];
    let mut c_q: u32 = 0; // context for quad
    let mut sp: usize = 0; // index into coeffs (first row, moving right)

    // sip = pointer into sigma1, sip_shift tracks nibble position
    let mut sip_idx: usize = 0;
    let mut sip_shift: u32 = 0;
    let sip_is_sigma1 = true; // initial rows always use sigma1
    let _ = sip_is_sigma1;

    let width_i = width as i32;
    let height_i = height as i32;

    for x in (0..width_i).step_by(4) {
        // --- Decode VLC for first quad ---
        let mut vlc_val = vlc.fetch();

        qinf[0] = VLC_TBL0[((c_q << 7) | (vlc_val & 0x7F)) as usize] as u32;

        if c_q == 0 {
            run -= 2;
            qinf[0] = if run == -1 { qinf[0] } else { 0 };
            if run < 0 {
                run = mel.decode_run()? as i32;
            }
        }

        // Prepare context for next quad (eqn. 1 in ITU T.814)
        c_q = ((qinf[0] & 0x10) >> 4) | ((qinf[0] & 0xE0) >> 5);

        // Consume VLC bits
        let vlc_bits0 = qinf[0] & 0x7;
        if vlc_bits0 > 0 {
            vlc.advance(vlc_bits0)?;
        }
        vlc_val = vlc.fetch();

        // Update sigma
        sigma1[sip_idx] |= (((qinf[0] & 0x30) >> 4) | ((qinf[0] & 0xC0) >> 2)) << sip_shift;

        // --- Decode VLC for second quad ---
        qinf[1] = 0;
        if x + 2 < width_i {
            qinf[1] = VLC_TBL0[((c_q << 7) | (vlc_val & 0x7F)) as usize] as u32;

            if c_q == 0 {
                run -= 2;
                qinf[1] = if run == -1 { qinf[1] } else { 0 };
                if run < 0 {
                    run = mel.decode_run()? as i32;
                }
            }

            c_q = ((qinf[1] & 0x10) >> 4) | ((qinf[1] & 0xE0) >> 5);

            let vlc_bits1 = qinf[1] & 0x7;
            if vlc_bits1 > 0 {
                vlc.advance(vlc_bits1)?;
            }
            vlc_val = vlc.fetch();
        }

        // Update sigma for second quad
        sigma1[sip_idx] |= ((qinf[1] & 0x30) | ((qinf[1] & 0xC0) << 2)) << (4 + sip_shift);

        if x & 0x7 != 0 {
            sip_idx += 1;
        }
        sip_shift ^= 0x10;

        // --- Retrieve u values ---
        let uvlc_mode = ((qinf[0] & 0x8) >> 3) | ((qinf[1] & 0x8) >> 2);
        let uvlc_mode = if uvlc_mode == 3 {
            run -= 2;
            let m = uvlc_mode + if run == -1 { 1 } else { 0 };
            if run < 0 {
                run = mel.decode_run()? as i32;
            }
            m
        } else {
            uvlc_mode
        };

        let (consumed_bits, u_q) = decode_init_uvlc(vlc_val, uvlc_mode);
        if u_q[0] > zero_bplanes_p1 || u_q[1] > zero_bplanes_p1 {
            return Err(Error::InvalidInput(
                "ht_decode_cblk: U_q exceeds zero bitplanes + 1".to_string(),
            ));
        }

        if consumed_bits > 0 {
            vlc.advance(consumed_bits)?;
        }

        // --- Decode MagSgn samples ---
        let mut locs: u32 = 0xFF;
        if x + 4 > width_i {
            locs >>= ((x + 4 - width_i) << 1) as u32;
        }
        if height_i <= 1 {
            locs &= 0x55; // only odd-numbered locs (row 0 only)
        }

        // Check for out-of-bounds significance
        if (((qinf[0] & 0xF0) >> 4) | (qinf[1] & 0xF0)) & !locs != 0 {
            return Err(Error::InvalidInput(
                "ht_decode_cblk: VLC produces significant samples outside codeblock".to_string(),
            ));
        }

        // First quad, sample 0 (row 0, col x)
        if qinf[0] & 0x10 != 0 {
            coeffs[sp] = decode_one_sample(
                &mut magsgn,
                u_q[0],
                (qinf[0] >> 12) & 1,
                (qinf[0] & 0x100) >> 8,
                p,
            )?;
        }

        // First quad, sample 1 (row 1, col x)
        if qinf[0] & 0x20 != 0 {
            coeffs[sp + stride] = decode_one_sample(
                &mut magsgn,
                u_q[0],
                (qinf[0] >> 13) & 1,
                (qinf[0] & 0x200) >> 9,
                p,
            )?;

            // Update line_state
            let t = line_state[lsp_idx] & 0x7F;
            let ms_val_bits = magsgn.fetch();
            // Recompute v_n for line state (we need the v_n that was just decoded)
            // Use the coefficient we just stored to extract v_n
            let stored = coeffs[sp + stride];
            let v_n_full = (stored & 0x7FFF_FFFF) >> (p - 1); // extract (v_n + 2)
            let v_n = v_n_full.saturating_sub(2);
            let _ = ms_val_bits;
            let e_n = 32 - v_n.leading_zeros();
            line_state[lsp_idx] = 0x80 | if t as u32 > e_n { t } else { e_n as u8 };
        }

        lsp_idx += 1;
        sp += 1;

        // First quad, sample 2 (row 0, col x+1)
        if qinf[0] & 0x40 != 0 {
            coeffs[sp] = decode_one_sample(
                &mut magsgn,
                u_q[0],
                (qinf[0] >> 14) & 1,
                (qinf[0] & 0x400) >> 10,
                p,
            )?;
        }

        // First quad, sample 3 (row 1, col x+1)
        line_state[lsp_idx] = 0;
        if qinf[0] & 0x80 != 0 {
            coeffs[sp + stride] = decode_one_sample(
                &mut magsgn,
                u_q[0],
                (qinf[0] >> 15) & 1,
                (qinf[0] & 0x800) >> 11,
                p,
            )?;

            let stored = coeffs[sp + stride];
            let v_n_full = (stored & 0x7FFF_FFFF) >> (p - 1);
            let v_n = v_n_full.saturating_sub(2);
            let e_nw = 32 - v_n.leading_zeros();
            line_state[lsp_idx] = 0x80 | e_nw as u8;
        }

        sp += 1;

        // Second quad, sample 0 (row 0, col x+2)
        if qinf[1] & 0x10 != 0 {
            coeffs[sp] = decode_one_sample(
                &mut magsgn,
                u_q[1],
                (qinf[1] >> 12) & 1,
                (qinf[1] & 0x100) >> 8,
                p,
            )?;
        }

        // Second quad, sample 1 (row 1, col x+2)
        if qinf[1] & 0x20 != 0 {
            coeffs[sp + stride] = decode_one_sample(
                &mut magsgn,
                u_q[1],
                (qinf[1] >> 13) & 1,
                (qinf[1] & 0x200) >> 9,
                p,
            )?;

            let t = line_state[lsp_idx] & 0x7F;
            let stored = coeffs[sp + stride];
            let v_n_full = (stored & 0x7FFF_FFFF) >> (p - 1);
            let v_n = v_n_full.saturating_sub(2);
            let e_n = 32 - v_n.leading_zeros();
            line_state[lsp_idx] = 0x80 | if t as u32 > e_n { t } else { e_n as u8 };
        }

        lsp_idx += 1;
        sp += 1;

        // Second quad, sample 2 (row 0, col x+3)
        if qinf[1] & 0x40 != 0 {
            coeffs[sp] = decode_one_sample(
                &mut magsgn,
                u_q[1],
                (qinf[1] >> 14) & 1,
                (qinf[1] & 0x400) >> 10,
                p,
            )?;
        }

        // Second quad, sample 3 (row 1, col x+3)
        line_state[lsp_idx] = 0;
        if qinf[1] & 0x80 != 0 {
            coeffs[sp + stride] = decode_one_sample(
                &mut magsgn,
                u_q[1],
                (qinf[1] >> 15) & 1,
                (qinf[1] & 0x800) >> 11,
                p,
            )?;

            let stored = coeffs[sp + stride];
            let v_n_full = (stored & 0x7FFF_FFFF) >> (p - 1);
            let v_n = v_n_full.saturating_sub(2);
            let e_nw = 32 - v_n.leading_zeros();
            line_state[lsp_idx] = 0x80 | e_nw as u8;
        }

        sp += 1;
    }

    // --- Cleanup pass: non-initial rows (y = 2, 4, 6, ...) ---
    let mut y = 2i32;
    while y < height_i {
        sip_shift ^= 0x2;
        sip_shift &= 0xFFFF_FFEFu32;
        let use_sigma2 = y & 0x4 != 0;

        lsp_idx = 0;
        let ls0_saved = line_state[0];
        let mut ls0 = ls0_saved;
        line_state[0] = 0;
        sp = (y as usize) * stride;
        c_q = 0;
        sip_idx = 0;

        for x in (0..width_i).step_by(4) {
            // --- First quad context and VLC ---
            c_q |= (ls0 >> 7) as u32;
            c_q |= ((line_state[lsp_idx + 1] >> 5) & 0x4) as u32;

            let mut vlc_val = vlc.fetch();
            qinf[0] = VLC_TBL1[((c_q << 7) | (vlc_val & 0x7F)) as usize] as u32;

            if c_q == 0 {
                run -= 2;
                qinf[0] = if run == -1 { qinf[0] } else { 0 };
                if run < 0 {
                    run = mel.decode_run()? as i32;
                }
            }

            // Prepare context: sigma^W | sigma^SW
            c_q = ((qinf[0] & 0x40) >> 5) | ((qinf[0] & 0x80) >> 6);

            let vlc_bits0 = qinf[0] & 0x7;
            if vlc_bits0 > 0 {
                vlc.advance(vlc_bits0)?;
            }
            vlc_val = vlc.fetch();

            // Update sigma
            let sip_buf = if use_sigma2 { &mut sigma2 } else { &mut sigma1 };
            sip_buf[sip_idx] |= (((qinf[0] & 0x30) >> 4) | ((qinf[0] & 0xC0) >> 2)) << sip_shift;

            // --- Second quad context and VLC ---
            qinf[1] = 0;
            if x + 2 < width_i {
                c_q |= (line_state[lsp_idx + 1] >> 7) as u32;
                c_q |= ((line_state[lsp_idx + 2] >> 5) & 0x4) as u32;

                qinf[1] = VLC_TBL1[((c_q << 7) | (vlc_val & 0x7F)) as usize] as u32;

                if c_q == 0 {
                    run -= 2;
                    qinf[1] = if run == -1 { qinf[1] } else { 0 };
                    if run < 0 {
                        run = mel.decode_run()? as i32;
                    }
                }

                c_q = ((qinf[1] & 0x40) >> 5) | ((qinf[1] & 0x80) >> 6);

                let vlc_bits1 = qinf[1] & 0x7;
                if vlc_bits1 > 0 {
                    vlc.advance(vlc_bits1)?;
                }
                vlc_val = vlc.fetch();
            }

            // Update sigma for second quad
            let sip_buf = if use_sigma2 { &mut sigma2 } else { &mut sigma1 };
            sip_buf[sip_idx] |= ((qinf[1] & 0x30) | ((qinf[1] & 0xC0) << 2)) << (4 + sip_shift);

            if x & 0x7 != 0 {
                sip_idx += 1;
            }
            sip_shift ^= 0x10;

            // --- Retrieve u values ---
            let uvlc_mode = ((qinf[0] & 0x8) >> 3) | ((qinf[1] & 0x8) >> 2);
            let (consumed_bits, mut u_q) = decode_noninit_uvlc(vlc_val, uvlc_mode);
            if consumed_bits > 0 {
                vlc.advance(consumed_bits)?;
            }

            // Calculate E^max and add to U_q (eqns 5 and 6 in ITU T.814)
            if (qinf[0] & 0xF0) & ((qinf[0] & 0xF0).wrapping_sub(1)) != 0 {
                let e = (ls0 & 0x7F) as u32;
                let e = e.max((line_state[lsp_idx + 1] & 0x7F) as u32);
                u_q[0] += e.saturating_sub(2);
            }
            if (qinf[1] & 0xF0) & ((qinf[1] & 0xF0).wrapping_sub(1)) != 0 {
                let e = (line_state[lsp_idx + 1] & 0x7F) as u32;
                let e = e.max((line_state[lsp_idx + 2] & 0x7F) as u32);
                u_q[1] += e.saturating_sub(2);
            }

            if u_q[0] > zero_bplanes_p1 || u_q[1] > zero_bplanes_p1 {
                return Err(Error::InvalidInput(
                    "ht_decode_cblk: U_q exceeds zero bitplanes + 1".to_string(),
                ));
            }

            ls0 = line_state[lsp_idx + 2];
            line_state[lsp_idx + 1] = 0;
            line_state[lsp_idx + 2] = 0;

            // --- Decode MagSgn samples ---
            let mut locs: u32 = 0xFF;
            if x + 4 > width_i {
                locs >>= ((x + 4 - width_i) << 1) as u32;
            }
            if y + 2 > height_i {
                locs &= 0x55;
            }

            if (((qinf[0] & 0xF0) >> 4) | (qinf[1] & 0xF0)) & !locs != 0 {
                return Err(Error::InvalidInput(
                    "ht_decode_cblk: VLC produces significant samples outside codeblock"
                        .to_string(),
                ));
            }

            // First quad, sample 0
            if qinf[0] & 0x10 != 0 {
                coeffs[sp] = decode_one_sample(
                    &mut magsgn,
                    u_q[0],
                    (qinf[0] >> 12) & 1,
                    (qinf[0] & 0x100) >> 8,
                    p,
                )?;
            } else if locs & 0x1 != 0 {
                coeffs[sp] = 0;
            }

            // First quad, sample 1
            if qinf[0] & 0x20 != 0 {
                coeffs[sp + stride] = decode_one_sample(
                    &mut magsgn,
                    u_q[0],
                    (qinf[0] >> 13) & 1,
                    (qinf[0] & 0x200) >> 9,
                    p,
                )?;

                let t = line_state[lsp_idx] & 0x7F;
                let stored = coeffs[sp + stride];
                let v_n_full = (stored & 0x7FFF_FFFF) >> (p - 1);
                let v_n = v_n_full.saturating_sub(2);
                let e_n = 32 - v_n.leading_zeros();
                line_state[lsp_idx] = 0x80 | if t as u32 > e_n { t } else { e_n as u8 };
            } else if locs & 0x2 != 0 {
                coeffs[sp + stride] = 0;
            }

            lsp_idx += 1;
            sp += 1;

            // First quad, sample 2
            if qinf[0] & 0x40 != 0 {
                coeffs[sp] = decode_one_sample(
                    &mut magsgn,
                    u_q[0],
                    (qinf[0] >> 14) & 1,
                    (qinf[0] & 0x400) >> 10,
                    p,
                )?;
            } else if locs & 0x4 != 0 {
                coeffs[sp] = 0;
            }

            // First quad, sample 3
            if qinf[0] & 0x80 != 0 {
                coeffs[sp + stride] = decode_one_sample(
                    &mut magsgn,
                    u_q[0],
                    (qinf[0] >> 15) & 1,
                    (qinf[0] & 0x800) >> 11,
                    p,
                )?;

                let stored = coeffs[sp + stride];
                let v_n_full = (stored & 0x7FFF_FFFF) >> (p - 1);
                let v_n = v_n_full.saturating_sub(2);
                let e_nw = 32 - v_n.leading_zeros();
                line_state[lsp_idx] = 0x80 | e_nw as u8;
            } else if locs & 0x8 != 0 {
                coeffs[sp + stride] = 0;
            }

            sp += 1;

            // Second quad, sample 0
            if qinf[1] & 0x10 != 0 {
                coeffs[sp] = decode_one_sample(
                    &mut magsgn,
                    u_q[1],
                    (qinf[1] >> 12) & 1,
                    (qinf[1] & 0x100) >> 8,
                    p,
                )?;
            } else if locs & 0x10 != 0 {
                coeffs[sp] = 0;
            }

            // Second quad, sample 1
            if qinf[1] & 0x20 != 0 {
                coeffs[sp + stride] = decode_one_sample(
                    &mut magsgn,
                    u_q[1],
                    (qinf[1] >> 13) & 1,
                    (qinf[1] & 0x200) >> 9,
                    p,
                )?;

                let t = line_state[lsp_idx] & 0x7F;
                let stored = coeffs[sp + stride];
                let v_n_full = (stored & 0x7FFF_FFFF) >> (p - 1);
                let v_n = v_n_full.saturating_sub(2);
                let e_n = 32 - v_n.leading_zeros();
                line_state[lsp_idx] = 0x80 | if t as u32 > e_n { t } else { e_n as u8 };
            } else if locs & 0x20 != 0 {
                coeffs[sp + stride] = 0;
            }

            lsp_idx += 1;
            sp += 1;

            // Second quad, sample 2
            if qinf[1] & 0x40 != 0 {
                coeffs[sp] = decode_one_sample(
                    &mut magsgn,
                    u_q[1],
                    (qinf[1] >> 14) & 1,
                    (qinf[1] & 0x400) >> 10,
                    p,
                )?;
            } else if locs & 0x40 != 0 {
                coeffs[sp] = 0;
            }

            // Second quad, sample 3
            if qinf[1] & 0x80 != 0 {
                coeffs[sp + stride] = decode_one_sample(
                    &mut magsgn,
                    u_q[1],
                    (qinf[1] >> 15) & 1,
                    (qinf[1] & 0x800) >> 11,
                    p,
                )?;

                let stored = coeffs[sp + stride];
                let v_n_full = (stored & 0x7FFF_FFFF) >> (p - 1);
                let v_n = v_n_full.saturating_sub(2);
                let e_nw = 32 - v_n.leading_zeros();
                line_state[lsp_idx] = 0x80 | e_nw as u8;
            } else if locs & 0x80 != 0 {
                coeffs[sp + stride] = 0;
            }

            sp += 1;
        }

        y += 2;
    }

    // --- Convert u32 coefficients to i32 (sign-magnitude to two's complement) ---
    let result: Vec<i32> = coeffs
        .iter()
        .map(|&c| {
            if c == 0 {
                0i32
            } else if c & 0x8000_0000 != 0 {
                // Negative: sign bit is set
                -((c & 0x7FFF_FFFF) as i32)
            } else {
                c as i32
            }
        })
        .collect();

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Tests: helper functions
    // -----------------------------------------------------------------------

    #[test]
    fn count_leading_zeros_basic() {
        assert_eq!(count_leading_zeros(0), 32);
        assert_eq!(count_leading_zeros(1), 31);
        assert_eq!(count_leading_zeros(0x8000_0000), 0);
        assert_eq!(count_leading_zeros(0x0000_0100), 23);
    }

    #[test]
    fn population_count_basic() {
        assert_eq!(population_count(0), 0);
        assert_eq!(population_count(0xFFFF_FFFF), 32);
        assert_eq!(population_count(0x5555_5555), 16);
        assert_eq!(population_count(0b1010_1010), 4);
    }

    // -----------------------------------------------------------------------
    // Tests: MEL decoder
    // -----------------------------------------------------------------------

    #[test]
    fn mel_decoder_init() {
        // MEL data with no unstuffing needed
        let data = [0x00, 0x00, 0x00, 0x00];
        let mel = MelDecoder::new(&data);
        assert_eq!(mel.k, 0);
        assert_eq!(mel.num_runs, 0);
    }

    #[test]
    fn mel_decoder_single_run() {
        // Simple MEL: a single '1' bit means run=0 (event immediately)
        // when k=0, '1' → run = 0, k stays at 0
        // when k=0, '0' followed by nothing → run = 1
        let data = [0x80, 0x00]; // bit pattern: 1 0 0 0 ...
        let mut mel = MelDecoder::new(&data);
        let run = mel.decode_run().unwrap();
        assert_eq!(run, 0); // '1' bit at k=0 → run=0
    }

    #[test]
    fn mel_decoder_unstuffing() {
        // After 0xFF, the MSB of the next byte is stripped
        let data = [0xFF, 0x7F]; // 0xFF followed by 0x7F (MSB=0, stripped)
        let mel = MelDecoder::new(&data);
        // Just verify initialization succeeds
        let _ = &mel;
    }

    // -----------------------------------------------------------------------
    // Tests: Reverse reader
    // -----------------------------------------------------------------------

    #[test]
    fn rev_reader_init_and_fetch() {
        // 4 bytes: [0x12, 0x34, 0x56, 0x78]
        // Reverse reader starts from end, so first bytes read are 0x78, 0x56, ...
        let data = [0x12, 0x34, 0x56, 0x78];
        let reader = RevReader::new(&data);
        let bits = reader.fetch();
        // First byte read backwards is 0x78 = 0b0111_1000
        // Expect these bits at LSB of fetch result
        assert_ne!(bits, 0);
    }

    #[test]
    fn rev_reader_advance_consumes_bits() {
        let data = [0x12, 0x34, 0x56, 0x78];
        let mut reader = RevReader::new(&data);
        let bits1 = reader.fetch();
        reader.advance(4).unwrap();
        let bits2 = reader.fetch();
        // After consuming 4 bits, the result should differ
        assert_ne!(bits1, bits2);
    }

    #[test]
    fn rev_reader_unstuffing() {
        // Data ending with 0xFF requires unstuffing
        let data = [0x00, 0xFF];
        let reader = RevReader::new(&data);
        // Should handle 0xFF unstuffing correctly
        let _ = reader.fetch();
    }

    // -----------------------------------------------------------------------
    // Tests: Forward reader
    // -----------------------------------------------------------------------

    #[test]
    fn frwd_reader_init_and_fetch() {
        let data = [0xAB, 0xCD, 0xEF, 0x01];
        let reader = FrwdReader::new(&data, 0xFF);
        let bits = reader.fetch();
        // First byte forward is 0xAB
        assert_ne!(bits, 0);
    }

    #[test]
    fn frwd_reader_advance_consumes_bits() {
        let data = [0xAB, 0xCD, 0xEF, 0x01];
        let mut reader = FrwdReader::new(&data, 0xFF);
        let bits1 = reader.fetch();
        reader.advance(8).unwrap();
        let bits2 = reader.fetch();
        assert_ne!(bits1, bits2);
    }

    #[test]
    fn frwd_reader_fill_on_exhaustion() {
        // 2-byte data, read more than available → fills with fill_byte
        let data = [0x42, 0x43];
        let mut reader = FrwdReader::new(&data, 0xFF);
        // Consume all real data
        reader.advance(16).unwrap();
        let bits = reader.fetch();
        // Should be filled with 0xFF bytes
        assert_eq!(bits & 0xFF, 0xFF);
    }

    #[test]
    fn frwd_reader_unstuffing() {
        // After 0xFF, MSB of next byte is stripped
        let data = [0xFF, 0x7F, 0x00];
        let reader = FrwdReader::new(&data, 0xFF);
        let _ = reader.fetch();
    }

    // -----------------------------------------------------------------------
    // Tests: UVLC decoders
    // -----------------------------------------------------------------------

    #[test]
    fn decode_init_uvlc_mode0() {
        let (consumed, u) = decode_init_uvlc(0, 0);
        assert_eq!(consumed, 0);
        assert_eq!(u, [1, 1]);
    }

    #[test]
    fn decode_init_uvlc_mode1() {
        // VLC bits: 1 (prefix "1" -> u_pfx=1, suffix_len=0)
        let (consumed, u) = decode_init_uvlc(0b1, 1);
        assert_eq!(consumed, 1);
        assert_eq!(u[0], 2); // d=1, u=1+1=2
        assert_eq!(u[1], 1);
    }

    #[test]
    fn decode_init_uvlc_mode2() {
        let (consumed, u) = decode_init_uvlc(0b1, 2);
        assert_eq!(consumed, 1);
        assert_eq!(u[0], 1);
        assert_eq!(u[1], 2);
    }

    #[test]
    fn decode_init_uvlc_mode1_prefix01() {
        // VLC bits: 10 (prefix "01" -> u_pfx=2, suffix_len=0)
        let (consumed, u) = decode_init_uvlc(0b10, 1);
        assert_eq!(consumed, 2);
        assert_eq!(u[0], 3); // d=2, u=2+1=3
        assert_eq!(u[1], 1);
    }

    #[test]
    fn decode_init_uvlc_mode4() {
        // Mode 4: both u_off=1, MEL event=1, each gets +2+kappa(=1)=+3
        // Two prefix "1" codewords
        let (consumed, u) = decode_init_uvlc(0b11, 4);
        assert_eq!(consumed, 2);
        assert_eq!(u[0], 4); // d=1, u=1+3=4
        assert_eq!(u[1], 4);
    }

    #[test]
    fn decode_noninit_uvlc_mode0() {
        let (consumed, u) = decode_noninit_uvlc(0, 0);
        assert_eq!(consumed, 0);
        assert_eq!(u, [1, 1]);
    }

    #[test]
    fn decode_noninit_uvlc_mode3() {
        // Two symbols, each "1" prefix
        let (consumed, u) = decode_noninit_uvlc(0b11, 3);
        assert_eq!(consumed, 2);
        assert_eq!(u[0], 2); // d=1, u=1+1
        assert_eq!(u[1], 2);
    }

    #[test]
    fn decode_noninit_uvlc_mode1() {
        let (consumed, u) = decode_noninit_uvlc(0b1, 1);
        assert_eq!(consumed, 1);
        assert_eq!(u[0], 2);
        assert_eq!(u[1], 1);
    }

    #[test]
    fn decode_noninit_uvlc_mode2() {
        let (consumed, u) = decode_noninit_uvlc(0b1, 2);
        assert_eq!(consumed, 1);
        assert_eq!(u[0], 1);
        assert_eq!(u[1], 2);
    }

    // -----------------------------------------------------------------------
    // Tests: HT codeblock decode
    // -----------------------------------------------------------------------

    #[test]
    fn ht_decode_cblk_rejects_invalid_inputs() {
        // Zero passes
        let result = ht_decode_cblk(&[0; 10], 4, 4, 0, &[10], 0, 8);
        assert!(result.is_err());

        // Too many passes
        let result = ht_decode_cblk(&[0; 10], 4, 4, 4, &[10], 0, 8);
        assert!(result.is_err());

        // Zero width
        let result = ht_decode_cblk(&[0; 10], 0, 4, 1, &[10], 0, 8);
        assert!(result.is_err());

        // Zero height
        let result = ht_decode_cblk(&[0; 10], 4, 0, 1, &[10], 0, 8);
        assert!(result.is_err());

        // Bit depth > 31
        let result = ht_decode_cblk(&[0; 10], 4, 4, 1, &[10], 0, 32);
        assert!(result.is_err());
    }

    #[test]
    fn ht_decode_cblk_zero_bitdepth() {
        // When p=0, all coefficients should be zero
        let result = ht_decode_cblk(&[0; 10], 4, 2, 1, &[10], 0, 0).unwrap();
        assert_eq!(result.len(), 8);
        assert!(result.iter().all(|&v| v == 0));
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn ht_decode_cblk_minimal_allzero() {
        // Construct a minimal valid HT codeblock that decodes to all-zero
        // coefficients. This requires a valid MEL/VLC/MagSgn bitstream where
        // all quads are non-significant (all runs = 0).
        //
        // For an all-zero block:
        // - MEL stream produces runs of all-zero events
        // - VLC lookup with context 0 and MEL event 0 yields qinf=0
        // - scup = 2 (minimal MEL+VLC segment)
        //
        // This test will be fully implemented when we have a reference
        // bitstream generator or can validate against the C decoder.
        let width = 4u32;
        let height = 2u32;
        let p = 8u32;

        // Placeholder: we need a properly constructed bitstream
        let data = vec![0u8; 16];
        let result = ht_decode_cblk(&data, width, height, 1, &[16], 0, p).unwrap();
        assert_eq!(result.len(), (width * height) as usize);
        assert!(result.iter().all(|&v| v == 0));
    }
}
