//! Parts of Graphviz `lib/sfdpgen` shared with neato's overlap removal:
//! the stress-majorization smoother (`post_process.c`) and its sparse
//! conjugate-gradient solver (`sparse_solve.c`).

#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

pub mod post_process;
