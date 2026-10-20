//! Port of Graphviz `lib/patchwork` (the `patchwork` squarified treemap engine).
//!
//! Module ↔ C file map:
//! - [`patchworkinit`] — `patchworkinit.c` (`patchwork_layout`, initialisation)
//! - [`patchwork`] — `patchwork.c` (`patchworkLayout` and the tree walk)
//! - [`tree_map`] — `tree_map.c` (`tree_map`, `squarify`)

#![allow(clippy::module_inception)]

pub mod patchwork;
pub mod patchworkinit;
pub mod tree_map;

pub use patchworkinit::patchwork_layout;
