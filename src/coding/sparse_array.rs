// Phase 200: Sparse array (C: opj_sparse_array_int32_t)

use crate::error::{Error, Result};
use crate::types::uint_ceildiv;

/// Block-based sparse i32 array (C: opj_sparse_array_int32_t).
///
/// Divides a logical 2D array into fixed-size blocks. Blocks are allocated
/// on first write; unwritten blocks read as zero.
pub struct SparseArray {
    width: u32,
    height: u32,
    block_width: u32,
    block_height: u32,
    block_count_hor: u32,
    block_count_ver: u32,
    data_blocks: Vec<Option<Vec<i32>>>,
}

impl SparseArray {
    /// Create a new sparse array (C: opj_sparse_array_int32_create).
    pub fn new(width: u32, height: u32, block_width: u32, block_height: u32) -> Self {
        assert!(
            width > 0 && height > 0 && block_width > 0 && block_height > 0,
            "SparseArray dimensions and block sizes must be non-zero"
        );
        let bch = uint_ceildiv(width, block_width);
        let bcv = uint_ceildiv(height, block_height);
        let total = (bch as usize) * (bcv as usize);
        Self {
            width,
            height,
            block_width,
            block_height,
            block_count_hor: bch,
            block_count_ver: bcv,
            data_blocks: vec![None; total],
        }
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

    /// Check if a region is valid (C: opj_sparse_array_is_region_valid).
    pub fn is_region_valid(&self, x0: u32, y0: u32, x1: u32, y1: u32) -> bool {
        !(x0 >= self.width
            || x1 <= x0
            || x1 > self.width
            || y0 >= self.height
            || y1 <= y0
            || y1 > self.height)
    }

    /// Read a rectangular region (C: opj_sparse_array_int32_read).
    #[allow(clippy::too_many_arguments)]
    pub fn read_region(
        &self,
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
        buf: &mut [i32],
        col_stride: u32,
        line_stride: u32,
        forgiving: bool,
    ) -> Result<()> {
        self.read_or_write(
            x0,
            y0,
            x1,
            y1,
            buf,
            &[],
            col_stride,
            line_stride,
            forgiving,
            true,
        )
    }

    /// Write a rectangular region (C: opj_sparse_array_int32_write).
    #[allow(clippy::too_many_arguments)]
    pub fn write_region(
        &mut self,
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
        buf: &[i32],
        col_stride: u32,
        line_stride: u32,
        forgiving: bool,
    ) -> Result<()> {
        // We need &mut self for write, but read_or_write needs to handle both.
        // Use a separate path to avoid borrow conflicts.
        if !self.is_region_valid(x0, y0, x1, y1) {
            return if forgiving {
                Ok(())
            } else {
                Err(Error::InvalidInput("region out of bounds".into()))
            };
        }

        debug_assert!(
            buf.len()
                > (y1 - y0 - 1) as usize * line_stride as usize
                    + (x1 - x0 - 1) as usize * col_stride as usize,
            "source buffer too small for write_region"
        );

        let bw = self.block_width;
        let bh = self.block_height;

        let mut block_y = y0 / bh;
        let mut y = y0;
        while y < y1 {
            let y_incr_full = if y == y0 { bh - (y0 % bh) } else { bh };
            let block_y_offset = bh - y_incr_full;
            let y_incr = y_incr_full.min(y1 - y);

            let mut block_x = x0 / bw;
            let mut x = x0;
            while x < x1 {
                let x_incr_full = if x == x0 { bw - (x0 % bw) } else { bw };
                let block_x_offset = bw - x_incr_full;
                let x_incr = x_incr_full.min(x1 - x);

                let block_idx = (block_y * self.block_count_hor + block_x) as usize;
                if self.data_blocks[block_idx].is_none() {
                    self.data_blocks[block_idx] = Some(vec![0i32; (bw as usize) * (bh as usize)]);
                }
                let block = self.data_blocks[block_idx].as_mut().unwrap();

                for j in 0..y_incr {
                    let dest_off =
                        (block_y_offset + j) as usize * bw as usize + block_x_offset as usize;
                    let src_off = (y - y0 + j) as usize * line_stride as usize
                        + (x - x0) as usize * col_stride as usize;
                    for k in 0..x_incr as usize {
                        block[dest_off + k] = buf[src_off + k * col_stride as usize];
                    }
                }

                x += x_incr;
                block_x += 1;
            }
            y += y_incr;
            block_y += 1;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn read_or_write(
        &self,
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
        buf: &mut [i32],
        _src: &[i32],
        col_stride: u32,
        line_stride: u32,
        forgiving: bool,
        is_read: bool,
    ) -> Result<()> {
        if !self.is_region_valid(x0, y0, x1, y1) {
            return if forgiving {
                Ok(())
            } else {
                Err(Error::InvalidInput("region out of bounds".into()))
            };
        }

        debug_assert!(is_read, "write should use write_region directly");
        debug_assert!(
            buf.len()
                > (y1 - y0 - 1) as usize * line_stride as usize
                    + (x1 - x0 - 1) as usize * col_stride as usize,
            "destination buffer too small for read_region"
        );

        let bw = self.block_width;
        let bh = self.block_height;

        let mut block_y = y0 / bh;
        let mut y = y0;
        while y < y1 {
            let y_incr_full = if y == y0 { bh - (y0 % bh) } else { bh };
            let block_y_offset = bh - y_incr_full;
            let y_incr = y_incr_full.min(y1 - y);

            let mut block_x = x0 / bw;
            let mut x = x0;
            while x < x1 {
                let x_incr_full = if x == x0 { bw - (x0 % bw) } else { bw };
                let block_x_offset = bw - x_incr_full;
                let x_incr = x_incr_full.min(x1 - x);

                let block_idx = (block_y * self.block_count_hor + block_x) as usize;
                let src_block = &self.data_blocks[block_idx];

                for j in 0..y_incr {
                    let buf_off = (y - y0 + j) as usize * line_stride as usize
                        + (x - x0) as usize * col_stride as usize;
                    match src_block {
                        None => {
                            for k in 0..x_incr as usize {
                                buf[buf_off + k * col_stride as usize] = 0;
                            }
                        }
                        Some(block) => {
                            let block_off = (block_y_offset + j) as usize * bw as usize
                                + block_x_offset as usize;
                            for k in 0..x_incr as usize {
                                buf[buf_off + k * col_stride as usize] = block[block_off + k];
                            }
                        }
                    }
                }

                x += x_incr;
                block_x += 1;
            }
            y += y_incr;
            block_y += 1;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_valid_array() {
        let sa = SparseArray::new(100, 200, 32, 32);
        assert_eq!(sa.width(), 100);
        assert_eq!(sa.height(), 200);
    }

    #[test]
    fn new_calculates_block_counts() {
        // 100/32 = ceil(3.125) = 4, 200/32 = ceil(6.25) = 7
        let sa = SparseArray::new(100, 200, 32, 32);
        assert_eq!(sa.block_count_hor(), 4);
        assert_eq!(sa.block_count_ver(), 7);
    }

    #[test]
    fn new_exact_block_size() {
        let sa = SparseArray::new(64, 128, 32, 32);
        assert_eq!(sa.block_count_hor(), 2);
        assert_eq!(sa.block_count_ver(), 4);
    }

    #[test]
    fn is_region_valid_accepts_valid() {
        let sa = SparseArray::new(100, 200, 32, 32);
        assert!(sa.is_region_valid(0, 0, 100, 200));
        assert!(sa.is_region_valid(10, 20, 50, 80));
        assert!(sa.is_region_valid(0, 0, 1, 1));
    }

    #[test]
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
    fn read_unwritten_region_returns_zeros() {
        let sa = SparseArray::new(64, 64, 32, 32);
        let mut buf = vec![42i32; 16];
        sa.read_region(0, 0, 4, 4, &mut buf, 1, 4, false).unwrap();
        assert!(buf.iter().all(|&v| v == 0));
    }

    #[test]
    fn write_then_read_roundtrip() {
        let mut sa = SparseArray::new(64, 64, 32, 32);
        let src: Vec<i32> = (0..16).collect();
        sa.write_region(0, 0, 4, 4, &src, 1, 4, false).unwrap();

        let mut dst = vec![0i32; 16];
        sa.read_region(0, 0, 4, 4, &mut dst, 1, 4, false).unwrap();
        assert_eq!(src, dst);
    }

    #[test]
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
    fn invalid_region_non_forgiving_returns_error() {
        let sa = SparseArray::new(64, 64, 32, 32);
        let mut buf = vec![0i32; 4];
        let result = sa.read_region(0, 0, 65, 1, &mut buf, 1, 65, false);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_region_forgiving_returns_ok() {
        let sa = SparseArray::new(64, 64, 32, 32);
        let mut buf = vec![0i32; 4];
        let result = sa.read_region(0, 0, 65, 1, &mut buf, 1, 65, true);
        assert!(result.is_ok());
    }

    #[test]
    fn single_element_write_read() {
        let mut sa = SparseArray::new(10, 10, 4, 4);
        let src = [42i32];
        sa.write_region(5, 5, 6, 6, &src, 1, 1, false).unwrap();

        let mut dst = [0i32];
        sa.read_region(5, 5, 6, 6, &mut dst, 1, 1, false).unwrap();
        assert_eq!(dst[0], 42);
    }
}
