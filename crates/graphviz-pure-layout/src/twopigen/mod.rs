//! Port of Graphviz `lib/twopigen` (the `twopi` radial layout engine).
//!
//! Module ↔ C file map:
//! - [`circle`] — `circle.c` (`circleLayout`: centre selection, BFS tree,
//!   subtree spans, angles, `ranksep` radii)
//! - [`twopiinit`] — `twopiinit.c` (`twopi_layout`: `root` handling,
//!   components, overlap removal, packing, edge routing)

pub mod circle;
pub mod twopiinit;
