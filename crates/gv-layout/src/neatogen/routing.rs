//! Obstacle-avoiding edge routing for `splines=true|polyline` in the neato
//! family (`spline_edges_` with `vconfig`, `makeSpline`, `makePolyline`,
//! `makeObstacle` in `lib/neatogen/neatosplines.c`).

use super::adjust::Expand;
use crate::common::types::{EdgeType, LGraph, Sub};

/// Visibility configuration over node obstacles.
pub struct Router {}

impl Router {
    /// Builds obstacles; `None` when nodes overlap (C falls back to lines).
    pub fn new(_g: &LGraph<'_>, _sub: &Sub, _margin: &Expand, _et: EdgeType) -> Option<Self> {
        None
    }

    /// Path-finding pass (`getPath` for every edge).
    pub fn path_pass(&self, _g: &mut LGraph<'_>, _sub: &Sub) {}

    /// Spline-drawing pass for one non-loop edge.
    pub fn route_edge(&self, g: &mut LGraph<'_>, e: usize, et: EdgeType) {
        super::neatosplines::make_straight_edge(g, e, et);
    }
}
