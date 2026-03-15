// Phase 200: Tag tree (C: opj_tgt_tree_t)

use crate::error::Result;
use crate::io::bio::Bio;

/// Tag tree node.
struct TgtNode {
    parent: Option<usize>,
    value: i32,
    low: i32,
    known: bool,
}

/// Tag tree (C: opj_tgt_tree_t).
///
/// Hierarchical structure for progressive encoding of inclusion/zero-bit-plane
/// information in Tier-2 coding. Leaf values propagate upward as minimums.
pub struct TagTree {
    #[allow(dead_code)]
    numleafsh: u32,
    #[allow(dead_code)]
    numleafsv: u32,
    nodes: Vec<TgtNode>,
}

impl TagTree {
    /// Create a new tag tree (C: opj_tgt_create).
    pub fn new(numleafsh: u32, numleafsv: u32) -> Self {
        debug_assert!(
            numleafsh > 0 && numleafsv > 0,
            "tag tree dimensions must be non-zero"
        );
        // Calculate total nodes across all levels
        let mut nplh = vec![numleafsh as i32];
        let mut nplv = vec![numleafsv as i32];
        let mut total_nodes = 0u32;
        loop {
            let n = (nplh.last().unwrap() * nplv.last().unwrap()) as u32;
            total_nodes += n;
            if n <= 1 {
                break;
            }
            nplh.push((nplh.last().unwrap() + 1) / 2);
            nplv.push((nplv.last().unwrap() + 1) / 2);
        }

        let mut nodes: Vec<TgtNode> = (0..total_nodes)
            .map(|_| TgtNode {
                parent: None,
                value: 999,
                low: 0,
                known: false,
            })
            .collect();

        // Set up parent pointers
        let num_levels = nplh.len();
        let mut node_idx = 0usize;
        let mut parent_base = (numleafsh * numleafsv) as usize;
        let mut parent_idx;
        let mut parent_row_start;

        for level in 0..num_levels - 1 {
            let w = nplh[level];
            let h = nplv[level];
            let parent_w = nplh[level + 1];
            parent_idx = parent_base;
            parent_row_start = parent_base;

            for j in 0..h {
                let mut k = w;
                while k > 0 {
                    k -= 1;
                    nodes[node_idx].parent = Some(parent_idx);
                    node_idx += 1;
                    if k > 0 {
                        k -= 1;
                        nodes[node_idx].parent = Some(parent_idx);
                        node_idx += 1;
                    }
                    parent_idx += 1;
                }
                if (j & 1) != 0 || j == h - 1 {
                    parent_row_start = parent_idx;
                } else {
                    parent_idx = parent_row_start;
                    parent_row_start += parent_w as usize;
                }
            }

            parent_base += (parent_w * nplv[level + 1]) as usize;
        }
        // Root has no parent (already None)

        Self {
            numleafsh,
            numleafsv,
            nodes,
        }
    }

    /// Total number of nodes in the tree.
    pub fn num_nodes(&self) -> usize {
        self.nodes.len()
    }

    /// Reset all node values to 999 (C: opj_tgt_reset).
    pub fn reset(&mut self) {
        for node in &mut self.nodes {
            node.value = 999;
            node.low = 0;
            node.known = false;
        }
    }

    /// Set leaf value and propagate minimum upward (C: opj_tgt_setvalue).
    pub fn set_value(&mut self, leafno: u32, value: i32) {
        debug_assert!(
            (leafno as usize) < (self.numleafsh as usize) * (self.numleafsv as usize),
            "leafno {leafno} out of range (numleafsh={}, numleafsv={})",
            self.numleafsh,
            self.numleafsv
        );
        let mut idx = leafno as usize;
        while self.nodes[idx].value > value {
            self.nodes[idx].value = value;
            match self.nodes[idx].parent {
                Some(p) => idx = p,
                None => break,
            }
        }
    }

    /// Encode a leaf value up to threshold (C: opj_tgt_encode).
    pub fn encode(&mut self, bio: &mut Bio, leafno: u32, threshold: i32) -> Result<()> {
        // Walk from leaf to root, collect path
        let mut stack = Vec::new();
        let mut idx = leafno as usize;
        while let Some(parent) = self.nodes[idx].parent {
            stack.push(idx);
            idx = parent;
        }
        // idx is now root

        let mut low = 0i32;
        loop {
            if low > self.nodes[idx].low {
                self.nodes[idx].low = low;
            } else {
                low = self.nodes[idx].low;
            }

            while low < threshold {
                if low >= self.nodes[idx].value {
                    if !self.nodes[idx].known {
                        bio.write(1, 1)?;
                        self.nodes[idx].known = true;
                    }
                    break;
                }
                bio.write(0, 1)?;
                low += 1;
            }

            self.nodes[idx].low = low;
            match stack.pop() {
                Some(next) => idx = next,
                None => break,
            }
        }
        Ok(())
    }

    /// Decode a leaf value up to threshold (C: opj_tgt_decode).
    /// Returns Ok(1) if value < threshold, Ok(0) otherwise.
    pub fn decode(&mut self, bio: &mut Bio, leafno: u32, threshold: i32) -> Result<u32> {
        // Walk from leaf to root, collect path
        let mut stack = Vec::new();
        let mut idx = leafno as usize;
        while let Some(parent) = self.nodes[idx].parent {
            stack.push(idx);
            idx = parent;
        }

        let mut low = 0i32;
        loop {
            if low > self.nodes[idx].low {
                self.nodes[idx].low = low;
            } else {
                low = self.nodes[idx].low;
            }

            while low < threshold && low < self.nodes[idx].value {
                if bio.read(1)? != 0 {
                    self.nodes[idx].value = low;
                } else {
                    low += 1;
                }
            }

            self.nodes[idx].low = low;
            match stack.pop() {
                Some(next) => idx = next,
                None => break,
            }
        }

        Ok(if self.nodes[idx].value < threshold {
            1
        } else {
            0
        })
    }
}

#[cfg(test)]
impl TagTree {
    fn node_value(&self, idx: usize) -> i32 {
        self.nodes[idx].value
    }
    fn node_low(&self, idx: usize) -> i32 {
        self.nodes[idx].low
    }
    fn node_known(&self, idx: usize) -> bool {
        self.nodes[idx].known
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_1x1() {
        let tree = TagTree::new(1, 1);
        assert_eq!(tree.num_nodes(), 1);
    }

    #[test]
    fn new_2x2() {
        let tree = TagTree::new(2, 2);
        assert_eq!(tree.num_nodes(), 5);
    }

    #[test]
    fn new_4x4() {
        let tree = TagTree::new(4, 4);
        assert_eq!(tree.num_nodes(), 21);
    }

    #[test]
    fn new_3x5() {
        let tree = TagTree::new(3, 5);
        assert_eq!(tree.num_nodes(), 24);
    }

    #[test]
    fn reset_sets_values_to_999() {
        let mut tree = TagTree::new(2, 2);
        tree.set_value(0, 5);
        tree.reset();
        assert_eq!(tree.num_nodes(), 5);
        for i in 0..tree.num_nodes() {
            assert_eq!(tree.node_value(i), 999, "node {i} value not reset");
            assert_eq!(tree.node_low(i), 0, "node {i} low not reset");
            assert!(!tree.node_known(i), "node {i} known not reset");
        }
    }

    #[test]
    fn set_value_propagates_up() {
        let mut tree = TagTree::new(2, 2);
        tree.set_value(0, 3);
        tree.set_value(1, 5);
        tree.set_value(2, 7);
        tree.set_value(3, 2);
        // Leaf values must be stored
        assert_eq!(tree.node_value(0), 3);
        assert_eq!(tree.node_value(1), 5);
        assert_eq!(tree.node_value(2), 7);
        assert_eq!(tree.node_value(3), 2);
        // Root (node 4 in a 2x2 tree) must hold the minimum leaf value (2)
        assert_eq!(tree.node_value(4), 2);
    }

    #[test]
    fn encode_decode_roundtrip_single_leaf() {
        let mut tree = TagTree::new(1, 1);
        tree.set_value(0, 3);

        let mut buf = [0u8; 16];
        {
            let mut bio = Bio::encoder(&mut buf);
            tree.encode(&mut bio, 0, 4).unwrap();
            bio.flush().unwrap();
        }

        let mut dec_tree = TagTree::new(1, 1);
        {
            let mut bio = Bio::decoder(&mut buf);
            let below = dec_tree.decode(&mut bio, 0, 4).unwrap();
            assert_eq!(below, 1);
        }
    }

    #[test]
    fn encode_decode_roundtrip_2x2() {
        let mut tree = TagTree::new(2, 2);
        tree.set_value(0, 1);
        tree.set_value(1, 3);
        tree.set_value(2, 0);
        tree.set_value(3, 5);

        let mut buf = [0u8; 64];
        {
            let mut bio = Bio::encoder(&mut buf);
            for leaf in 0..4 {
                tree.encode(&mut bio, leaf, 6).unwrap();
            }
            bio.flush().unwrap();
        }

        let mut dec_tree = TagTree::new(2, 2);
        {
            let mut bio = Bio::decoder(&mut buf);
            for leaf in 0..4 {
                dec_tree.decode(&mut bio, leaf, 6).unwrap();
            }
        }
    }

    #[test]
    fn encode_decode_threshold_below_value() {
        let mut tree = TagTree::new(1, 1);
        tree.set_value(0, 5);

        let mut buf = [0u8; 16];
        {
            let mut bio = Bio::encoder(&mut buf);
            tree.encode(&mut bio, 0, 3).unwrap();
            bio.flush().unwrap();
        }

        let mut dec_tree = TagTree::new(1, 1);
        {
            let mut bio = Bio::decoder(&mut buf);
            let below = dec_tree.decode(&mut bio, 0, 3).unwrap();
            assert_eq!(below, 0);
        }
    }

    #[test]
    fn progressive_encode_decode() {
        let mut enc_tree = TagTree::new(2, 2);
        enc_tree.set_value(0, 1);
        enc_tree.set_value(1, 3);
        enc_tree.set_value(2, 0);
        enc_tree.set_value(3, 5);

        let mut buf = [0u8; 64];
        {
            let mut bio = Bio::encoder(&mut buf);
            for leaf in 0..4 {
                enc_tree.encode(&mut bio, leaf, 1).unwrap();
            }
            for leaf in 0..4 {
                enc_tree.encode(&mut bio, leaf, 4).unwrap();
            }
            bio.flush().unwrap();
        }

        let mut dec_tree = TagTree::new(2, 2);
        {
            let mut bio = Bio::decoder(&mut buf);
            for leaf in 0..4 {
                dec_tree.decode(&mut bio, leaf, 1).unwrap();
            }
            for leaf in 0..4 {
                dec_tree.decode(&mut bio, leaf, 4).unwrap();
            }
        }
    }
}
