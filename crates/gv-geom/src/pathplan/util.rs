//! `lib/pathplan/util.c`.

use super::{Pedge, Ppoint};

/// `Ppolybarriers(polys, npolys, &barriers, &n)`: every polygon side as a segment.
pub fn ppolybarriers(polys: &[Vec<Ppoint>]) -> Vec<Pedge> {
    let mut bar = Vec::new();
    for pp in polys {
        for j in 0..pp.len() {
            let k = if j + 1 >= pp.len() { 0 } else { j + 1 };
            bar.push(Pedge { a: pp[j], b: pp[k] });
        }
    }
    bar
}

/// `make_polyline(line, &sline)`: a polyline as a piecewise-linear Bézier.
pub fn make_polyline(line: &[Ppoint]) -> Vec<Ppoint> {
    let mut out = Vec::with_capacity(3 * line.len());
    let n = line.len();
    out.push(line[0]);
    out.push(line[0]);
    let mut i = 1;
    while i + 1 < n {
        out.push(line[i]);
        out.push(line[i]);
        out.push(line[i]);
        i += 1;
    }
    out.push(line[i]);
    out.push(line[i]);
    out
}
