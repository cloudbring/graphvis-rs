//! Voronoi overlap removal (`vAdjust`/`sAdjust` in `lib/neatogen/adjust.c`).

use super::adjust::AdjustMode;
use crate::common::types::{LGraph, Sub};

/// `vAdjust` (`mode == Vor`) or `sAdjust` (`mode == Scale`).
pub fn adjust(_g: &mut LGraph<'_>, _sub: &Sub, _mode: AdjustMode) -> bool {
    false
}
