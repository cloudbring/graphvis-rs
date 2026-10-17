//! Prism overlap removal (`fdpAdjust` in `lib/neatogen/adjust.c`,
//! `lib/neatogen/overlap.c`).

use super::adjust::AdjustData;
use crate::common::types::{LGraph, Sub};

/// `fdpAdjust(g, am)`.
pub fn fdp_adjust(_g: &mut LGraph<'_>, _sub: &Sub, _am: &AdjustData) {}
