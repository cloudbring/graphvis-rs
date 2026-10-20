//! `lib/pathplan/inpoly.c`.

use super::Ppoint;
use super::visibility::wind;

/// `in_poly(poly, q)`: is `q` inside the convex, clockwise polygon?
pub fn in_poly(poly: &[Ppoint], q: Ppoint) -> bool {
    let n = poly.len();
    for i in 0..n {
        let i1 = (i + n - 1) % n;
        if wind(poly[i1], poly[i], q) == 1 {
            return false;
        }
    }
    true
}
