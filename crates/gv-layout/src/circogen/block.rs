//! `lib/circogen/block.c`: `block_t` and block lists.

use super::graph::CGraph;

/// `COALESCED_F`.
pub const COALESCED_F: i32 = 1 << 0;

/// `block_t`.
#[derive(Debug, Default, Clone)]
pub struct Block {
    /// `child`: the node of this block attached to the parent block.
    pub child: Option<usize>,
    /// `sub_graph`: nodes and edges of the block.
    pub sub_graph: CGraph,
    /// Radius of the block and its sub-blocks.
    pub radius: f64,
    /// Radius of the block alone.
    pub rad0: f64,
    /// Ordered nodes of the block circle.
    pub circle_list: Vec<usize>,
    /// Child blocks (indices into the block arena).
    pub children: Vec<usize>,
    /// If the block has one node, the angle at which to place the parent.
    pub parent_pos: f64,
    pub flags: i32,
}

/// `mkBlock(g)`.
pub fn mk_block(g: CGraph) -> Block {
    Block {
        sub_graph: g,
        ..Block::default()
    }
}

/// `blockSize(sp)`.
pub fn block_size(sp: &Block) -> usize {
    sp.sub_graph.nnodes()
}

/// `appendBlock(bl, bp)`: add at the end.
pub fn append_block(bl: &mut Vec<usize>, bp: usize) {
    bl.push(bp);
}

/// `insertBlock(bl, bp)`: add at the beginning.
pub fn insert_block(bl: &mut Vec<usize>, bp: usize) {
    bl.insert(0, bp);
}
