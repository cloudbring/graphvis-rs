//! Port of Graphviz `lib/pathplan` (16.1.0): shortest paths inside simple
//! polygons, visibility graphs among polygonal obstacles, and spline fitting
//! that avoids barrier segments.
//!
//! Module ↔ C file map:
//! - [`util`] — `util.c` (`make_polyline`, `Ppolybarriers`)
//! - [`shortest`] — `shortest.c` (`Pshortestpath`)
//! - [`route`] — `route.c` (`Proutespline`)
//! - [`solvers`] — `solvers.c` (`solve3`)
//! - [`cvt`] — `cvt.c` (`Pobsopen`, `Pobspath`)
//! - [`visibility`] — `visibility.c` and `shortestpth.c`
//! - [`inpoly`] — `inpoly.c` (`in_poly`)
//! - [`triang`] — `triang.c` (`ccw`, `isdiagonal`, `Ptriangulate`)
//!
//! Floating-point expressions follow the C source as compiled by clang with
//! contraction enabled (`a*b + c*d` evaluates as `fma(a, b, c*d)`), so results
//! match the reference build bit for bit where the platform `libm` agrees.

pub mod cvt;
pub mod inpoly;
pub mod route;
pub mod shortest;
pub mod solvers;
pub mod triang;
pub mod util;
pub mod visibility;

pub use gv_core::Point as Ppoint;

/// `Pedge_t`: a barrier segment.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pedge {
    pub a: Ppoint,
    pub b: Ppoint,
}

/// `POLYID_NONE` / `POLYID_UNKNOWN` (`vispath.h`).
pub const POLYID_NONE: i32 = -1111;
pub const POLYID_UNKNOWN: i32 = -2222;

pub use cvt::{Vconfig, pobsopen, pobspath};
pub use inpoly::in_poly;
pub use route::proutespline;
pub use shortest::pshortestpath;
pub use util::{make_polyline, ppolybarriers};
