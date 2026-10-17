//! `call_tri2` from `lib/neatogen/call_tri.c` (`call_tri` lives in
//! [`super::overlap`]).

use super::delaunay::ug_graph;
use crate::sparse::{Format, SparseMatrix};

/// `call_tri2(n, dim, xx)`: the relative neighbourhood graph plus the
/// diagonal, symmetrized.
pub fn call_tri2(n: usize, dim: usize, xx: &[f64]) -> SparseMatrix {
    let x: Vec<f64> = (0..n).map(|i| xx[dim * i]).collect();
    let y: Vec<f64> = (0..n).map(|i| xx[dim * i + 1]).collect();
    let g = ug_graph(&x, &y);
    let mut a = SparseMatrix::new(n, n, true, Format::Coord);
    for (i, nb) in g.iter().enumerate() {
        for &j in nb {
            a.coordinate_form_add_entry(i as i32, j as i32, 1.0);
        }
    }
    for i in 0..n {
        a.coordinate_form_add_entry(i as i32, i as i32, 1.0);
    }
    let mut b = a.from_coordinate_format();
    b.symmetrize(false)
}
