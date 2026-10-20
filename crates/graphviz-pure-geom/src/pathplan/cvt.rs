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
pub fn pobspath(config: &Vconfig, p0: Ppoint, poly0: i32, p1: Ppoint, poly1: i32) -> Vec<Ppoint> {
    let ptvis0 = pt_vis(config, poly0, p0);
    let ptvis1 = pt_vis(config, poly1, p1);
    let dad = make_path(p0, poly0, &ptvis0, p1, poly1, &ptvis1, config);
    let n = config.n as i32;
    let mut mid = Vec::new();
    let mut i = dad[config.n];
    while i != n + 1 {
        mid.push(config.p[i as usize]);
        i = dad[i as usize];
    }
    let mut ops = Vec::with_capacity(mid.len() + 2);
    ops.push(p0);
    ops.extend(mid.iter().rev());
    ops.push(p1);
    ops
}
