use crate::error::Result;

/// Memory-backed byte stream (C: opj_stream_private_t).
#[allow(dead_code)]
pub struct MemoryStream {
    data: Vec<u8>,
    position: usize,
    is_input: bool,
}

impl MemoryStream {
    pub fn new_input(_data: Vec<u8>) -> Self {
        todo!()
    }

    pub fn new_output() -> Self {
        todo!()
    }

    pub fn read(&mut self, _buf: &mut [u8]) -> Result<usize> {
        todo!()
    }

    pub fn write(&mut self, _buf: &[u8]) -> Result<usize> {
        todo!()
    }

    pub fn skip(&mut self, _n: i64) -> Result<()> {
        todo!()
    }

    pub fn seek(&mut self, _pos: usize) -> Result<()> {
        todo!()
    }

    pub fn tell(&self) -> usize {
        todo!()
    }

    pub fn bytes_left(&self) -> usize {
        todo!()
    }

    pub fn data(&self) -> &[u8] {
        todo!()
    }
}

/// Write big-endian bytes (C: opj_write_bytes_BE).
pub fn write_bytes_be(_buf: &mut [u8], _val: u32, _n: usize) {
    todo!()
}

/// Read big-endian bytes (C: opj_read_bytes_BE).
pub fn read_bytes_be(_buf: &[u8], _n: usize) -> u32 {
    todo!()
}

/// Write f64 as big-endian (C: opj_write_double_BE).
pub fn write_f64_be(_buf: &mut [u8], _val: f64) {
    todo!()
}

/// Read f64 from big-endian (C: opj_read_double_BE).
pub fn read_f64_be(_buf: &[u8]) -> f64 {
    todo!()
}

/// Write f32 as big-endian (C: opj_write_float_BE).
pub fn write_f32_be(_buf: &mut [u8], _val: f32) {
    todo!()
}

/// Read f32 from big-endian (C: opj_read_float_BE).
pub fn read_f32_be(_buf: &[u8]) -> f32 {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Byte order conversion ---

    #[test]
    #[ignore = "not yet implemented"]
    fn write_read_bytes_be_1() {
        let mut buf = [0u8; 4];
        write_bytes_be(&mut buf, 0xAB, 1);
        assert_eq!(read_bytes_be(&buf, 1), 0xAB);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn write_read_bytes_be_2() {
        let mut buf = [0u8; 4];
        write_bytes_be(&mut buf, 0xABCD, 2);
        assert_eq!(buf[0], 0xAB);
        assert_eq!(buf[1], 0xCD);
        assert_eq!(read_bytes_be(&buf, 2), 0xABCD);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn write_read_bytes_be_4() {
        let mut buf = [0u8; 4];
        write_bytes_be(&mut buf, 0xDEADBEEF, 4);
        assert_eq!(read_bytes_be(&buf, 4), 0xDEADBEEF);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn write_read_f64_be() {
        let mut buf = [0u8; 8];
        let val = 3.14159265358980_f64;
        write_f64_be(&mut buf, val);
        let read_val = read_f64_be(&buf);
        assert_eq!(val.to_bits(), read_val.to_bits());
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn write_read_f32_be() {
        let mut buf = [0u8; 4];
        let val = 2.71829_f32;
        write_f32_be(&mut buf, val);
        let read_val = read_f32_be(&buf);
        assert_eq!(val.to_bits(), read_val.to_bits());
    }

    // --- MemoryStream ---

    #[test]
    #[ignore = "not yet implemented"]
    fn output_stream_write_and_read_back() {
        let mut stream = MemoryStream::new_output();
        let data = b"Hello, JPEG 2000!";
        stream.write(data).unwrap();
        assert_eq!(stream.tell(), data.len());
        assert_eq!(&stream.data()[..data.len()], data);
    }

    #[test]
    #[ignore = "not yet implemented"]
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
    #[ignore = "not yet implemented"]
    fn input_stream_read_at_end() {
        let data = vec![0x01, 0x02];
        let mut stream = MemoryStream::new_input(data);
        let mut buf = [0u8; 4];
        let n = stream.read(&mut buf).unwrap();
        assert_eq!(n, 2); // Only 2 bytes available
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn stream_seek_and_tell() {
        let data = vec![0; 100];
        let mut stream = MemoryStream::new_input(data);
        stream.seek(50).unwrap();
        assert_eq!(stream.tell(), 50);
        stream.seek(0).unwrap();
        assert_eq!(stream.tell(), 0);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn stream_skip() {
        let data = vec![0; 100];
        let mut stream = MemoryStream::new_input(data);
        stream.skip(10).unwrap();
        assert_eq!(stream.tell(), 10);
        stream.skip(20).unwrap();
        assert_eq!(stream.tell(), 30);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn stream_bytes_left() {
        let data = vec![0; 100];
        let mut stream = MemoryStream::new_input(data);
        assert_eq!(stream.bytes_left(), 100);
        stream.skip(30).unwrap();
        assert_eq!(stream.bytes_left(), 70);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn stream_seek_out_of_bounds() {
        let data = vec![0; 10];
        let mut stream = MemoryStream::new_input(data);
        let result = stream.seek(20);
        assert!(result.is_err());
    }
}
