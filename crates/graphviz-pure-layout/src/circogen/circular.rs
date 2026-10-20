//! `lib/circogen/circular.c` and the data of `circular.h`.

use super::block::{Block, mk_block};
use super::blocktree::create_blocktree;
use super::circpos::circ_pos;
use super::graph::{CGraph, EdgeArena};
use crate::common::types::LGraph;
use crate::common::utils::{late_double, mapbool};

/// `MINDIST`: default `mindist`.
const MINDIST: f64 = 1.0;

pub const VISITED_F: i32 = 1 << 0;
pub const ONSTACK_F: i32 = 1 << 2;
pub const PARENT_F: i32 = 1 << 3;
pub const PATH_F: i32 = 1 << 4;
pub const NEIGHBOR_F: i32 = 1 << 5;

/// `cdata`: per derived node layout data. The C union members are kept as
/// separate fields; each pass writes its fields before reading them, so the
/// overlap is never observable.
#[derive(Debug, Clone, Default)]
pub struct CData {
    pub flags: i32,
    /// `PARENT(n)`: parent in the block-cutpoint traversal.
    pub parent: Option<usize>,
    /// `BLOCK(n)`: block containing the node (index into the block arena).
    pub block: Option<usize>,
    /// `VAL(n)`, `LOWVAL(n)` (pass 1).
    pub val: i32,
    pub low_val: i32,
    /// `TPARENT`, `LEAFONE`, `LEAFTWO`, `DISTONE`, `DISTTWO` (pass 3b).
    pub tparent: Option<usize>,
    pub leafone: Option<usize>,
    pub leaftwo: Option<usize>,
    pub distone: i32,
    pub disttwo: i32,
    /// `POSITION(n)`, `PSI(n)` (passes 3c, 4).
    pub position: i32,
    pub psi: f64,
}

/// The circo working state shared by the passes: derived node data
/// (`ND_alg`), derived positions (`ND_pos`), the edge arena and the block
/// arena, plus `circ_state`.
pub struct Circ {
    pub nd: Vec<CData>,
    /// `ND_pos` of the derived nodes, inches.
    pub pos: Vec<[f64; 2]>,
    /// `ND_width`, `ND_height` of the original nodes (`ORIGN`).
    pub width: Vec<f64>,
    pub height: Vec<f64>,
    pub arena: EdgeArena,
    pub blocks: Vec<Block>,
    /// `circ_state.bl`.
    pub bl: Vec<usize>,
    pub order_count: i32,
    pub min_dist: f64,
    /// `circ_state.rootname`.
    pub rootname: Option<String>,
    /// Whether the original graph declares the node attribute `root`.
    pub n_root: bool,
    /// Original node `root` values (`late_bool(ORIGN(n), N_root, false)`).
    pub node_root: Vec<bool>,
    /// Original node names, for `agfindnode`.
    pub names: Vec<String>,
}

impl Circ {
    pub fn flag(&self, n: usize, f: i32) -> bool {
        self.nd[n].flags & f != 0
    }

    pub fn set_flag(&mut self, n: usize, f: i32) {
        self.nd[n].flags |= f;
    }

    pub fn unset_flag(&mut self, n: usize, f: i32) {
        self.nd[n].flags &= !f;
    }

    /// `BLK_PARENT(b)`.
    pub fn blk_parent(&self, b: usize) -> Option<usize> {
        self.blocks[b].child.and_then(|c| self.nd[c].parent)
    }

    /// `makeBlock` / `mkBlock`: a new empty block.
    pub fn make_block(&mut self) -> usize {
        self.blocks.push(mk_block(CGraph::default()));
        self.blocks.len() - 1
    }
}

/// `initGraphAttrs(g, state)`: attributes from the original root graph.
fn init_graph_attrs(c: &mut Circ, rg: &LGraph<'_>) {
    c.bl.clear();
    c.order_count = 1;
    c.min_dist = late_double(&rg.src.graph_attrs, "mindist", MINDIST, 0.0);
    c.n_root = rg.node_attr_declared("root");
    c.rootname = rg.gattr("root").map(str::to_string);
}

/// `createOneBlock(g, state)`: every node of `g` in a single block.
fn create_one_block(c: &mut Circ, g: &CGraph) -> usize {
    let bp = c.make_block();
    for n in g.nodes() {
        c.blocks[bp].sub_graph.add_node(n);
        c.nd[n].block = Some(bp);
    }
    bp
}

/// `circularLayout(g, realg, blockCount)`: lay out one connected component
/// `g` of the derived graph.
pub fn circular_layout(c: &mut Circ, g: &CGraph, realg: &LGraph<'_>) {
    if g.nnodes() == 1 {
        let n = g.first_node().expect("one node");
        c.pos[n] = [0.0, 0.0];
        return;
    }
    init_graph_attrs(c, realg);
    let root = if mapbool(realg.gattr("oneblock")) {
        create_one_block(c, g)
    } else {
        create_blocktree(c, g)
    };
    circ_pos(c, g, root);
}
