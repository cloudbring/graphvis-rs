//! Graph scanning and the Kamada–Kawai solver, ported from
//! `lib/neatogen/stuff.c` (`scan_graph_mode`, `initial_positions`,
//! `diffeq_model`, `solve_model`, `choose_node`, `move_node`, `D2E`,
//! `update_arrays`, `shortest_path`/`s1` with its heap) and the KK distance
//! models of `neatoinit.c` (`subset_model`, `mds_model`, `circuit_model`).

use super::linalg::{solve, solve_circuit};
use super::stress::has_pos;
use super::{INIT_RANDOM, INIT_REGULAR, INIT_SELF, MODE_KK, MODE_SGD, NeatoParams};
use crate::common::types::{LGraph, P_SET, Sub};
use graphviz_pure_core::math;
use graphviz_pure_core::strtod::{atof, scan_lf};

thread_local! {
    /// `Reduce` (`dot -x`).
    static REDUCE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Sets `Reduce` (`dot -x`) for the layouts that follow on this thread.
pub fn set_reduce(on: bool) {
    REDUCE.with(|r| r.set(on));
}

/// `scan_graph_mode`'s `Reduce` pass: singletons and trees are deleted from the root graph
/// (`degreeKind`, `prune`). A pruned node's chain is followed while its other end is left
/// with one neighbour.
pub fn reduce_graph(graph: &mut graphviz_pure_core::Graph) {
    if !REDUCE.with(std::cell::Cell::get) {
        return;
    }
    let n = graph.nodes.len();
    // Each node's edges, as (edge ends).
    let mut incident: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    for e in &graph.edges {
        let (t, h) = (e.tail.0 as usize, e.head.0 as usize);
        for v in if t == h { vec![t] } else { vec![t, h] } {
            if let Some(list) = incident.get_mut(v) {
                list.push((t, h));
            }
        }
    }
    let mut deleted = vec![false; n];
    let gone = |deleted: &[bool], v: usize| deleted.get(v).copied().unwrap_or(true);
    // `degreeKind`: 0, 1 (with the neighbour) or 2 for more; loops and multi-edges to the
    // one neighbour do not count.
    let degree = |deleted: &[bool], v: usize| -> (u8, usize) {
        let mut other = None;
        for &(t, h) in incident.get(v).into_iter().flatten() {
            if t == h || gone(deleted, t) || gone(deleted, h) {
                continue;
            }
            let o = if t == v { h } else { t };
            match other {
                None => other = Some(o),
                Some(x) if x == o => {}
                Some(_) => return (2, 0),
            }
        }
        other.map_or((0, 0), |o| (1, o))
    };
    for v in 0..n {
        if gone(&deleted, v) {
            continue;
        }
        let mut next = match degree(&deleted, v) {
            (0, _) => Some((v, None)),
            (1, other) => Some((v, Some(other))),
            _ => None,
        };
        // The node, then `prune` along its chain.
        while let Some((p, other)) = next {
            if let Some(d) = deleted.get_mut(p) {
                *d = true;
            }
            next = other.and_then(|o| match degree(&deleted, o) {
                (0, _) => Some((o, None)),
                (1, oo) => Some((o, Some(oo))),
                _ => None,
            });
        }
    }
    if deleted.contains(&true) {
        graph.delete_nodes(&deleted);
    }
}

/// KK working arrays (`GD_dist`, `GD_spring`, `GD_sum_t`, `GD_t`).
pub struct KkState {
    pub dist: Vec<Vec<f64>>,
    pub spring: Vec<Vec<f64>>,
    pub sum_t: Vec<Vec<f64>>,
    pub t: Vec<Vec<Vec<f64>>>,
    pub nlist: Vec<usize>,
    pub moves: i64,
}

/// `lenattr`: the edge's `len`, or `None` to use the default. C also warns about a bad
/// value (`bad edge len`), which the port does not print yet.
fn lenattr(g: &LGraph<'_>, e: usize, declared: bool) -> Option<f64> {
    if !declared {
        return None;
    }
    let s = g.eattr(e, "len")?;
    if s.is_empty() {
        return None;
    }
    // Nop is never set here: `neato -n` does not reach scan_graph_mode.
    match scan_lf(s) {
        Some((v, _)) if v > 0.0 || v.is_nan() => Some(v),
        _ => None,
    }
}

/// `scan_graph_mode(G, mode)`: number the nodes of `sub`, set `ED_dist`,
/// `Epsilon`, `Damping` and `Initial_dist`. Returns the node count and, in
/// KK mode, the KK arrays.
pub fn scan_graph_mode(
    g: &mut LGraph<'_>,
    sub: &Sub,
    mode: i32,
    p: &mut NeatoParams,
) -> (usize, Option<KkState>) {
    let nv = sub.nodes.len();
    let ne = g.nedges(sub);
    let len_declared = g.edge_attr_declared("len");
    let getdouble = |g: &LGraph<'_>, name: &str, v: &mut f64| {
        if let Some((f, _)) = g.gattr(name).and_then(scan_lf) {
            *v = f;
        }
    };
    let dfltlen = 1.0;
    let mut total_len = 0.0;
    if mode == MODE_KK {
        p.epsilon = 0.0001 * nv as f64;
        getdouble(g, "epsilon", &mut p.epsilon);
        p.damping = match g.gattr("Damping") {
            Some(s) => atof(s),
            None => 0.99,
        };
    } else if mode == MODE_SGD {
        p.epsilon = 0.01;
        getdouble(g, "epsilon", &mut p.epsilon);
    } else {
        p.epsilon = super::stress::DFLT_TOLERANCE;
        getdouble(g, "epsilon", &mut p.epsilon);
    }
    for (i, &np) in sub.nodes.iter().enumerate() {
        g.nodes[np].id = i;
        g.nodes[np].heapindex = -1;
        let outs: Vec<usize> = g.outs(sub, np).collect();
        for e in outs {
            let len = lenattr(g, e, len_declared).unwrap_or(dfltlen);
            g.edges[e].dist = len;
            total_len += len;
        }
    }
    p.initial_dist = match g.gattr("defaultdist").filter(|s| !s.is_empty()) {
        Some(s) => p.epsilon.max(atof(s)),
        None => total_len / (if ne > 0 { ne } else { 1 }) as f64 * (nv as f64).sqrt() + 1.0,
    };
    let kk = (mode == MODE_KK).then(|| KkState {
        dist: vec![vec![p.initial_dist; nv]; nv],
        spring: vec![vec![1.0; nv]; nv],
        sum_t: vec![vec![1.0; g.ndim]; nv],
        t: vec![vec![vec![0.0; g.ndim]; nv]; nv],
        nlist: sub.nodes.clone(),
        moves: 0,
    });
    (nv, kk)
}

/// `initial_positions(G, nG)` for KK.
fn initial_positions(g: &mut LGraph<'_>, sub: &Sub, ng: usize, p: &NeatoParams) {
    let init = super::neatoinit::check_start(g, sub, ng, INIT_RANDOM, p);
    if init == INIT_REGULAR {
        return;
    }
    let _ = INIT_SELF;
    for &np in &sub.nodes {
        if has_pos(g, np) {
            continue;
        }
        let x = g.drand.drand48();
        let y = g.drand.drand48();
        g.nodes[np].pos[0] = x;
        g.nodes[np].pos[1] = y;
        for d in 2..g.ndim {
            g.nodes[np].pos[d] = g.drand.drand48();
        }
    }
}

fn distvec(p0: &[f64], p1: &[f64], vec: &mut [f64], ndim: usize) -> f64 {
    let mut dist = 0.0f64;
    for k in 0..ndim {
        vec[k] = p0[k] - p1[k];
        dist = vec[k].mul_add(vec[k], dist);
    }
    dist.sqrt()
}

/// `diffeq_model(G, nG)`.
fn diffeq_model(g: &LGraph<'_>, sub: &Sub, kk: &mut KkState, ng: usize) {
    let ndim = g.ndim;
    for i in 0..ng {
        for j in 0..i {
            let mut f = 1.0 / (kk.dist[i][j] * kk.dist[i][j]);
            if let Some(e) = g.find_edge(sub, kk.nlist[i], kk.nlist[j]) {
                f *= g.edges[e].factor;
            }
            kk.spring[i][j] = f;
            kk.spring[j][i] = f;
        }
    }
    for i in 0..ng {
        for k in 0..ndim {
            kk.sum_t[i][k] = 0.0;
        }
    }
    let mut del = vec![0.0; ndim];
    for i in 0..ng {
        for j in 0..ng {
            if i == j {
                continue;
            }
            let dist = distvec(
                &g.nodes[kk.nlist[i]].pos,
                &g.nodes[kk.nlist[j]].pos,
                &mut del,
                ndim,
            );
            for k in 0..ndim {
                // K * (del - D * del / dist)
                let v = kk.spring[i][j] * (del[k] - kk.dist[i][j] * del[k] / dist);
                kk.t[i][j][k] = v;
                kk.sum_t[i][k] += v;
            }
        }
    }
}

fn update_arrays(g: &LGraph<'_>, kk: &mut KkState, ng: usize, i: usize) {
    let ndim = g.ndim;
    for k in 0..ndim {
        kk.sum_t[i][k] = 0.0;
    }
    let mut del = vec![0.0; ndim];
    for j in 0..ng {
        if i == j {
            continue;
        }
        let dist = distvec(
            &g.nodes[kk.nlist[i]].pos,
            &g.nodes[kk.nlist[j]].pos,
            &mut del,
            ndim,
        );
        for k in 0..ndim {
            let v = kk.spring[i][j] * (del[k] - kk.dist[i][j] * del[k] / dist);
            kk.t[i][j][k] = v;
            kk.sum_t[i][k] += v;
            let old = kk.t[j][i][k];
            kk.t[j][i][k] = -v;
            kk.sum_t[j][k] += -v - old;
        }
    }
}

/// `D2E(G, nG, n, M)`: Hessian of the energy at node `n`.
fn d2e(g: &LGraph<'_>, kk: &KkState, ng: usize, n: usize, m: &mut [f64]) {
    let ndim = g.ndim;
    for v in m.iter_mut() {
        *v = 0.0;
    }
    let mut t = vec![0.0; ndim];
    let vn = &g.nodes[kk.nlist[n]].pos;
    for i in 0..ng {
        if n == i {
            continue;
        }
        let vi = &g.nodes[kk.nlist[i]].pos;
        let mut sq = 0.0f64;
        for k in 0..ndim {
            t[k] = vn[k] - vi[k];
            sq = t[k].mul_add(t[k], sq);
        }
        let scale = 1.0 / math::powf(sq, 1.5);
        let kni = kk.spring[n][i];
        let dni = kk.dist[n][i];
        for k in 0..ndim {
            for l in 0..k {
                m[l * ndim + k] = (kni * dni * t[k] * t[l]).mul_add(scale, m[l * ndim + k]);
            }
            // K * (1 - D * (sq - t*t) * scale)
            let inner = (-t[k]).mul_add(t[k], sq);
            let paren = (-(dni * inner)).mul_add(scale, 1.0);
            m[k * ndim + k] = kni.mul_add(paren, m[k * ndim + k]);
        }
    }
    for k in 1..ndim {
        for l in 0..k {
            m[k * ndim + l] = m[l * ndim + k];
        }
    }
}

fn choose_node(g: &LGraph<'_>, kk: &KkState, ng: usize, p: &NeatoParams) -> Option<usize> {
    if kk.moves >= p.max_iter {
        return None;
    }
    let mut max = 0.0;
    let mut choice = None;
    for i in 0..ng {
        let np = kk.nlist[i];
        if g.nodes[np].pinned > P_SET {
            continue;
        }
        let mut m = 0.0f64;
        for k in 0..g.ndim {
            m = kk.sum_t[i][k].mul_add(kk.sum_t[i][k], m);
        }
        if m > max {
            choice = Some(i);
            max = m;
        }
    }
    if max < p.epsilon * p.epsilon {
        None
    } else {
        choice
    }
}

fn move_node(g: &mut LGraph<'_>, kk: &mut KkState, ng: usize, m: usize, p: &NeatoParams) {
    let ndim = g.ndim;
    let mut a = vec![0.0; ndim * ndim];
    let mut b = vec![0.0; ndim];
    let mut c = vec![0.0; ndim];
    d2e(g, kk, ng, m, &mut a);
    for i in 0..ndim {
        c[i] = -kk.sum_t[m][i];
    }
    solve(&mut a, &mut b, &mut c, ndim);
    let np = kk.nlist[m];
    for i in 0..ndim {
        let r = g.drand.drand48();
        b[i] *= (2.0 * (1.0 - p.damping)).mul_add(r, p.damping);
        g.nodes[np].pos[i] += b[i];
    }
    kk.moves += 1;
    update_arrays(g, kk, ng, m);
}

/// `solve_model(G, nG)`.
fn solve_model(g: &mut LGraph<'_>, kk: &mut KkState, ng: usize, p: &NeatoParams) {
    while let Some(m) = choose_node(g, kk, ng, p) {
        move_node(g, kk, ng, m, p);
    }
}

/// `shortest_path(G, nG)` / `s1`: Dijkstra from every node with the
/// `neato_enqueue` heap, recording `GD_dist`.
pub fn shortest_path(
    g: &mut LGraph<'_>,
    sub: &Sub,
    kk: &mut KkState,
    ng: usize,
    initial_dist: f64,
) {
    let mut heap: Vec<usize> = Vec::with_capacity(ng + 1);
    for &src in &sub.nodes.clone() {
        for &v in &sub.nodes {
            g.nodes[v].dist = initial_dist;
        }
        g.nodes[src].dist = 0.0;
        g.nodes[src].hops = 0;
        heap.clear();
        enqueue(g, &mut heap, src);
        while let Some(v) = dequeue(g, &mut heap) {
            if v != src {
                let (i, j) = (g.nodes[src].id, g.nodes[v].id);
                kk.dist[i][j] = g.nodes[v].dist;
                kk.dist[j][i] = g.nodes[v].dist;
            }
            let edges: Vec<usize> = g.all_edges(sub, v).collect();
            for e in edges {
                let mut u = g.edges[e].tail;
                if u == v {
                    u = g.edges[e].head;
                }
                let f = g.nodes[v].dist + g.edges[e].dist;
                if g.nodes[u].dist > f {
                    g.nodes[u].dist = f;
                    if g.nodes[u].heapindex >= 0 {
                        heapup(g, &mut heap, u);
                    } else {
                        g.nodes[u].hops = g.nodes[v].hops + 1;
                        enqueue(g, &mut heap, u);
                    }
                }
            }
        }
    }
}

fn heapup(g: &mut LGraph<'_>, heap: &mut [usize], v: usize) {
    let mut i = g.nodes[v].heapindex as usize;
    while i > 0 {
        let par = (i - 1) / 2;
        let u = heap[par];
        if g.nodes[u].dist <= g.nodes[v].dist {
            break;
        }
        heap[par] = v;
        g.nodes[v].heapindex = par as i64;
        heap[i] = u;
        g.nodes[u].heapindex = i as i64;
        i = par;
    }
}

fn heapdown(g: &mut LGraph<'_>, heap: &mut [usize], v: usize) {
    let size = heap.len();
    let mut i = g.nodes[v].heapindex as usize;
    loop {
        let left = 2 * i + 1;
        if left >= size {
            break;
        }
        let right = left + 1;
        let c = if right < size && g.nodes[heap[right]].dist < g.nodes[heap[left]].dist {
            right
        } else {
            left
        };
        let u = heap[c];
        if g.nodes[v].dist <= g.nodes[u].dist {
            break;
        }
        heap[c] = v;
        g.nodes[v].heapindex = c as i64;
        heap[i] = u;
        g.nodes[u].heapindex = i as i64;
        i = c;
    }
}

fn enqueue(g: &mut LGraph<'_>, heap: &mut Vec<usize>, v: usize) {
    let i = heap.len();
    heap.push(v);
    g.nodes[v].heapindex = i as i64;
    if i > 0 {
        heapup(g, heap, v);
    }
}

fn dequeue(g: &mut LGraph<'_>, heap: &mut Vec<usize>) -> Option<usize> {
    if heap.is_empty() {
        return None;
    }
    let rv = heap[0];
    let last = heap.pop().unwrap();
    if !heap.is_empty() {
        heap[0] = last;
        g.nodes[last].heapindex = 0;
        if heap.len() > 1 {
            heapdown(g, heap, last);
        }
    }
    g.nodes[rv].heapindex = -1;
    Some(rv)
}

/// `kkNeato(g, nG, model)`: Kamada–Kawai gradient descent.
pub fn kk_neato(
    g: &mut LGraph<'_>,
    sub: &Sub,
    kk: &mut KkState,
    ng: usize,
    model: i32,
    p: &NeatoParams,
) {
    use super::{MODEL_CIRCUIT, MODEL_MDS, MODEL_SUBSET};
    if model == MODEL_SUBSET {
        let (gp, _, _) = super::graphdata::make_graph_data(g, sub, MODE_KK, MODEL_SUBSET);
        let dij = super::stress::compute_apsp_artificial_weights(&gp, ng);
        for i in 0..ng {
            for j in 0..ng {
                kk.dist[i][j] = f64::from(dij[i][j]);
            }
        }
    } else if model == MODEL_CIRCUIT {
        if !circuit_model_kk(g, sub, kk, ng) {
            shortest_path(g, sub, kk, ng, p.initial_dist);
        }
    } else if model == MODEL_MDS {
        shortest_path(g, sub, kk, ng, p.initial_dist);
        // C indexes GD_dist by AGSEQ here (1-based), which is undefined
        // behaviour for the last node; the port uses the layout ids.
        for &v in &sub.nodes {
            let outs: Vec<usize> = g.outs(sub, v).collect();
            for e in outs {
                let (i, j) = (g.nodes[g.edges[e].tail].id, g.nodes[g.edges[e].head].id);
                if i == j {
                    continue;
                }
                kk.dist[i][j] = g.edges[e].dist;
                kk.dist[j][i] = g.edges[e].dist;
            }
        }
    } else {
        shortest_path(g, sub, kk, ng, p.initial_dist);
    }
    initial_positions(g, sub, ng, p);
    diffeq_model(g, sub, kk, ng);
    solve_model(g, kk, ng, p);
}

/// `circuit_model(g, nG)` (`circuit.c`) for KK.
fn circuit_model_kk(g: &LGraph<'_>, sub: &Sub, kk: &mut KkState, ng: usize) -> bool {
    let mut gm = vec![vec![0.0f64; ng]; ng];
    let mut gm_inv = vec![vec![0.0f64; ng]; ng];
    for &v in &sub.nodes {
        for e in g.all_edges(sub, v) {
            let (i, j) = (g.nodes[g.edges[e].tail].id, g.nodes[g.edges[e].head].id);
            if i == j {
                continue;
            }
            let c = -1.0 / g.edges[e].dist;
            gm[i][j] = c;
            gm[j][i] = c;
        }
    }
    let rv = solve_circuit(ng, &mut gm, &mut gm_inv);
    if rv {
        for i in 0..ng {
            for j in 0..ng {
                kk.dist[i][j] = (-2.0f64).mul_add(gm_inv[i][j], gm_inv[i][i] + gm_inv[j][j]);
            }
        }
    }
    rv
}
