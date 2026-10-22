//! `lib/pathplan/cvt.c`: obstacle configurations and shortest paths among them.

use super::Ppoint;
use super::visibility::{make_path, pt_vis, visibility};

/// `vconfig_t`.
#[derive(Debug, Clone, Default)]
pub struct Vconfig {
    pub npoly: usize,
    /// Number of barrier points.
    pub n: usize,
    pub p: Vec<Ppoint>,
    pub start: Vec<usize>,
    pub next: Vec<usize>,
    pub prev: Vec<usize>,
    /// Visibility matrix (`N x N`).
    pub vis: Vec<Vec<f64>>,
}

/// `Pobsopen(obs, n_obs)`.
pub fn pobsopen(obs: &[Vec<Ppoint>]) -> Vconfig {
    let n: usize = obs.iter().map(Vec::len).sum();
    let mut rv = Vconfig {
        npoly: obs.len(),
        n,
        p: Vec::with_capacity(n),
        start: Vec::with_capacity(obs.len() + 1),
        next: vec![0; n],
        prev: vec![0; n],
        vis: Vec::new(),
    };
    let mut i = 0usize;
    for poly in obs {
        let start = i;
        rv.start.push(start);
        let end = start + poly.len() - 1;
        for pt in poly {
            rv.p.push(*pt);
            rv.next[i] = i + 1;
            rv.prev[i] = i.wrapping_sub(1);
            i += 1;
        }
        rv.next[end] = start;
        rv.prev[start] = end;
    }
    rv.start.push(i);
    visibility(&mut rv);
    rv
}

/// `Pobspath(config, p0, poly0, p1, poly1, &route)`.
///
/// Returns `None` when `p1` cannot be reached from `p0`: `shortestPath` then leaves a `-1`
/// in the `dad` chain, and C's `Pobspath` follows it to `dad[-1]`, reading before the
/// array, and loops forever there (#198: fdp's `compoundEdges` after `overlap=ipsep`).
pub fn pobspath(
    config: &Vconfig,
    p0: Ppoint,
    poly0: i32,
    p1: Ppoint,
    poly1: i32,
) -> Option<Vec<Ppoint>> {
    let ptvis0 = pt_vis(config, poly0, p0);
    let ptvis1 = pt_vis(config, poly1, p1);
    let dad = make_path(p0, poly0, &ptvis0, p1, poly1, &ptvis1, config);
    let n = config.n as i32;
    let mut mid = Vec::new();
    let mut i = dad[config.n];
    while i != n + 1 {
        // Each `dad` entry names a vertex finished earlier, so a chain without a `-1`
        // reaches `N + 1` within `N` steps.
        if i < 0 || mid.len() >= config.n {
            return None;
        }
        mid.push(config.p[i as usize]);
        i = dad[i as usize];
    }
    let mut ops = Vec::with_capacity(mid.len() + 2);
    ops.push(p0);
    ops.extend(mid.iter().rev());
    ops.push(p1);
    Some(ops)
}

#[cfg(test)]
mod tests {
    use super::super::POLYID_NONE;
    use super::*;

    /// Clockwise, as neato and fdp build their obstacles.
    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Ppoint> {
        [(x0, y0), (x0, y1), (x1, y1), (x1, y0)]
            .map(|(x, y)| Ppoint::new(x, y))
            .to_vec()
    }

    /// #198: an endpoint inside an obstacle sees no barrier vertex, so `shortestPath`
    /// leaves a `-1` in the `dad` chain: at `dad[N]` for an enclosed target, further down
    /// for an enclosed source. C's `Pobspath` then reads `dad[-1]`; the port finds no path.
    #[test]
    fn pobspath_with_an_enclosed_endpoint_finds_no_path() {
        let config = pobsopen(&[square(0.0, 0.0, 10.0, 10.0), square(20.0, 0.0, 30.0, 10.0)]);
        let outside = Ppoint::new(-5.0, 5.0);
        let enclosed = Ppoint::new(25.0, 5.0);
        let route = |p, q| pobspath(&config, p, POLYID_NONE, q, POLYID_NONE);
        assert_eq!(route(outside, enclosed), None);
        assert_eq!(route(enclosed, outside), None);
        // A route around both obstacles still walks the chain to the end.
        let (p, q) = (outside, Ppoint::new(35.0, 5.0));
        let ops = route(p, q).unwrap();
        assert!(ops.len() > 2, "{ops:?}");
        assert_eq!((ops[0], ops[ops.len() - 1]), (p, q));
    }
}
