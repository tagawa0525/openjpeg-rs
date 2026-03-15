use crate::error::{Error, Result};

/// Memory-backed byte stream (C: opj_stream_private_t).
pub struct MemoryStream {
    data: Vec<u8>,
    position: usize,
    is_input: bool,
}

impl MemoryStream {
    /// Create an input stream from existing data.
    pub fn new_input(data: Vec<u8>) -> Self {
        Self {
            data,
            position: 0,
            is_input: true,
        }
    }

    /// Create an empty output stream.
    pub fn new_output() -> Self {
        Self {
            data: Vec::new(),
            position: 0,
            is_input: false,
        }
    }

    /// Read bytes into `buf`. Returns actual bytes read.
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if !self.is_input {
            return Err(Error::InvalidInput("cannot read from output stream".into()));
        }
        let available = self.data.len().saturating_sub(self.position);
        let n = buf.len().min(available);
        buf[..n].copy_from_slice(&self.data[self.position..self.position + n]);
        self.position += n;
        Ok(n)
    }

    /// Write bytes from `buf`. Returns actual bytes written.
    pub fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if self.is_input {
            return Err(Error::InvalidInput("cannot write to input stream".into()));
        }
        let end = self.position + buf.len();
        if end > self.data.len() {
            self.data.resize(end, 0);
        }
        self.data[self.position..end].copy_from_slice(buf);
        self.position = end;
        Ok(buf.len())
    }

    /// Skip `n` bytes forward (positive) or backward (negative).
    pub fn skip(&mut self, n: i64) -> Result<()> {
        let new_pos = if n >= 0 {
            self.position.checked_add(n as usize)
        } else {
            self.position.checked_sub((-n) as usize)
        };
        match new_pos {
            Some(p) if p <= self.data.len() => {
                self.position = p;
                Ok(())
            }
            _ => Err(Error::InvalidInput("skip out of bounds".into())),
        }
    }

    /// Seek to absolute position.
    pub fn seek(&mut self, pos: usize) -> Result<()> {
        if pos > self.data.len() {
            return Err(Error::InvalidInput("seek out of bounds".into()));
        }
        self.position = pos;
        Ok(())
    }

    /// Current position in stream.
    pub fn tell(&self) -> usize {
        self.position
    }

    /// Bytes remaining from current position.
    pub fn bytes_left(&self) -> usize {
        self.data.len().saturating_sub(self.position)
    }

    /// Access underlying data.
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

// --- Byte order conversion functions ---

/// Write `val` as `n` big-endian bytes to `buf` (C: opj_write_bytes_BE).
///
/// # Panics (debug)
/// Panics if `n` is not in `1..=4` or `buf.len() < n`.
pub fn write_bytes_be(buf: &mut [u8], val: u32, n: usize) {
    debug_assert!((1..=4).contains(&n), "write_bytes_be: n must be in 1..=4");
    debug_assert!(buf.len() >= n, "write_bytes_be: buffer too small");
    let bytes = val.to_be_bytes();
    buf[..n].copy_from_slice(&bytes[4 - n..]);
}

/// Read `n` big-endian bytes from `buf` as u32 (C: opj_read_bytes_BE).
///
/// # Panics (debug)
/// Panics if `n` is not in `1..=4` or `buf.len() < n`.
pub fn read_bytes_be(buf: &[u8], n: usize) -> u32 {
    debug_assert!((1..=4).contains(&n), "read_bytes_be: n must be in 1..=4");
    debug_assert!(buf.len() >= n, "read_bytes_be: buffer too small");
    let mut bytes = [0u8; 4];
    bytes[4 - n..].copy_from_slice(&buf[..n]);
    u32::from_be_bytes(bytes)
}

/// Write f64 as big-endian (C: opj_write_double_BE).
pub fn write_f64_be(buf: &mut [u8; 8], val: f64) {
    *buf = val.to_be_bytes();
}

/// Read f64 from big-endian (C: opj_read_double_BE).
pub fn read_f64_be(buf: &[u8; 8]) -> f64 {
    f64::from_be_bytes(*buf)
}

/// Write f32 as big-endian (C: opj_write_float_BE).
pub fn write_f32_be(buf: &mut [u8; 4], val: f32) {
    *buf = val.to_be_bytes();
}

/// Read f32 from big-endian (C: opj_read_float_BE).
pub fn read_f32_be(buf: &[u8; 4]) -> f32 {
    f32::from_be_bytes(*buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Byte order conversion ---

    #[test]
    fn write_read_bytes_be_1() {
        let mut buf = [0u8; 4];
        write_bytes_be(&mut buf, 0xAB, 1);
        assert_eq!(read_bytes_be(&buf, 1), 0xAB);
    }

    #[test]
    fn write_read_bytes_be_2() {
        let mut buf = [0u8; 4];
        write_bytes_be(&mut buf, 0xABCD, 2);
        assert_eq!(buf[0], 0xAB);
        assert_eq!(buf[1], 0xCD);
        assert_eq!(read_bytes_be(&buf, 2), 0xABCD);
    }

    #[test]
    fn write_read_bytes_be_4() {
        let mut buf = [0u8; 4];
        write_bytes_be(&mut buf, 0xDEADBEEF, 4);
        assert_eq!(read_bytes_be(&buf, 4), 0xDEADBEEF);
    }

    #[test]
    fn write_read_f64_be() {
        let mut buf = [0u8; 8];
        let val = 3.14159265358980_f64;
        write_f64_be(&mut buf, val);
        let read_val = read_f64_be(&buf);
        assert_eq!(val.to_bits(), read_val.to_bits());
    }

    #[test]
    fn write_read_f32_be() {
        let mut buf = [0u8; 4];
        let val = 2.71829_f32;
        write_f32_be(&mut buf, val);
        let read_val = read_f32_be(&buf);
        assert_eq!(val.to_bits(), read_val.to_bits());
    }

    // --- MemoryStream ---

    #[test]
    fn output_stream_write_and_read_back() {
        let mut stream = MemoryStream::new_output();
        let data = b"Hello, JPEG 2000!";
        stream.write(data).unwrap();
        assert_eq!(stream.tell(), data.len());
        assert_eq!(&stream.data()[..data.len()], data);
    }

    #[test]
    fn input_stream_read() {
        let data = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let mut stream = MemoryStream::new_input(data);
        let mut buf = [0u8; 2];
        let n = stream.read(&mut buf).unwrap();
        assert_eq!(n, 2);
        assert_eq!(buf, [0xDE, 0xAD]);
        let n = stream.read(&mut buf).unwrap();
        assert_eq!(n, 2);
        assert_eq!(buf, [0xBE, 0xEF]);
    }

    #[test]
    fn input_stream_read_at_end() {
        let data = vec![0x01, 0x02];
        let mut stream = MemoryStream::new_input(data);
        let mut buf = [0u8; 4];
        let n = stream.read(&mut buf).unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn stream_seek_and_tell() {
        let data = vec![0; 100];
        let mut stream = MemoryStream::new_input(data);
        stream.seek(50).unwrap();
        assert_eq!(stream.tell(), 50);
        stream.seek(0).unwrap();
        assert_eq!(stream.tell(), 0);
    }

    #[test]
    fn stream_skip() {
        let data = vec![0; 100];
        let mut stream = MemoryStream::new_input(data);
        stream.skip(10).unwrap();
        assert_eq!(stream.tell(), 10);
        stream.skip(20).unwrap();
        assert_eq!(stream.tell(), 30);
    }

    #[test]
    fn stream_bytes_left() {
        let data = vec![0; 100];
        let mut stream = MemoryStream::new_input(data);
        assert_eq!(stream.bytes_left(), 100);
        stream.skip(30).unwrap();
        assert_eq!(stream.bytes_left(), 70);
    }

    #[test]
    fn stream_seek_out_of_bounds() {
        let data = vec![0; 10];
        let mut stream = MemoryStream::new_input(data);
        let result = stream.seek(20);
        assert!(result.is_err());
    }
}
