// Phase 200: Sparse array (C: opj_sparse_array_int32_t)

use crate::error::Result;

/// Block-based sparse i32 array (C: opj_sparse_array_int32_t).
#[allow(dead_code)]
pub struct SparseArray {
    width: u32,
    height: u32,
    block_width: u32,
    block_height: u32,
    block_count_hor: u32,
    block_count_ver: u32,
    data_blocks: Vec<Option<Vec<i32>>>,
}

#[allow(dead_code)]
impl SparseArray {
    pub fn new(_width: u32, _height: u32, _block_width: u32, _block_height: u32) -> Self {
        todo!()
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn block_count_hor(&self) -> u32 {
        self.block_count_hor
    }
    pub fn block_count_ver(&self) -> u32 {
        self.block_count_ver
    }
    pub fn is_region_valid(&self, _x0: u32, _y0: u32, _x1: u32, _y1: u32) -> bool {
        todo!()
    }
    #[allow(clippy::too_many_arguments)]
    pub fn read_region(
        &self,
        _x0: u32,
        _y0: u32,
        _x1: u32,
        _y1: u32,
        _buf: &mut [i32],
        _col_stride: u32,
        _line_stride: u32,
        _forgiving: bool,
    ) -> Result<()> {
        todo!()
    }
    #[allow(clippy::too_many_arguments)]
    pub fn write_region(
        &mut self,
        _x0: u32,
        _y0: u32,
        _x1: u32,
        _y1: u32,
        _buf: &[i32],
        _col_stride: u32,
        _line_stride: u32,
        _forgiving: bool,
    ) -> Result<()> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "not yet implemented"]
    fn new_creates_valid_array() {
        let sa = SparseArray::new(100, 200, 32, 32);
        assert_eq!(sa.width(), 100);
        assert_eq!(sa.height(), 200);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn new_calculates_block_counts() {
        // 100/32 = ceil(3.125) = 4, 200/32 = ceil(6.25) = 7
        let sa = SparseArray::new(100, 200, 32, 32);
        assert_eq!(sa.block_count_hor(), 4);
        assert_eq!(sa.block_count_ver(), 7);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn new_exact_block_size() {
        let sa = SparseArray::new(64, 128, 32, 32);
        assert_eq!(sa.block_count_hor(), 2);
        assert_eq!(sa.block_count_ver(), 4);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn is_region_valid_accepts_valid() {
        let sa = SparseArray::new(100, 200, 32, 32);
        assert!(sa.is_region_valid(0, 0, 100, 200));
        assert!(sa.is_region_valid(10, 20, 50, 80));
        assert!(sa.is_region_valid(0, 0, 1, 1));
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn is_region_valid_rejects_invalid() {
        let sa = SparseArray::new(100, 200, 32, 32);
        // empty region
        assert!(!sa.is_region_valid(10, 10, 10, 10));
        // x1 < x0
        assert!(!sa.is_region_valid(50, 0, 10, 100));
        // out of bounds
        assert!(!sa.is_region_valid(0, 0, 101, 200));
        assert!(!sa.is_region_valid(0, 0, 100, 201));
        // x0 >= width
        assert!(!sa.is_region_valid(100, 0, 101, 1));
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn read_unwritten_region_returns_zeros() {
        let sa = SparseArray::new(64, 64, 32, 32);
        let mut buf = vec![42i32; 16];
        sa.read_region(0, 0, 4, 4, &mut buf, 1, 4, false).unwrap();
        assert!(buf.iter().all(|&v| v == 0));
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn write_then_read_roundtrip() {
        let mut sa = SparseArray::new(64, 64, 32, 32);
        let src: Vec<i32> = (0..16).collect();
        sa.write_region(0, 0, 4, 4, &src, 1, 4, false).unwrap();

        let mut dst = vec![0i32; 16];
        sa.read_region(0, 0, 4, 4, &mut dst, 1, 4, false).unwrap();
        assert_eq!(src, dst);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn write_then_read_partial_block() {
        let mut sa = SparseArray::new(64, 64, 32, 32);
        // Write to region that doesn't start at block boundary
        let src: Vec<i32> = (100..109).collect();
        sa.write_region(10, 10, 13, 13, &src, 1, 3, false).unwrap();

        let mut dst = vec![0i32; 9];
        sa.read_region(10, 10, 13, 13, &mut dst, 1, 3, false)
            .unwrap();
        assert_eq!(src, dst);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn write_then_read_across_blocks() {
        let mut sa = SparseArray::new(64, 64, 8, 8);
        // 12x12 region crossing multiple 8x8 blocks
        let src: Vec<i32> = (0..144).collect();
        sa.write_region(2, 2, 14, 14, &src, 1, 12, false).unwrap();

        let mut dst = vec![0i32; 144];
        sa.read_region(2, 2, 14, 14, &mut dst, 1, 12, false)
            .unwrap();
        assert_eq!(src, dst);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn read_with_col_stride() {
        let mut sa = SparseArray::new(32, 32, 16, 16);
        let src: Vec<i32> = (0..4).collect();
        sa.write_region(0, 0, 2, 2, &src, 1, 2, false).unwrap();

        // Read with col_stride=2 (interleaved output)
        let mut dst = vec![0i32; 8];
        sa.read_region(0, 0, 2, 2, &mut dst, 2, 4, false).unwrap();
        // row 0: dst[0]=0, dst[2]=1
        // row 1: dst[4]=2, dst[6]=3
        assert_eq!(dst[0], 0);
        assert_eq!(dst[2], 1);
        assert_eq!(dst[4], 2);
        assert_eq!(dst[6], 3);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn write_with_col_stride() {
        let mut sa = SparseArray::new(32, 32, 16, 16);
        // Source with col_stride=2
        let src = vec![10, 0, 20, 0, 30, 0, 40, 0];
        sa.write_region(0, 0, 2, 2, &src, 2, 4, false).unwrap();

        let mut dst = vec![0i32; 4];
        sa.read_region(0, 0, 2, 2, &mut dst, 1, 2, false).unwrap();
        assert_eq!(dst, vec![10, 20, 30, 40]);
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn invalid_region_non_forgiving_returns_error() {
        let sa = SparseArray::new(64, 64, 32, 32);
        let mut buf = vec![0i32; 4];
        let result = sa.read_region(0, 0, 65, 1, &mut buf, 1, 65, false);
        assert!(result.is_err());
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn invalid_region_forgiving_returns_ok() {
        let sa = SparseArray::new(64, 64, 32, 32);
        let mut buf = vec![0i32; 4];
        let result = sa.read_region(0, 0, 65, 1, &mut buf, 1, 65, true);
        assert!(result.is_ok());
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn single_element_write_read() {
        let mut sa = SparseArray::new(10, 10, 4, 4);
        let src = [42i32];
        sa.write_region(5, 5, 6, 6, &src, 1, 1, false).unwrap();

        let mut dst = [0i32];
        sa.read_region(5, 5, 6, 6, &mut dst, 1, 1, false).unwrap();
        assert_eq!(dst[0], 42);
    }
}
