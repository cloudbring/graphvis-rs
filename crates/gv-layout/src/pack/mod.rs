//! Port of Graphviz `lib/pack`: connected components (`ccomps.c`) and
//! packing of disconnected layouts (`pack.c`, polyomino packing after
//! Freivalds et al., plus array packing).
//!
//! Components are [`Sub`] views over an [`LGraph`]; packing reads node
//! positions (`ND_pos`, inches), sizes and installed edge splines, and
//! translates each component like `shiftGraphs`.

// Faithful ports of numeric C loops index several arrays in lockstep.
#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

use crate::common::geom::{add, sub as psub};
use crate::common::types::{LGraph, P_PIN, Sub};
use crate::common::utils::{compute_bb, coord, parse_leading_int};
use crate::cutil::qsort_by;
use gv_core::{BoundingBox, Point};
use std::collections::HashSet;

/// `pack_mode`; order matters (`mode <= l_graph` selects polyomino packing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum PackMode {
    #[default]
    Undef,
    Clust,
    Node,
    Graph,
    Array,
    Aspect,
}

pub const PK_COL_MAJOR: u32 = 1 << 0;
pub const PK_USER_VALS: u32 = 1 << 1;
pub const PK_TOP_ALIGN: u32 = 1 << 2;
pub const PK_LEFT_ALIGN: u32 = 1 << 3;
pub const PK_RIGHT_ALIGN: u32 = 1 << 4;
pub const PK_BOT_ALIGN: u32 = 1 << 5;
pub const PK_INPUT_ORDER: u32 = 1 << 6;

/// `pack_info`.
#[derive(Debug, Clone, Default)]
pub struct PackInfo {
    pub aspect: f32,
    pub sz: i32,
    pub margin: u32,
    pub do_splines: bool,
    pub mode: PackMode,
    pub fixed: Option<Vec<bool>>,
    pub vals: Option<Vec<u32>>,
    pub flags: u32,
}

fn chk_flags<'a>(p: &'a str, pinfo: &mut PackInfo) -> &'a str {
    let Some(mut rest) = p.strip_prefix('_') else {
        return p;
    };
    while let Some(c) = rest.chars().next() {
        let f = match c {
            'c' => PK_COL_MAJOR,
            'i' => PK_INPUT_ORDER,
            'u' => PK_USER_VALS,
            't' => PK_TOP_ALIGN,
            'b' => PK_BOT_ALIGN,
            'l' => PK_LEFT_ALIGN,
            'r' => PK_RIGHT_ALIGN,
            _ => break,
        };
        pinfo.flags |= f;
        rest = &rest[1..];
    }
    rest
}

/// `parsePackModeInfo(p, dflt, pinfo)` / `getPackModeInfo`.
pub fn get_pack_mode_info(p: Option<&str>, dflt: PackMode, pinfo: &mut PackInfo) -> PackMode {
    pinfo.flags = 0;
    pinfo.mode = dflt;
    pinfo.sz = 0;
    pinfo.vals = None;
    if let Some(p) = p {
        if let Some(r) = p.strip_prefix("array") {
            pinfo.mode = PackMode::Array;
            let r = chk_flags(r, pinfo);
            if let Some(i) = parse_leading_int(r)
                && i > 0
            {
                pinfo.sz = i as i32;
            }
        } else if let Some(r) = p.strip_prefix("aspect") {
            pinfo.mode = PackMode::Aspect;
            pinfo.aspect = match crate::common::utils::parse_leading_f64(r) {
                Some(v) if v > 0.0 => v as f32,
                _ => 1.0,
            };
        } else if p == "cluster" {
            pinfo.mode = PackMode::Clust;
        } else if p == "graph" {
            pinfo.mode = PackMode::Graph;
        } else if p == "node" {
            pinfo.mode = PackMode::Node;
        }
    }
    pinfo.mode
}

/// `getPack(g, not_def, dflt)`: the `pack` attribute.
pub fn get_pack(p: Option<&str>, not_def: i64, dflt: i64) -> i64 {
    let Some(p) = p else { return not_def };
    match parse_leading_int(p) {
        Some(i) if i >= 0 => i,
        Some(_) => not_def,
        None if p.starts_with('t') || p.starts_with('T') => dflt,
        None => not_def,
    }
}

/// `getPackInfo(g, dflt, dfltMargin, pinfo)`.
pub fn get_pack_info(
    g: &LGraph<'_>,
    dflt: PackMode,
    dflt_margin: i64,
    pinfo: &mut PackInfo,
) -> PackMode {
    pinfo.margin = get_pack(g.gattr("pack"), dflt_margin, dflt_margin) as u32;
    pinfo.do_splines = false;
    pinfo.fixed = None;
    get_pack_mode_info(g.gattr("packmode"), dflt, pinfo)
}

/// DFS of `ccomps.c`: marks and collects the component of `n`.
fn dfs(g: &LGraph<'_>, root: &Sub, n: usize, mark: &mut [bool], out: &mut Vec<usize>) {
    let mut stack = vec![n];
    mark[n] = true;
    while let Some(v) = stack.pop() {
        out.push(v);
        for e in g.all_edges(root, v) {
            let mut other = g.edges[e].tail;
            if other == v {
                other = g.edges[e].head;
            }
            if !mark[other] {
                mark[other] = true;
                stack.push(other);
            }
        }
    }
}

/// `pccomps(g, &ncc, pfx, &pinned)`: connected components, with all pinned
/// nodes (and what they reach) in the first one. Returns the components and
/// whether a pinned component exists.
pub fn pccomps(g: &LGraph<'_>) -> (Vec<Sub>, bool) {
    let root = g.root_sub();
    let n = g.nodes.len();
    let mut mark = vec![false; n];
    let mut out = Vec::new();
    let mut pinned_nodes = Vec::new();
    let mut pin = false;
    for v in 0..n {
        if mark[v] || g.nodes[v].pinned != P_PIN {
            continue;
        }
        pin = true;
        dfs(g, &root, v, &mut mark, &mut pinned_nodes);
    }
    if pin {
        out.push(Sub::new(n, pinned_nodes));
    }
    for v in 0..n {
        if mark[v] {
            continue;
        }
        let mut c = Vec::new();
        dfs(g, &root, v, &mut mark, &mut c);
        out.push(Sub::new(n, c));
    }
    (out, pin)
}

/// `ccomps(g, &ncc, pfx)`: connected components in node order.
pub fn ccomps(g: &LGraph<'_>) -> Vec<Sub> {
    let root = g.root_sub();
    let n = g.nodes.len();
    let mut mark = vec![false; n];
    let mut out = Vec::new();
    for v in 0..n {
        if mark[v] {
            continue;
        }
        let mut c = Vec::new();
        dfs(g, &root, v, &mut mark, &mut c);
        out.push(Sub::new(n, c));
    }
    out
}

const C: f64 = 100.0;

fn c_round(f: f64) -> i64 {
    crate::common::utils::c_round(f)
}

/// `GRID(x, s)`.
fn grid(x: f64, s: i32) -> i32 {
    (x / f64::from(s)).ceil() as i32
}

/// `CVAL(v, s)` on doubles.
fn cval(v: f64, s: i32) -> f64 {
    let s = f64::from(s);
    if v >= 0.0 {
        v / s
    } else {
        ((v + 1.0) / s) - 1.0
    }
}

fn cell(p: Point, s: i32) -> Point {
    Point::new(cval(p.x, s), cval(p.y, s))
}

fn round_pt(p: Point) -> Point {
    Point::new(p.x.round(), p.y.round())
}

/// `computeStep(ng, bbs, margin)`.
fn compute_step(bbs: &[BoundingBox], margin: u32) -> i32 {
    let ng = bbs.len();
    let a = C * ng as f64 - 1.0;
    let mut b = 0.0;
    let mut c = 0.0;
    for bb in bbs {
        let w = bb.ur.x - bb.ll.x + 2.0 * f64::from(margin);
        let h = bb.ur.y - bb.ll.y + 2.0 * f64::from(margin);
        b -= w + h;
        c = (-w).mul_add(h, c);
    }
    let d = b.mul_add(b, -(4.0 * a * c));
    let r = d.sqrt();
    let l1 = (-b + r) / (2.0 * a);
    let root = l1 as i32;
    if root == 0 { 1 } else { root }
}

struct GInfo {
    perim: i32,
    cells: Vec<(i64, i64)>,
    index: usize,
}

type PointSet = HashSet<(i64, i64)>;

fn sgn(x: i64) -> i64 {
    if x > 0 { 1 } else { -1 }
}

/// `fillLine(p, q, ps)`: Bresenham over cells.
fn fill_line(p: Point, q: Point, ps: &mut PointSet) {
    let (x1, y1, x2, y2) = (c_round(p.x), c_round(p.y), c_round(q.x), c_round(q.y));
    let dx = x2 - x1;
    let ax = dx.abs() << 1;
    let sx = sgn(dx);
    let dy = y2 - y1;
    let ay = dy.abs() << 1;
    let sy = sgn(dy);
    let (mut x, mut y) = (x1, y1);
    if ax > ay {
        let mut d = ay - (ax >> 1);
        loop {
            ps.insert((x, y));
            if x == x2 {
                return;
            }
            if d >= 0 {
                y += sy;
                d -= ax;
            }
            x += sx;
            d += ay;
        }
    } else {
        let mut d = ax - (ay >> 1);
        loop {
            ps.insert((x, y));
            if y == y2 {
                return;
            }
            if d >= 0 {
                x += sx;
                d -= ay;
            }
            y += sy;
            d += ax;
        }
    }
}

/// `fillEdge(e, p, ps, dx, dy, ssize, doS)`.
fn fill_edge(
    g: &LGraph<'_>,
    e: usize,
    p: Point,
    ps: &mut PointSet,
    dx: f64,
    dy: f64,
    ssize: i32,
    do_s: bool,
) {
    let mv = |q: Point| Point::new(q.x + dx, q.y + dy);
    let spl = g.edges[e].spl.as_ref();
    if !do_s || spl.is_none() {
        let hpt = cell(mv(coord(g, g.edges[e].head)), ssize);
        fill_line(p, hpt, ps);
        return;
    }
    for bz in spl.unwrap() {
        let (mut pt, mut hpt, mut k) = if bz.sflag != 0 {
            (bz.sp, bz.list[0], 1)
        } else {
            (bz.list[0], bz.list[1], 2)
        };
        pt = cell(mv(pt), ssize);
        hpt = cell(mv(hpt), ssize);
        fill_line(pt, hpt, ps);
        while k < bz.list.len() {
            pt = hpt;
            hpt = cell(mv(bz.list[k]), ssize);
            fill_line(pt, hpt, ps);
            k += 1;
        }
        if bz.eflag != 0 {
            pt = hpt;
            hpt = cell(mv(bz.ep), ssize);
            fill_line(pt, hpt, ps);
        }
    }
}

fn add_rect(ps: &mut PointSet, ll: Point, ur: Point) {
    let mut x = ll.x;
    while x <= ur.x {
        let mut y = ll.y;
        while y <= ur.y {
            ps.insert((x as i64, y as i64));
            y += 1.0;
        }
        x += 1.0;
    }
}

/// `genBox(bb0, info, ssize, margin, center)`.
fn gen_box(bb0: &BoundingBox, ssize: i32, margin: u32, center: Point, index: usize) -> GInfo {
    let m = f64::from(margin);
    let bb = BoundingBox::new(
        bb0.ll.x.round(),
        bb0.ll.y.round(),
        bb0.ur.x.round(),
        bb0.ur.y.round(),
    );
    let mut ps = PointSet::new();
    let ll = round_pt(cell(Point::new(center.x - m, center.y - m), ssize));
    let ur = round_pt(cell(
        Point::new(
            center.x + bb.ur.x - bb.ll.x + m,
            center.y + bb.ur.y - bb.ll.y + m,
        ),
        ssize,
    ));
    add_rect(&mut ps, ll, ur);
    let w = grid(bb0.ur.x - bb0.ll.x + 2.0 * m, ssize);
    let h = grid(bb0.ur.y - bb0.ll.y + 2.0 * m, ssize);
    GInfo {
        perim: w + h,
        cells: ps.into_iter().collect(),
        index,
    }
}

/// `genPoly(root, g, info, ssize, pinfo, center)`.
fn gen_poly(
    g: &LGraph<'_>,
    sub: &Sub,
    ssize: i32,
    pinfo: &PackInfo,
    center: Point,
    index: usize,
) -> GInfo {
    let mut ps = PointSet::new();
    let margin = f64::from(pinfo.margin);
    let dx = center.x - sub.bb.ll.x.round();
    let dy = center.y - sub.bb.ll.y.round();
    let root = g.root_sub();
    let mut clust_of: Vec<Option<usize>> = vec![None; g.nodes.len()];
    if pinfo.mode == PackMode::Clust {
        for &c in &sub.clusters {
            let b = g.clusters[c].bb;
            let mut bb = BoundingBox::new(
                b.ll.x.round(),
                b.ll.y.round(),
                b.ur.x.round(),
                b.ur.y.round(),
            );
            if bb.ur.x > bb.ll.x && bb.ur.y > bb.ll.y {
                bb.ll = Point::new(bb.ll.x + dx - margin, bb.ll.y + dy - margin);
                bb.ur = Point::new(bb.ur.x + dx + margin, bb.ur.y + dy + margin);
                let ll = round_pt(cell(bb.ll, ssize));
                let ur = round_pt(cell(bb.ur, ssize));
                add_rect(&mut ps, ll, ur);
                for n in crate::neatogen::neatoinit::subgraph_nodes(g.src, g.clusters[c].subgraph) {
                    clust_of[n] = Some(c);
                }
            }
        }
    }
    for &n in &sub.nodes {
        let ptf = coord(g, n);
        let mut pt = Point::new(ptf.x.round() + dx, ptf.y.round() + dy);
        let in_clust = clust_of[n];
        if in_clust.is_none() {
            let nd = &g.nodes[n];
            let s2 = Point::new(
                (margin + (nd.lw + nd.rw) / 2.0).round(),
                (margin + nd.ht / 2.0).round(),
            );
            let ll = round_pt(cell(psub(pt, s2), ssize));
            let ur = round_pt(cell(add(pt, s2), ssize));
            add_rect(&mut ps, ll, ur);
        }
        pt = round_pt(cell(pt, ssize));
        for e in g.outs(&root, n) {
            if in_clust.is_some() && in_clust == clust_of[g.edges[e].head] {
                continue;
            }
            fill_edge(g, e, pt, &mut ps, dx, dy, ssize, pinfo.do_splines);
        }
    }
    let w = grid(sub.bb.ur.x - sub.bb.ll.x + 2.0 * margin, ssize);
    let h = grid(sub.bb.ur.y - sub.bb.ll.y + 2.0 * margin, ssize);
    GInfo {
        perim: w + h,
        cells: ps.into_iter().collect(),
        index,
    }
}

/// `fits(x, y, info, ps, place, step, bbs)`.
fn fits(
    x: i64,
    y: i64,
    info: &GInfo,
    ps: &mut PointSet,
    step: i32,
    bbs: &[BoundingBox],
) -> Option<Point> {
    if info
        .cells
        .iter()
        .any(|&(cx, cy)| ps.contains(&(cx + x, cy + y)))
    {
        return None;
    }
    let ll = Point::new(bbs[info.index].ll.x.round(), bbs[info.index].ll.y.round());
    let place = Point::new(
        (i64::from(step) * x) as f64 - ll.x,
        (i64::from(step) * y) as f64 - ll.y,
    );
    for &(cx, cy) in &info.cells {
        ps.insert((cx + x, cy + y));
    }
    Some(place)
}

/// `placeFixed(info, ps, place, center)`.
fn place_fixed(info: &GInfo, ps: &mut PointSet, center: Point) -> Point {
    for &c in &info.cells {
        ps.insert(c);
    }
    Point::new(-center.x, -center.y)
}

/// `placeGraph(i, info, ps, place, step, margin, bbs)`.
fn place_graph(
    i: usize,
    info: &GInfo,
    ps: &mut PointSet,
    step: i32,
    margin: u32,
    bbs: &[BoundingBox],
) -> Point {
    let bb = bbs[info.index];
    if i == 0 {
        let w = grid(bb.ur.x - bb.ll.x + 2.0 * f64::from(margin), step);
        let h = grid(bb.ur.y - bb.ll.y + 2.0 * f64::from(margin), step);
        if let Some(p) = fits(i64::from(-w / 2), i64::from(-h / 2), info, ps, step, bbs) {
            return p;
        }
    }
    if let Some(p) = fits(0, 0, info, ps, step, bbs) {
        return p;
    }
    let w = (bb.ur.x - bb.ll.x).ceil();
    let h = (bb.ur.y - bb.ll.y).ceil();
    let mut bnd: i64 = 1;
    loop {
        if w >= h {
            let (mut x, mut y) = (0i64, -bnd);
            while x < bnd {
                if let Some(p) = fits(x, y, info, ps, step, bbs) {
                    return p;
                }
                x += 1;
            }
            while y < bnd {
                if let Some(p) = fits(x, y, info, ps, step, bbs) {
                    return p;
                }
                y += 1;
            }
            while x > -bnd {
                if let Some(p) = fits(x, y, info, ps, step, bbs) {
                    return p;
                }
                x -= 1;
            }
            while y > -bnd {
                if let Some(p) = fits(x, y, info, ps, step, bbs) {
                    return p;
                }
                y -= 1;
            }
            while x < 0 {
                if let Some(p) = fits(x, y, info, ps, step, bbs) {
                    return p;
                }
                x += 1;
            }
        } else {
            let (mut x, mut y) = (-bnd, 0i64);
            while y > -bnd {
                if let Some(p) = fits(x, y, info, ps, step, bbs) {
                    return p;
                }
                y -= 1;
            }
            while x < bnd {
                if let Some(p) = fits(x, y, info, ps, step, bbs) {
                    return p;
                }
                x += 1;
            }
            while y < bnd {
                if let Some(p) = fits(x, y, info, ps, step, bbs) {
                    return p;
                }
                y += 1;
            }
            while x > -bnd {
                if let Some(p) = fits(x, y, info, ps, step, bbs) {
                    return p;
                }
                x -= 1;
            }
            while y > 0 {
                if let Some(p) = fits(x, y, info, ps, step, bbs) {
                    return p;
                }
                y -= 1;
            }
        }
        bnd += 1;
    }
}

fn cmpf(x: &usize, y: &usize, infos: &[GInfo]) -> i32 {
    let (a, b) = (infos[*x].perim, infos[*y].perim);
    if b < a {
        -1
    } else if b > a {
        1
    } else {
        0
    }
}

/// `polyGraphs(ng, gs, root, pinfo)`.
fn poly_graphs(g: &mut LGraph<'_>, gs: &mut [Sub], pinfo: &PackInfo) -> Option<Vec<Point>> {
    let ng = gs.len();
    if ng == 0 {
        return None;
    }
    let mut fixed_bb = BoundingBox::default();
    let mut fixed_cnt = 0;
    for (i, sub) in gs.iter_mut().enumerate() {
        compute_bb(g, sub, false);
        if let Some(f) = &pinfo.fixed
            && f[i]
        {
            let b = sub.bb;
            let bb = BoundingBox::new(
                b.ll.x.round(),
                b.ll.y.round(),
                b.ur.x.round(),
                b.ur.y.round(),
            );
            if fixed_cnt > 0 {
                fixed_bb.ll.x = bb.ll.x.min(fixed_bb.ll.x);
                fixed_bb.ll.y = bb.ll.y.min(fixed_bb.ll.y);
                fixed_bb.ur.x = bb.ur.x.max(fixed_bb.ur.x);
                fixed_bb.ur.y = bb.ur.y.max(fixed_bb.ur.y);
            } else {
                fixed_bb = bb;
            }
            fixed_cnt += 1;
        }
    }
    let bbs: Vec<BoundingBox> = gs.iter().map(|s| s.bb).collect();
    let step = compute_step(&bbs, pinfo.margin);
    if step <= 0 {
        return None;
    }
    let mut center = Point::default();
    if pinfo.fixed.is_some() {
        center = Point::new(
            ((fixed_bb.ll.x + fixed_bb.ur.x) / 2.0).round(),
            ((fixed_bb.ll.y + fixed_bb.ur.y) / 2.0).round(),
        );
    }
    let infos: Vec<GInfo> = gs
        .iter()
        .enumerate()
        .map(|(i, s)| {
            if pinfo.mode == PackMode::Graph {
                gen_box(&s.bb, step, pinfo.margin, center, i)
            } else {
                gen_poly(g, s, step, pinfo, center, i)
            }
        })
        .collect();
    let mut sinfo: Vec<usize> = (0..ng).collect();
    qsort_by(&mut sinfo, |a, b| cmpf(a, b, &infos));
    let mut ps = PointSet::new();
    let mut places = vec![Point::default(); ng];
    if let Some(fixed) = &pinfo.fixed {
        for i in 0..ng {
            if fixed[i] {
                let inf = &infos[sinfo[i]];
                places[inf.index] = place_fixed(inf, &mut ps, center);
            }
        }
        for i in 0..ng {
            if !fixed[i] {
                let inf = &infos[sinfo[i]];
                places[inf.index] = place_graph(i, inf, &mut ps, step, pinfo.margin, &bbs);
            }
        }
    } else {
        for (i, &si) in sinfo.iter().enumerate() {
            let inf = &infos[si];
            places[inf.index] = place_graph(i, inf, &mut ps, step, pinfo.margin, &bbs);
        }
    }
    Some(places)
}

/// `INC(m, c, r, nc, nr)`.
fn inc(m: bool, c: &mut usize, r: &mut usize, nc: usize, nr: usize) {
    if m {
        *c += 1;
        if *c == nc {
            *c = 0;
            *r += 1;
        }
    } else {
        *r += 1;
        if *r == nr {
            *r = 0;
            *c += 1;
        }
    }
}

/// `arrayRects(ng, gs, pinfo)`.
pub fn array_rects(gs: &[BoundingBox], pinfo: &PackInfo) -> Vec<Point> {
    let ng = gs.len();
    let sz = pinfo.sz;
    let (row_major, nr, nc);
    if pinfo.flags & PK_COL_MAJOR != 0 {
        row_major = false;
        nr = if sz > 0 {
            sz as usize
        } else {
            (ng as f64).sqrt().ceil() as usize
        };
        nc = ng.div_ceil(nr);
    } else {
        row_major = true;
        nc = if sz > 0 {
            sz as usize
        } else {
            (ng as f64).sqrt().ceil() as usize
        };
        nr = ng.div_ceil(nc);
    }
    let mut widths = vec![0.0f64; nc + 1];
    let mut heights = vec![0.0f64; nr + 1];
    let m = f64::from(pinfo.margin);
    let info: Vec<(f64, f64, usize)> = gs
        .iter()
        .enumerate()
        .map(|(i, b)| (b.ur.x - b.ll.x + m, b.ur.y - b.ll.y + m, i))
        .collect();
    let mut sinfo: Vec<usize> = (0..ng).collect();
    if let Some(vals) = &pinfo.vals {
        // gv_sort is a stable merge sort over user values.
        sinfo.sort_by_key(|&i| vals[info[i].2]);
    } else if pinfo.flags & PK_INPUT_ORDER == 0 {
        qsort_by(&mut sinfo, |a, b| {
            let da = info[*a].1 + info[*a].0;
            let db = info[*b].1 + info[*b].0;
            if da < db {
                1
            } else if da > db {
                -1
            } else {
                0
            }
        });
    }
    let (mut r, mut c) = (0usize, 0usize);
    for &si in &sinfo {
        widths[c] = widths[c].max(info[si].0);
        heights[r] = heights[r].max(info[si].1);
        inc(row_major, &mut c, &mut r, nc, nr);
    }
    let mut wd = 0.0;
    for w in widths.iter_mut() {
        let v = *w;
        *w = wd;
        wd += v;
    }
    let mut ht = 0.0;
    for i in (1..=nr).rev() {
        let v = heights[i - 1];
        heights[i] = ht;
        ht += v;
    }
    heights[0] = ht;
    let mut places = vec![Point::default(); ng];
    let (mut r, mut c) = (0usize, 0usize);
    for &si in &sinfo {
        let idx = info[si].2;
        let bb = gs[idx];
        let x = if pinfo.flags & PK_LEFT_ALIGN != 0 {
            widths[c].round()
        } else if pinfo.flags & PK_RIGHT_ALIGN != 0 {
            (widths[c + 1] - (bb.ur.x - bb.ll.x)).round()
        } else {
            ((widths[c] + widths[c + 1] - bb.ur.x - bb.ll.x) / 2.0).round()
        };
        let y = if pinfo.flags & PK_TOP_ALIGN != 0 {
            (heights[r] - (bb.ur.y - bb.ll.y)).round()
        } else if pinfo.flags & PK_BOT_ALIGN != 0 {
            heights[r + 1].round()
        } else {
            ((heights[r] + heights[r + 1] - bb.ur.y - bb.ll.y) / 2.0).round()
        };
        places[idx] = Point::new(x, y);
        inc(row_major, &mut c, &mut r, nc, nr);
    }
    places
}

/// `putGraphs(ng, gs, root, pinfo)`: translations for each component.
pub fn put_graphs(g: &mut LGraph<'_>, gs: &mut [Sub], pinfo: &mut PackInfo) -> Option<Vec<Point>> {
    if gs.is_empty() {
        return None;
    }
    if pinfo.mode <= PackMode::Graph {
        return poly_graphs(g, gs, pinfo);
    }
    for s in gs.iter_mut() {
        compute_bb(g, s, false);
    }
    let bbs: Vec<BoundingBox> = gs.iter().map(|s| s.bb).collect();
    if pinfo.mode == PackMode::Array {
        if pinfo.flags & PK_USER_VALS != 0 {
            let v = g
                .gattr("sortv")
                .and_then(parse_leading_int)
                .filter(|&v| v >= 0)
                .unwrap_or(0) as u32;
            pinfo.vals = Some(vec![v; gs.len()]);
        }
        let pts = array_rects(&bbs, pinfo);
        pinfo.vals = None;
        return Some(pts);
    }
    None
}

/// `shiftGraphs(ng, gs, pp, root, doSplines)`.
pub fn shift_graphs(g: &mut LGraph<'_>, gs: &mut [Sub], pp: &[Point], do_splines: bool) {
    let root = g.root_sub();
    for (i, sub) in gs.iter_mut().enumerate() {
        let (dx, dy) = (pp[i].x, pp[i].y);
        let (fx, fy) = (dx / 72.0, dy / 72.0);
        for &n in &sub.nodes {
            let nd = &mut g.nodes[n];
            nd.pos[0] += fx;
            nd.pos[1] += fy;
            nd.coord.x += dx;
            nd.coord.y += dy;
            if let Some(l) = nd.xlabel.as_mut() {
                l.pos.x += dx;
                l.pos.y += dy;
            }
            if do_splines {
                let outs: Vec<usize> = g.outs(&root, n).collect();
                for e in outs {
                    shift_edge(g, e, dx, dy);
                }
            }
        }
        sub.bb.ll.x += dx;
        sub.bb.ur.x += dx;
        sub.bb.ll.y += dy;
        sub.bb.ur.y += dy;
        for c in sub.clusters.clone() {
            shift_cluster(g, c, dx, dy);
        }
    }
}

fn shift_cluster(g: &mut LGraph<'_>, c: usize, dx: f64, dy: f64) {
    let cl = &mut g.clusters[c];
    cl.bb.ll.x += dx;
    cl.bb.ur.x += dx;
    cl.bb.ll.y += dy;
    cl.bb.ur.y += dy;
    if let Some(l) = cl.label.as_mut().filter(|l| l.set) {
        l.pos.x += dx;
        l.pos.y += dy;
    }
    for k in g.clusters[c].children.clone() {
        shift_cluster(g, k, dx, dy);
    }
}

fn shift_edge(g: &mut LGraph<'_>, e: usize, dx: f64, dy: f64) {
    let ed = &mut g.edges[e];
    for l in [
        &mut ed.label,
        &mut ed.xlabel,
        &mut ed.head_label,
        &mut ed.tail_label,
    ]
    .into_iter()
    .flatten()
    {
        l.pos.x += dx;
        l.pos.y += dy;
    }
    let Some(spl) = ed.spl.as_mut() else { return };
    for b in spl.iter_mut() {
        for p in &mut b.list {
            p.x += dx;
            p.y += dy;
        }
        if b.sflag != 0 {
            b.sp.x += dx;
            b.sp.y += dy;
        }
        if b.eflag != 0 {
            b.ep.x += dx;
            b.ep.y += dy;
        }
    }
}

/// `packGraphs(ng, gs, root, info)`; false if packing failed.
pub fn pack_graphs(g: &mut LGraph<'_>, gs: &mut [Sub], info: &PackInfo) -> bool {
    let mut info = info.clone();
    let Some(pp) = put_graphs(g, gs, &mut info) else {
        return false;
    };
    shift_graphs(g, gs, &pp, info.do_splines);
    true
}

/// `putRects(ng, bbs, pinfo)`.
pub fn put_rects(bbs: &[BoundingBox], pinfo: &PackInfo) -> Option<Vec<Point>> {
    if bbs.is_empty() {
        return None;
    }
    match pinfo.mode {
        PackMode::Graph => poly_rects(bbs, pinfo),
        PackMode::Array => Some(array_rects(bbs, pinfo)),
        _ => None,
    }
}

/// `polyRects(ng, gs, pinfo)`.
fn poly_rects(bbs: &[BoundingBox], pinfo: &PackInfo) -> Option<Vec<Point>> {
    let step = compute_step(bbs, pinfo.margin);
    if step <= 0 {
        return None;
    }
    let infos: Vec<GInfo> = bbs
        .iter()
        .enumerate()
        .map(|(i, b)| gen_box(b, step, pinfo.margin, Point::default(), i))
        .collect();
    let mut sinfo: Vec<usize> = (0..bbs.len()).collect();
    qsort_by(&mut sinfo, |a, b| cmpf(a, b, &infos));
    let mut ps = PointSet::new();
    let mut places = vec![Point::default(); bbs.len()];
    for (i, &si) in sinfo.iter().enumerate() {
        let inf = &infos[si];
        places[inf.index] = place_graph(i, inf, &mut ps, step, pinfo.margin, bbs);
    }
    Some(places)
}

/// `packRects(ng, bbs, pinfo)`: translate the boxes in place; false on failure.
pub fn pack_rects(bbs: &mut [BoundingBox], pinfo: &PackInfo) -> bool {
    if bbs.len() <= 1 {
        return true;
    }
    let Some(pp) = put_rects(bbs, pinfo) else {
        return false;
    };
    for (b, p) in bbs.iter_mut().zip(pp) {
        b.ll = add(b.ll, p);
        b.ur = add(b.ur, p);
    }
    true
}
