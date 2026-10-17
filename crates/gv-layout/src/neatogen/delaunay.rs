//! Delaunay triangulation edges (`delaunay_tri` in `lib/neatogen/delaunay.c`).
//!
//! C uses GTS's incremental Delaunay triangulation. This is an incremental
//! Bowyer–Watson triangulation inside an enclosing triangle, which yields
//! the same edge set for points in general position; ties between
//! co-circular points and the edge enumeration order (a GTS hash table) may
//! differ. Coincident points are merged like `gts_delaunay_add_vertex`.

#[derive(Clone, Copy)]
struct Tri {
    v: [usize; 3],
}

fn orient(ax: f64, ay: f64, bx: f64, by: f64, cx: f64, cy: f64) -> f64 {
    (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)
}

fn in_circumcircle(p: &[(f64, f64)], t: &Tri, q: (f64, f64)) -> bool {
    let (a, b, c) = (p[t.v[0]], p[t.v[1]], p[t.v[2]]);
    let (adx, ady) = (a.0 - q.0, a.1 - q.1);
    let (bdx, bdy) = (b.0 - q.0, b.1 - q.1);
    let (cdx, cdy) = (c.0 - q.0, c.1 - q.1);
    let det = (adx * adx + ady * ady) * (bdx * cdy - cdx * bdy)
        - (bdx * bdx + bdy * bdy) * (adx * cdy - cdx * ady)
        + (cdx * cdx + cdy * cdy) * (adx * bdy - bdx * ady);
    let o = orient(a.0, a.1, b.0, b.1, c.0, c.1);
    if o > 0.0 { det > 0.0 } else { det < 0.0 }
}

/// Edges `(i, j)` (`i < j`, sorted) of the Delaunay triangulation of the
/// points, or the chain along x (or y for a vertical line) when all points
/// are collinear, as C does when GTS returns no edges.
pub fn delaunay_tri(x: &[f64], y: &[f64]) -> Vec<(usize, usize)> {
    let n = x.len();
    let mut pts: Vec<(f64, f64)> = (0..n).map(|i| (x[i], y[i])).collect();
    // Enclosing triangle, like gts_triangle_enclosing(..., 100).
    let (mut xmin, mut ymin, mut xmax, mut ymax) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for &(px, py) in &pts {
        xmin = xmin.min(px);
        ymin = ymin.min(py);
        xmax = xmax.max(px);
        ymax = ymax.max(py);
    }
    let d = (xmax - xmin).max(ymax - ymin).max(1.0) * 100.0;
    let (cx, cy) = ((xmin + xmax) / 2.0, (ymin + ymax) / 2.0);
    pts.push((cx - 2.0 * d, cy - d));
    pts.push((cx + 2.0 * d, cy - d));
    pts.push((cx, cy + 2.0 * d));
    let mut tris = vec![Tri {
        v: [n, n + 1, n + 2],
    }];
    let mut alias: Vec<usize> = (0..n).collect();
    'outer: for i in 0..n {
        for j in 0..i {
            if alias[j] == j && pts[j] == pts[i] {
                alias[i] = j;
                continue 'outer;
            }
        }
        let q = pts[i];
        let mut bad = Vec::new();
        for (k, t) in tris.iter().enumerate() {
            if in_circumcircle(&pts, t, q) {
                bad.push(k);
            }
        }
        if bad.is_empty() {
            continue;
        }
        let mut boundary: Vec<(usize, usize)> = Vec::new();
        for &k in &bad {
            let t = tris[k];
            for e in 0..3 {
                let (a, b) = (t.v[e], t.v[(e + 1) % 3]);
                let shared = bad.iter().any(|&o| {
                    o != k && {
                        let u = tris[o].v;
                        (0..3).any(|f| {
                            (u[f] == b && u[(f + 1) % 3] == a) || (u[f] == a && u[(f + 1) % 3] == b)
                        })
                    }
                });
                if !shared {
                    boundary.push((a, b));
                }
            }
        }
        let mut keep: Vec<Tri> = tris
            .iter()
            .enumerate()
            .filter(|(k, _)| !bad.contains(k))
            .map(|(_, t)| *t)
            .collect();
        for (a, b) in boundary {
            let mut t = Tri { v: [a, b, i] };
            if orient(pts[a].0, pts[a].1, pts[b].0, pts[b].1, q.0, q.1) < 0.0 {
                t.v.swap(0, 1);
            }
            keep.push(t);
        }
        tris = keep;
    }
    let mut edges = std::collections::BTreeSet::new();
    for t in &tris {
        if t.v.iter().any(|&v| v >= n) {
            continue;
        }
        if orient(
            pts[t.v[0]].0,
            pts[t.v[0]].1,
            pts[t.v[1]].0,
            pts[t.v[1]].1,
            pts[t.v[2]].0,
            pts[t.v[2]].1,
        ) == 0.0
        {
            continue;
        }
        for e in 0..3 {
            let (a, b) = (t.v[e], t.v[(e + 1) % 3]);
            edges.insert((a.min(b), a.max(b)));
        }
    }
    if !edges.is_empty() {
        return edges.into_iter().collect();
    }
    // Collinear: chain the points sorted along x (y for a vertical line).
    let mut vs: Vec<usize> = (0..n).collect();
    let key: &[f64] = if n > 1 && x[0] == x[1] { y } else { x };
    crate::cutil::qsort_by(&mut vs, |a, b| {
        if key[*a] < key[*b] {
            -1
        } else if key[*a] > key[*b] {
            1
        } else {
            0
        }
    });
    vs.windows(2).map(|w| (w[0], w[1])).collect()
}
