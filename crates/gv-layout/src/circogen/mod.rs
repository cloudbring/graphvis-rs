//! Port of Graphviz `lib/circogen` (the `circo` circular layout engine).
//!
//! Module ↔ C file map:
//! - [`circularinit`] — `circularinit.c` (`circo_layout`, `circoLayout`,
//!   `circomps`, derived graph)
//! - [`circular`] — `circular.c` / `circular.h` (`circularLayout`,
//!   `initGraphAttrs`, `createOneBlock`, `cdata`, `circ_state`)
//! - [`blocktree`] — `blocktree.c` (biconnected components, block tree)
//! - [`blockpath`] — `blockpath.c` (`layout_block`: skeleton, spanning tree,
//!   longest path, crossing reduction); `deglist` is the sorted node list there
//! - [`circpos`] — `circpos.c` (`circPos`: placing child blocks)
//! - [`block`] — `block.c` (`block_t`, block lists)
//! - [`nodelist`], [`edgelist`] — `nodelist.c`, `edgelist.c`
//! - [`graph`] — the subset of cgraph (strict undirected graphs, subgraphs,
//!   edge/node deletion, sequence-ordered iteration) circo relies on

// Faithful ports of numeric C loops index several arrays in lockstep.
#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

pub mod block;
pub mod blockpath;
pub mod blocktree;
pub mod circpos;
pub mod circular;
pub mod circularinit;
pub mod edgelist;
pub mod graph;
pub mod nodelist;
