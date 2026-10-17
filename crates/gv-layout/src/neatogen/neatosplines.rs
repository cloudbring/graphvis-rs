//! Edge generation for the neato family, ported from
//! `lib/neatogen/neatosplines.c` (`spline_edges`, `spline_edges0`,
//! `splineEdges`, `spline_edges_`, `makeSelfArcs`, `neato_set_aspect`,
//! `neato_translate`) and `lib/common/routespl.c` (`makeStraightEdge(s)`).

use super::adjust::{Expand, esep_factor};
use crate::common::splines::{add_label_bb, make_port_labels, make_self_edge};
use crate::common::types::{EdgeType, LGraph, RatioKind, Sub};
use crate::common::utils::compute_bb;
use gv_core::Point;

/// `makeSelfArcs(e, stepx)`.
pub fn make_self_arcs(g: &mut LGraph<'_>, e: usize, stepx: f64) {
    let cnt = g.edges[e].count.max(0) as usize;
    if cnt == 1 || g.concentrate {
        make_self_edge(g, &[e], stepx, stepx);
        if let Some(l) = g.edges[e].label.clone() {
            add_label_bb(&mut g.bb, &l, false);
        }
        make_port_labels(g, e);
    } else if cnt > 1 {
        let mut edges = Vec::with_capacity(cnt);
        let mut cur = Some(e);
        for _ in 0..cnt {
            let Some(c) = cur else { break };
            edges.push(c);
            cur = g.edges[c].to_virt;
        }
        make_self_edge(g, &edges, stepx, stepx);
        for &e in &edges {
            if let Some(l) = g.edges[e].label.clone() {
                add_label_bb(&mut g.bb, &l, false);
            }
            make_port_labels(g, e);
        }
    }
}

/// `makeStraightEdge(g, e, et, sinfo)`: collect `e`'s equivalence class.
pub fn make_straight_edge(g: &mut LGraph<'_>, sub: &Sub, e: usize, et: EdgeType) {
    let mut list = vec![e];
    let mut e0 = e;
    while let Some(n) = g.edges[e0].to_virt {
        if n == e0 {
            break;
        }
        list.push(n);
        e0 = n;
    }
    make_straight_edges(g, sub, &list, et);
}

/// `makeStraightEdges(g, edge_list, e_cnt, et, sinfo)` (shared implementation
/// in [`crate::common::routespl::make_straight_edges`]).
pub fn make_straight_edges(g: &mut LGraph<'_>, sub: &Sub, edge_list: &[usize], et: EdgeType) {
    let (nodesep, conc) = (g.nodesep, g.concentrate);
    crate::common::routespl::make_straight_edges(g, edge_list, et, nodesep, conc, &|g, e| {
        get_cycle_centroid(g, sub, e)
    });
}

/// `equivEdge` key: canonicalised `(n1, p1, n2, p2)`.
fn equiv_key(g: &LGraph<'_>, e: usize) -> (usize, [u64; 2], usize, [u64; 2]) {
    let ed = &g.edges[e];
    let bits = |p: Point| [p.x.to_bits(), p.y.to_bits()];
    let (t, h) = (ed.tail, ed.head);
    let tp = ed.tail_port.p;
    let hp = ed.head_port.p;
    if t < h {
        (t, bits(tp), h, bits(hp))
    } else if t > h {
        (h, bits(hp), t, bits(tp))
    } else {
        let (p1, p2) = if tp.x < hp.x {
            (tp, hp)
        } else if tp.x > hp.x {
            (hp, tp)
        } else if tp.y < hp.y {
            (tp, hp)
        } else if tp.y > hp.y {
            (hp, tp)
        } else {
            (tp, tp)
        };
        (t, bits(p1), t, bits(p2))
    }
}

/// `splineEdges(g, spline_edges_, edgetype)`.
pub fn spline_edges_main(g: &mut LGraph<'_>, sub: &Sub, edgetype: EdgeType) {
    let margin = esep_factor(g);
    for &n in &sub.nodes {
        let outs: Vec<usize> = g.outs(sub, n).collect();
        for e in outs {
            crate::common::ports::resolve_ports(g, e);
        }
    }
    let mut map: std::collections::HashMap<(usize, [u64; 2], usize, [u64; 2]), usize> =
        std::collections::HashMap::new();
    for &n in &sub.nodes {
        let outs: Vec<usize> = g.outs(sub, n).collect();
        for e in outs {
            let key = equiv_key(g, e);
            match map.get(&key) {
                Some(&leader) => {
                    g.edges[leader].count += 1;
                    g.edges[e].to_virt = g.edges[leader].to_virt;
                    g.edges[leader].to_virt = Some(e);
                }
                None => {
                    g.edges[e].count = 1;
                    map.insert(key, e);
                }
            }
        }
    }
    spline_edges_route(g, sub, &margin, edgetype);
}

/// `spline_edges_(g, pmargin, edgetype)`.
fn spline_edges_route(g: &mut LGraph<'_>, sub: &Sub, pmargin: &Expand, edgetype: EdgeType) {
    let router = if edgetype >= EdgeType::Pline {
        super::routing::Router::new(g, sub, pmargin, edgetype)
    } else {
        None
    };
    if let Some(r) = &router {
        r.path_pass(g, sub);
    }
    for &n in &sub.nodes {
        let outs: Vec<usize> = g.outs(sub, n).collect();
        for e in outs {
            let head = g.edges[e].head;
            if g.edges[e].count == 0 {
                continue;
            }
            if n == head {
                let step = g.nodesep as f64;
                make_self_arcs(g, e, step);
            } else if let Some(r) = &router {
                r.route_edge(g, sub, e, edgetype);
            } else {
                make_straight_edge(g, sub, e, edgetype);
            }
        }
    }
}

/// `spline_edges0(g, set_aspect)`.
pub fn spline_edges0(g: &mut LGraph<'_>, sub: &Sub, is_root: bool, set_aspect: bool) {
    let mut et = g.edge_type;
    if set_aspect {
        neato_set_aspect(g, sub, is_root);
    }
    if et == EdgeType::None {
        return;
    }
    if et == EdgeType::Ortho || et == EdgeType::CompoundEdge {
        // lib/ortho is not ported; C falls back to polylines when ORTHO is
        // unavailable, which is the closest available routing.
        et = EdgeType::Pline;
    }
    spline_edges_main(g, sub, et);
}

/// `spline_edges(g)`: translate `sub` so its bounding box starts at the
/// origin, then route its edges.
pub fn spline_edges(g: &mut LGraph<'_>, sub: &mut Sub, is_root: bool) {
    compute_bb(g, sub, is_root);
    let ll = sub.bb.ll;
    let off = Point::new(ll.x / 72.0, ll.y / 72.0);
    for &n in &sub.nodes {
        g.nodes[n].pos[0] -= off.x;
        g.nodes[n].pos[1] -= off.y;
    }
    shift_clusters(g, sub, ll);
    if is_root {
        g.bb = sub.bb;
    }
    spline_edges0(g, sub, is_root, true);
    if is_root {
        sub.bb = g.bb;
    }
}

fn shift_clusters(g: &mut LGraph<'_>, sub: &mut Sub, off: Point) {
    fn rec(g: &mut LGraph<'_>, c: usize, off: Point) {
        for k in g.clusters[c].children.clone() {
            rec(g, k, off);
        }
        let bb = &mut g.clusters[c].bb;
        bb.ur.x -= off.x;
        bb.ur.y -= off.y;
        bb.ll.x -= off.x;
        bb.ll.y -= off.y;
    }
    for c in sub.clusters.clone() {
        rec(g, c, off);
    }
    sub.bb.ur.x -= off.x;
    sub.bb.ur.y -= off.y;
    sub.bb.ll.x -= off.x;
    sub.bb.ll.y -= off.y;
}

/// `neato_translate(g)`: move the root so its bounding box starts at the origin.
pub fn neato_translate(g: &mut LGraph<'_>) {
    let ll = g.bb.ll;
    let off = Point::new(ll.x / 72.0, ll.y / 72.0);
    for n in &mut g.nodes {
        n.pos[0] -= off.x;
        n.pos[1] -= off.y;
        if let Some(l) = n.xlabel.as_mut().filter(|l| l.set) {
            l.pos.x -= ll.x;
            l.pos.y -= ll.y;
        }
    }
    for e in &mut g.edges {
        if let Some(spl) = e.spl.as_mut() {
            for b in spl.iter_mut() {
                for p in &mut b.list {
                    p.x -= ll.x;
                    p.y -= ll.y;
                }
                if b.sflag != 0 {
                    b.sp.x -= ll.x;
                    b.sp.y -= ll.y;
                }
                if b.eflag != 0 {
                    b.ep.x -= ll.x;
                    b.ep.y -= ll.y;
                }
            }
            for l in [
                &mut e.label,
                &mut e.xlabel,
                &mut e.head_label,
                &mut e.tail_label,
            ]
            .into_iter()
            .flatten()
            {
                if l.set {
                    l.pos.x -= ll.x;
                    l.pos.y -= ll.y;
                }
            }
        }
    }
    translate_g(g, ll);
}

fn translate_g(g: &mut LGraph<'_>, off: Point) {
    let sh = |b: &mut gv_core::BoundingBox| {
        b.ur.x -= off.x;
        b.ur.y -= off.y;
        b.ll.x -= off.x;
        b.ll.y -= off.y;
    };
    sh(&mut g.bb);
    if let Some(l) = g.label.as_mut().filter(|l| l.set) {
        l.pos.x -= off.x;
        l.pos.y -= off.y;
    }
    for c in &mut g.clusters {
        sh(&mut c.bb);
        if let Some(l) = c.label.as_mut().filter(|l| l.set) {
            l.pos.x -= off.x;
            l.pos.y -= off.y;
        }
    }
}

fn scale_bb(g: &mut LGraph<'_>, xf: f64, yf: f64) {
    let sc = |b: &mut gv_core::BoundingBox| {
        b.ur.x *= xf;
        b.ur.y *= yf;
        b.ll.x *= xf;
        b.ll.y *= yf;
    };
    sc(&mut g.bb);
    if let Some(l) = g.label.as_mut().filter(|l| l.set) {
        l.pos.x *= xf;
        l.pos.y *= yf;
    }
    for c in &mut g.clusters {
        sc(&mut c.bb);
        if let Some(l) = c.label.as_mut().filter(|l| l.set) {
            l.pos.x *= xf;
            l.pos.y *= yf;
        }
    }
}

/// `_neato_set_aspect(g)`; true if a transformation was applied.
fn neato_set_aspect_inner(g: &mut LGraph<'_>, is_root: bool) -> bool {
    if !is_root {
        return false;
    }
    let kind = g.drawing.ratio_kind;
    if kind == RatioKind::None {
        return false;
    }
    let mut translated = false;
    if g.bb.ll.x != 0.0 || g.bb.ll.y != 0.0 {
        translated = true;
        neato_translate(g);
    }
    let (xf, yf);
    match kind {
        RatioKind::Fill => {
            if g.drawing.size.x <= 0.0 {
                return translated;
            }
            let mut x = g.drawing.size.x / g.bb.ur.x;
            let mut y = g.drawing.size.y / g.bb.ur.y;
            if x < 1.0 || y < 1.0 {
                if x < y {
                    y /= x;
                    x = 1.0;
                } else {
                    x /= y;
                    y = 1.0;
                }
            }
            (xf, yf) = (x, y);
        }
        RatioKind::Expand => {
            if g.drawing.size.x <= 0.0 {
                return translated;
            }
            let x = g.drawing.size.x / g.bb.ur.x;
            let y = g.drawing.size.y / g.bb.ur.y;
            if x > 1.0 && y > 1.0 {
                let s = x.min(y);
                (xf, yf) = (s, s);
            } else {
                return translated;
            }
        }
        RatioKind::Value => {
            let desired = g.drawing.ratio;
            let actual = g.bb.ur.y / g.bb.ur.x;
            if actual < desired {
                (xf, yf) = (1.0, desired / actual);
            } else {
                (xf, yf) = (actual / desired, 1.0);
            }
        }
        _ => return translated,
    }
    for n in &mut g.nodes {
        n.pos[0] *= xf;
        n.pos[1] *= yf;
    }
    scale_bb(g, xf, yf);
    true
}

/// `neato_set_aspect(g)`: apply `ratio` and copy `ND_pos` (inches) to `ND_coord` (points).
pub fn neato_set_aspect(g: &mut LGraph<'_>, sub: &Sub, is_root: bool) -> bool {
    let moved = neato_set_aspect_inner(g, is_root);
    for &n in &sub.nodes {
        let p = &g.nodes[n].pos;
        g.nodes[n].coord = Point::new(72.0 * p[0], 72.0 * p[1]);
    }
    moved
}

/// `find_all_cycles` / `get_cycle_centroid` from `routespl.c`.
fn get_cycle_centroid(g: &LGraph<'_>, sub: &Sub, edge: usize) -> Point {
    fn dfs(
        g: &LGraph<'_>,
        sub: &Sub,
        search: usize,
        visited: &mut Vec<usize>,
        end: usize,
        cycles: &mut Vec<Vec<usize>>,
    ) {
        if visited.contains(&search) {
            if search == end {
                let unique = !cycles
                    .iter()
                    .any(|c| c.len() == visited.len() && c.iter().all(|x| visited.contains(x)));
                if unique {
                    cycles.push(visited.clone());
                }
            }
        } else {
            visited.push(search);
            let outs: Vec<usize> = g.outs(sub, search).collect();
            for e in outs {
                dfs(g, sub, g.edges[e].head, visited, end, cycles);
            }
            visited.pop();
        }
    }
    let mut cycles = Vec::new();
    for &n in &sub.nodes {
        let mut visited = Vec::new();
        dfs(g, sub, n, &mut visited, n, &mut cycles);
    }
    let (start, end) = (g.edges[edge].tail, g.edges[edge].head);
    let contains = |c: &Vec<usize>| {
        let l = c.len();
        (0..l).any(|i| c[if i == 0 { l - 1 } else { i - 1 }] == start && c[i] == end)
    };
    let mut shortest: Option<&Vec<usize>> = None;
    for c in &cycles {
        if c.len() < 3 {
            continue;
        }
        if (shortest.is_none() || shortest.unwrap().len() > c.len()) && contains(c) {
            shortest = Some(c);
        }
    }
    let Some(cycle) = shortest else {
        let bb = if sub.nodes.len() == g.nodes.len() {
            g.bb
        } else {
            sub.bb
        };
        return Point::new((bb.ll.x + bb.ur.x) / 2.0, (bb.ll.y + bb.ur.y) / 2.0);
    };
    let mut sum = Point::new(0.0, 0.0);
    let mut cnt = 0.0;
    for &n in cycle {
        sum.x += g.nodes[n].coord.x;
        sum.y += g.nodes[n].coord.y;
        cnt += 1.0;
    }
    Point::new(sum.x / cnt, sum.y / cnt)
}
