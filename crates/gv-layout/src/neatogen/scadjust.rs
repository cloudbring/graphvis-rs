//! Scaling and constraint-based overlap removal from `lib/neatogen/constraint.c`
//! (`scAdjust`, `cAdjust`) and `adjust.c` (`vpscAdjust`).

use super::adjust::AdjustMode;
use crate::common::types::{LGraph, Sub};

/// `scAdjust(g, equal)`.
pub fn sc_adjust(_g: &mut LGraph<'_>, _sub: &Sub, _equal: i32) -> bool {
    false
}

/// `cAdjust(g, mode)`.
pub fn c_adjust(_g: &mut LGraph<'_>, _sub: &Sub, _mode: AdjustMode) {}

/// `vpscAdjust(G)`.
pub fn vpsc_adjust(_g: &mut LGraph<'_>, _sub: &Sub) -> bool {
    false
}
