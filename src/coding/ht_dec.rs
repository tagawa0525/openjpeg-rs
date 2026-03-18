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
}
