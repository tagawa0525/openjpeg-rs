use crate::error::{Error, Result};

/// Bit I/O encoder/decoder (C: opj_bio_t).
///
/// Uses a 16-bit buffer (`buf`) with JPEG 2000 bit stuffing:
/// after writing 0xFF, only 7 bits are available for the next byte.
pub struct Bio<'a> {
    data: &'a mut [u8],
    pos: usize,
    buf: u32,
    ct: u32,
}

impl<'a> Bio<'a> {
    /// Create encoder. `ct=8` means 8 bits free to write.
    pub fn encoder(buf: &'a mut [u8]) -> Self {
        Self {
            data: buf,
            pos: 0,
            buf: 0,
            ct: 8,
        }
    }

    /// Create decoder. `ct=0` means no bits read yet (triggers bytein on first read).
    pub fn decoder(buf: &'a mut [u8]) -> Self {
        Self {
            data: buf,
            pos: 0,
            buf: 0,
            ct: 0,
        }
    }

    /// Write `n` bits of value `v` (MSB first).
    pub fn write(&mut self, v: u32, n: u32) -> Result<()> {
        debug_assert!(n > 0 && n <= 32);
        for i in (0..n).rev() {
            self.put_bit((v >> i) & 1)?;
        }
        Ok(())
    }

    /// Read `n` bits (MSB first).
    pub fn read(&mut self, n: u32) -> Result<u32> {
        debug_assert!(n > 0 && n <= 32);
        let mut v = 0u32;
        for i in (0..n).rev() {
            v |= self.get_bit()? << i;
        }
        Ok(v)
    }

    /// Flush remaining bits to output.
    pub fn flush(&mut self) -> Result<()> {
        self.byte_out()?;
        if self.ct == 7 {
            self.byte_out()?;
        }
        Ok(())
    }

    /// Align decoder to byte boundary.
    pub fn inalign(&mut self) -> Result<()> {
        if (self.buf & 0xff) == 0xff {
            self.byte_in()?;
        }
        self.ct = 0;
        Ok(())
    }

    /// Number of bytes written/read so far.
    pub fn num_bytes(&self) -> usize {
        self.pos
    }

    fn put_bit(&mut self, b: u32) -> Result<()> {
        if self.ct == 0 {
            self.byte_out()?;
        }
        self.ct -= 1;
        self.buf |= b << self.ct;
        Ok(())
    }

    fn get_bit(&mut self) -> Result<u32> {
        if self.ct == 0 {
            self.byte_in()?;
        }
        self.ct -= 1;
        Ok((self.buf >> self.ct) & 1)
    }

    /// Write the current byte to output buffer.
    /// Shifts buf left by 8, applies 0xFFFF mask.
    /// If previous byte was 0xFF, next byte allows only 7 bits.
    fn byte_out(&mut self) -> Result<()> {
        self.buf = (self.buf << 8) & 0xffff;
        self.ct = if self.buf == 0xff00 { 7 } else { 8 };
        if self.pos >= self.data.len() {
            return Err(Error::BufferTooSmall);
        }
        self.data[self.pos] = (self.buf >> 8) as u8;
        self.pos += 1;
        Ok(())
    }

    /// Read next byte from input buffer.
    fn byte_in(&mut self) -> Result<()> {
        self.buf = (self.buf << 8) & 0xffff;
        self.ct = if self.buf == 0xff00 { 7 } else { 8 };
        if self.pos >= self.data.len() {
            return Err(Error::EndOfStream);
        }
        self.buf |= self.data[self.pos] as u32;
        self.pos += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_single_bits() {
        let mut buf = [0u8; 4];
        {
            let mut enc = Bio::encoder(&mut buf);
            enc.write(1, 1).unwrap();
            enc.write(0, 1).unwrap();
            enc.write(1, 1).unwrap();
            enc.write(1, 1).unwrap();
            enc.flush().unwrap();
        }
        {
            let mut dec = Bio::decoder(&mut buf);
            assert_eq!(dec.read(1).unwrap(), 1);
            assert_eq!(dec.read(1).unwrap(), 0);
            assert_eq!(dec.read(1).unwrap(), 1);
            assert_eq!(dec.read(1).unwrap(), 1);
        }
    }

    #[test]
    fn roundtrip_multi_bits() {
        let mut buf = [0u8; 8];
        {
            let mut enc = Bio::encoder(&mut buf);
            enc.write(0b1010, 4).unwrap();
            enc.write(0b11001100, 8).unwrap();
            enc.write(255, 8).unwrap();
            enc.flush().unwrap();
        }
        {
            let mut dec = Bio::decoder(&mut buf);
            assert_eq!(dec.read(4).unwrap(), 0b1010);
            assert_eq!(dec.read(8).unwrap(), 0b11001100);
            assert_eq!(dec.read(8).unwrap(), 255);
        }
    }

    #[test]
    fn num_bytes_after_flush() {
        let mut buf = [0u8; 8];
        let mut enc = Bio::encoder(&mut buf);
        enc.write(0b1010, 4).unwrap();
        enc.flush().unwrap();
        assert!(enc.num_bytes() >= 1);
    }

    #[test]
    fn ff_stuffing() {
        let mut buf = [0u8; 4];
        {
            let mut enc = Bio::encoder(&mut buf);
            enc.write(0xFF, 8).unwrap();
            enc.write(0b1010101, 7).unwrap();
            enc.flush().unwrap();
        }
        {
            let mut dec = Bio::decoder(&mut buf);
            assert_eq!(dec.read(8).unwrap(), 0xFF);
            assert_eq!(dec.read(7).unwrap(), 0b1010101);
        }
    }

    #[test]
    fn encoder_buffer_too_small() {
        let mut buf = [0u8; 1];
        let mut enc = Bio::encoder(&mut buf);
        enc.write(0xFF, 8).unwrap();
        let result = enc.flush();
        assert!(result.is_err());
    }

    #[test]
    fn roundtrip_32bit_value() {
        let mut buf = [0u8; 8];
        let val = 0xDEADBEEF_u32;
        {
            let mut enc = Bio::encoder(&mut buf);
            enc.write(val, 32).unwrap();
            enc.flush().unwrap();
        }
        {
            let mut dec = Bio::decoder(&mut buf);
            assert_eq!(dec.read(32).unwrap(), val);
        }
    }

    #[test]
    fn inalign_basic() {
        let mut buf = [0u8; 4];
        {
            let mut enc = Bio::encoder(&mut buf);
            enc.write(0b101, 3).unwrap();
            enc.flush().unwrap();
        }
        {
            let mut dec = Bio::decoder(&mut buf);
            let _ = dec.read(3).unwrap();
            dec.inalign().unwrap();
        }
    }
}
