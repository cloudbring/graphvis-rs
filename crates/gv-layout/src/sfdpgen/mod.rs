//! Port of Graphviz `lib/sfdpgen` (the `sfdp` layout engine).
//!
//! Module ↔ C file map:
//! - [`sfdpinit`] — `sfdpinit.c` (`sfdp_layout`, `tuneControl`)
//! - [`spring_electrical`] — `spring_electrical.c`
//! - [`multilevel`] — `Multilevel.c`
//! - [`post_process`] — `post_process.c` and `sparse_solve.c`
//! - [`stress_model`] — `stress_model.c`

#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

pub mod multilevel;
pub mod post_process;
pub mod sfdpinit;
pub mod spring_electrical;
pub mod stress_model;
