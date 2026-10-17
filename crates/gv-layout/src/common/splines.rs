//! Edge spline installation shared by the neato family, ported from
//! `lib/common/splines.c` (`bezier_clip`, `shape_clip0`, `clip_and_install`,
//! `makeSelfEdge` and the `self*` loop routers, port labels, `edgeMidpoint`),
//! `lib/common/utils.c` (`Bezier`, `dotneato_closest`) and
//! `lib/common/emit.c` (`update_bb_bz`).

use super::arrows::{arrow_end_clip, arrow_flags, arrow_start_clip};
use super::geom::{MILLIPOINT, approxeqpt, dist, dist2, mid};
use super::ports::{BOTTOM, LEFT, RIGHT, TOP};
use super::shapes::InsideCtx;
use super::types::{Bezier, EdgeType, LGraph, TextLabel};
use super::utils::late_double;
use gv_core::{BoundingBox, Point};

/// `Bezier(V, t, Left, Right)`: de Casteljau evaluation and subdivision.
pub fn bezier_split(v: &[Point; 4], t: f64) -> (Point, [Point; 4], [Point; 4]) {
    let mut vt = [[Point::default(); 4]; 4];
    vt[0] = *v;
    for i in 1..=3 {
        for j in 0..=(3 - i) {
            // (1.0 - t) * a + t * b, contracted to fma(1 - t, a, t * b).
            vt[i][j].x = (1.0 - t).mul_add(vt[i - 1][j].x, t * vt[i - 1][j + 1].x);
            vt[i][j].y = (1.0 - t).mul_add(vt[i - 1][j].y, t * vt[i - 1][j + 1].y);
        }
    }
    let left = [vt[0][0], vt[1][0], vt[2][0], vt[3][0]];
    let right = [vt[3][0], vt[2][1], vt[1][2], vt[0][3]];
    (vt[3][0], left, right)
}

/// `Bezier(V, t, NULL, NULL)`.
pub fn bezier_pt(v: &[Point; 4], t: f64) -> Point {
    bezier_split(v, t).0
}

/// `bezier_clip`: binary search for where the curve crosses the boundary
/// described by `inside`. With `left_inside`, `sp[0]` is inside, else `sp[3]`.
pub fn bezier_clip(inside: &mut dyn FnMut(Point) -> bool, sp: &mut [Point; 4], left_inside: bool) {
    let mut best = [Point::default(); 4];
    let mut seg;
    let mut pt = if left_inside { sp[0] } else { sp[3] };
    let mut found = false;
    let mut low = 0.0f64;
    let mut high = 1.0f64;
    loop {
        let opt = pt;
        let t = (high + low) / 2.0;
        let (p, l, r) = bezier_split(sp, t);
        pt = p;
        seg = if left_inside { r } else { l };
        if inside(pt) {
            if left_inside {
                low = t;
            } else {
                high = t;
            }
            best = seg;
            found = true;
        } else if left_inside {
            high = t;
        } else {
            low = t;
        }
        if !((opt.x - pt.x).abs() > 0.5 || (opt.y - pt.y).abs() > 0.5) {
            break;
        }
    }
    *sp = if found { best } else { seg };
}

/// `shape_clip0`: clip `curve` (absolute coordinates) to node `n`.
fn shape_clip0(
    g: &LGraph<'_>,
    ctx: &mut InsideCtx,
    n: usize,
    curve: &mut [Point],
    left_inside: bool,
) {
    let c0 = g.nodes[n].coord;
    let mut c = [Point::default(); 4];
    for i in 0..4 {
        c[i] = Point::new(curve[i].x - c0.x, curve[i].y - c0.y);
    }
    let shape = &g.nodes[n].shape;
    bezier_clip(&mut |p| shape.inside(ctx, p), &mut c, left_inside);
    for i in 0..4 {
        curve[i] = Point::new(c[i].x + c0.x, c[i].y + c0.y);
    }
}

/// `shape_clip(n, curve)`: clips using `curve[0]` to decide which end is inside.
pub fn shape_clip(g: &LGraph<'_>, n: usize, curve: &mut [Point; 4]) {
    if g.nodes[n].shape.no_clip {
        return;
    }
    let mut ctx = InsideCtx::default();
    let c0 = g.nodes[n].coord;
    let left_inside = g.nodes[n]
        .shape
        .inside(&mut ctx, Point::new(curve[0].x - c0.x, curve[0].y - c0.y));
    shape_clip0(g, &mut ctx, n, curve, left_inside);
}

const HW: f64 = 2.0;
const SMALL: f64 = 0.000_000_000_1;

/// `ptToLine2`: squared distance from `p` to the line through `a` and `b`.
pub fn pt_to_line2(a: Point, b: Point, p: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let mut a2 = (p.y - a.y).mul_add(dx, -((p.x - a.x) * dy));
    a2 *= a2;
    if a2 < SMALL {
        return 0.0;
    }
    a2 / dx.mul_add(dx, dy * dy)
}

fn check_control_points(cp: &[Point; 4]) -> bool {
    let d1 = pt_to_line2(cp[0], cp[3], cp[1]);
    let d2 = pt_to_line2(cp[0], cp[3], cp[2]);
    d1 < HW * HW && d2 < HW * HW
}

/// `update_bb_bz`: grow `bb` to contain a Bézier segment.
pub fn update_bb_bz(bb: &mut BoundingBox, cp: &[Point; 4]) {
    let out = cp
        .iter()
        .any(|p| p.x > bb.ur.x || p.x < bb.ll.x || p.y > bb.ur.y || p.y < bb.ll.y);
    if !out {
        return;
    }
    if check_control_points(cp) {
        for p in cp {
            if p.x > bb.ur.x {
                bb.ur.x = p.x;
            } else if p.x < bb.ll.x {
                bb.ll.x = p.x;
            }
            if p.y > bb.ur.y {
                bb.ur.y = p.y;
            } else if p.y < bb.ll.y {
                bb.ll.y = p.y;
            }
        }
    } else {
        let (_, l, r) = bezier_split(cp, 0.5);
        update_bb_bz(bb, &l);
        update_bb_bz(bb, &r);
    }
}

/// `clip_and_install(fe, hn, ps, pn, info)` for edges without virtual
/// chains (`ED_to_orig` is NULL and ranks are equal, as in neato): clips the
/// raw spline `ps` to the end nodes and arrowheads, appends it to the edge's
/// splines and grows the root bounding box.
pub fn clip_and_install(g: &mut LGraph<'_>, e: usize, hn: usize, ps_in: &[Point]) {
    let mut ps = ps_in.to_vec();
    let pn = ps.len();
    let tn = g.edges[e].tail;
    let (clip_tail, clip_head, tbox, hbox) = if tn == g.edges[e].tail {
        let ed = &g.edges[e];
        (
            ed.tail_port.clip,
            ed.head_port.clip,
            ed.tail_port.bp,
            ed.head_port.bp,
        )
    } else {
        let ed = &g.edges[e];
        (
            ed.head_port.clip,
            ed.tail_port.clip,
            ed.head_port.bp,
            ed.tail_port.bp,
        )
    };
    let mut start = 0usize;
    if clip_tail && !g.nodes[tn].shape.no_clip {
        let mut ctx = InsideCtx {
            bp: tbox,
            ..InsideCtx::default()
        };
        let c = g.nodes[tn].coord;
        while start < pn - 4 {
            let p2 = Point::new(ps[start + 3].x - c.x, ps[start + 3].y - c.y);
            if !g.nodes[tn].shape.inside(&mut ctx, p2) {
                break;
            }
            start += 3;
        }
        shape_clip0(g, &mut ctx, tn, &mut ps[start..start + 4], true);
    }
    let mut end = pn - 4;
    if clip_head && !g.nodes[hn].shape.no_clip {
        let mut ctx = InsideCtx {
            bp: hbox,
            ..InsideCtx::default()
        };
        let c = g.nodes[hn].coord;
        while end > 0 {
            let p2 = Point::new(ps[end].x - c.x, ps[end].y - c.y);
            if !g.nodes[hn].shape.inside(&mut ctx, p2) {
                break;
            }
            end -= 3;
        }
        shape_clip0(g, &mut ctx, hn, &mut ps[end..end + 4], false);
    }
    while start < pn - 4 {
        if !approxeqpt(ps[start], ps[start + 3], MILLIPOINT) {
            break;
        }
        start += 3;
    }
    while end > 0 {
        if !approxeqpt(ps[end], ps[end + 3], MILLIPOINT) {
            break;
        }
        end -= 3;
    }
    let mut newspl = Bezier::default();
    // arrow_clip
    let attrs = &g.src.edges[e].attrs;
    let (sflag, eflag) = arrow_flags(attrs, g.is_directed());
    if sflag != 0 {
        start = arrow_start_clip(attrs, &mut ps, start, end, &mut newspl, sflag);
    }
    if eflag != 0 {
        end = arrow_end_clip(attrs, &mut ps, start, end, &mut newspl, eflag);
    }
    let mut list = Vec::with_capacity(end - start + 4);
    let mut i = start;
    while i < end + 4 {
        let mut cp = [Point::default(); 4];
        list.push(ps[i]);
        cp[0] = ps[i];
        i += 1;
        if i >= end + 4 {
            break;
        }
        list.push(ps[i]);
        cp[1] = ps[i];
        i += 1;
        list.push(ps[i]);
        cp[2] = ps[i];
        i += 1;
        cp[3] = ps[i];
        update_bb_bz(&mut g.bb, &cp);
    }
    newspl.list = list;
    g.edges[e].spl.get_or_insert_with(Vec::new).push(newspl);
}

/// `addLabelBB` / `updateBB`: grow `bb` by a positioned label.
pub fn add_label_bb(bb: &mut BoundingBox, lp: &TextLabel, flip: bool) {
    let (w, h) = if flip {
        (lp.dimen.y, lp.dimen.x)
    } else {
        (lp.dimen.x, lp.dimen.y)
    };
    let p = lp.pos;
    let min = p.x - w / 2.0;
    let max = p.x + w / 2.0;
    if min < bb.ll.x {
        bb.ll.x = min;
    }
    if max > bb.ur.x {
        bb.ur.x = max;
    }
    let min = p.y - h / 2.0;
    let max = p.y + h / 2.0;
    if min < bb.ll.y {
        bb.ll.y = min;
    }
    if max > bb.ur.y {
        bb.ur.y = max;
    }
}

fn convert_sides_to_points(tail_side: u8, head_side: u8) -> i32 {
    const VERTICES: [u8; 8] = [12, 4, 6, 2, 3, 1, 9, 8];
    let ti = VERTICES.iter().position(|&v| v == tail_side);
    let hi = VERTICES.iter().position(|&v| v == head_side);
    match (ti, hi) {
        (Some(t), Some(h)) => ((t + 1) * 10 + h + 1) as i32,
        _ => 0,
    }
}

fn label_width(lbl: &TextLabel) -> f64 {
    lbl.dimen.x
}

fn label_height(lbl: &TextLabel) -> f64 {
    lbl.dimen.y
}

fn self_right(g: &mut LGraph<'_>, edges: &[usize], stepx: f64, sizey: f64) {
    let e = edges[0];
    let n = g.edges[e].tail;
    let cnt = edges.len();
    let stepy = (sizey / 2.0 / cnt as f64).max(2.0);
    let np = g.nodes[n].coord;
    let tp = super::geom::add(g.edges[e].tail_port.p, np);
    let hp = super::geom::add(g.edges[e].head_port.p, np);
    let mut sgn = if tp.y >= hp.y { 1.0 } else { -1.0 };
    let mut dx = g.nodes[n].rw;
    let mut dy = 0.0;
    let pp = convert_sides_to_points(g.edges[e].tail_port.side, g.edges[e].head_port.side);
    if (pp == 32 || pp == 65) && tp.y == hp.y {
        sgn = -sgn;
    }
    let mut tx = dx.min(3.0 * (np.x + dx - tp.x));
    let mut hx = dx.min(3.0 * (np.x + dx - hp.x));
    for &e in edges {
        dx += stepx;
        tx += stepx;
        hx += stepx;
        dy += sgn * stepy;
        let points = [
            tp,
            Point::new(tp.x + tx / 3.0, tp.y + dy),
            Point::new(np.x + dx, tp.y + dy),
            Point::new(np.x + dx, (tp.y + hp.y) / 2.0),
            Point::new(np.x + dx, hp.y - dy),
            Point::new(hp.x + hx / 3.0, hp.y - dy),
            hp,
        ];
        if let Some(l) = g.edges[e].label.as_mut() {
            let width = label_width(l);
            l.pos = Point::new(np.x + dx + width / 2.0, np.y);
            l.set = true;
            if width > stepx {
                dx += width - stepx;
            }
        }
        let h = g.edges[e].head;
        clip_and_install(g, e, h, &points);
    }
}

fn self_left(g: &mut LGraph<'_>, edges: &[usize], stepx: f64, sizey: f64) {
    let e = edges[0];
    let n = g.edges[e].tail;
    let cnt = edges.len();
    let stepy = (sizey / 2.0 / cnt as f64).max(2.0);
    let np = g.nodes[n].coord;
    let tp = super::geom::add(g.edges[e].tail_port.p, np);
    let hp = super::geom::add(g.edges[e].head_port.p, np);
    let mut sgn = if tp.y >= hp.y { 1.0 } else { -1.0 };
    let mut dx = g.nodes[n].lw;
    let mut dy = 0.0;
    let pp = convert_sides_to_points(g.edges[e].tail_port.side, g.edges[e].head_port.side);
    if (pp == 12 || pp == 67) && tp.y == hp.y {
        sgn = -sgn;
    }
    let mut tx = dx.min(3.0 * (tp.x + dx - np.x));
    let mut hx = dx.min(3.0 * (hp.x + dx - np.x));
    for &e in edges {
        dx += stepx;
        tx += stepx;
        hx += stepx;
        dy += sgn * stepy;
        let points = [
            tp,
            Point::new(tp.x - tx / 3.0, tp.y + dy),
            Point::new(np.x - dx, tp.y + dy),
            Point::new(np.x - dx, (tp.y + hp.y) / 2.0),
            Point::new(np.x - dx, hp.y - dy),
            Point::new(hp.x - hx / 3.0, hp.y - dy),
            hp,
        ];
        if let Some(l) = g.edges[e].label.as_mut() {
            let width = label_width(l);
            l.pos = Point::new(np.x - dx - width / 2.0, np.y);
            l.set = true;
            if width > stepx {
                dx += width - stepx;
            }
        }
        let h = g.edges[e].head;
        clip_and_install(g, e, h, &points);
    }
}

fn self_top(g: &mut LGraph<'_>, edges: &[usize], sizex: f64, stepy: f64) {
    let e = edges[0];
    let n = g.edges[e].tail;
    let cnt = edges.len();
    let stepx = (sizex / 2.0 / cnt as f64).max(2.0);
    let np = g.nodes[n].coord;
    let tp = super::geom::add(g.edges[e].tail_port.p, np);
    let hp = super::geom::add(g.edges[e].head_port.p, np);
    let sgn = if tp.x >= hp.x { 1.0 } else { -1.0 };
    let (lw, rw) = (g.nodes[n].lw, g.nodes[n].rw);
    let mut dy = g.nodes[n].ht / 2.0;
    let mut dx = 0.0;
    match convert_sides_to_points(g.edges[e].tail_port.side, g.edges[e].head_port.side) {
        15 => dx = sgn * (rw - (hp.x - np.x) + stepx),
        38 => dx = sgn * (lw - (np.x - hp.x) + stepx),
        41 | 48 => dx = sgn * (rw - (tp.x - np.x) + stepx),
        14 | 37 | 47 | 51 | 57 | 58 => {
            dx = sgn * ((lw - (np.x - tp.x) + (rw - (hp.x - np.x))) / 3.0)
        }
        73 => dx = sgn * (lw - (np.x - tp.x) + stepx),
        83 => dx = sgn * (lw - (np.x - tp.x)),
        84 => dx = sgn * ((lw - (np.x - tp.x) + (rw - (hp.x - np.x))) / 2.0 + stepx),
        74 | 75 | 85 => {
            dx = sgn * ((lw - (np.x - tp.x) + (rw - (hp.x - np.x))) / 2.0 + 2.0 * stepx)
        }
        _ => {}
    }
    let mut ty = dy.min(3.0 * (np.y + dy - tp.y));
    let mut hy = dy.min(3.0 * (np.y + dy - hp.y));
    for &e in edges {
        dy += stepy;
        ty += stepy;
        hy += stepy;
        dx += sgn * stepx;
        let points = [
            tp,
            Point::new(tp.x + dx, tp.y + ty / 3.0),
            Point::new(tp.x + dx, np.y + dy),
            Point::new((tp.x + hp.x) / 2.0, np.y + dy),
            Point::new(hp.x - dx, np.y + dy),
            Point::new(hp.x - dx, hp.y + hy / 3.0),
            hp,
        ];
        if let Some(l) = g.edges[e].label.as_mut() {
            let height = label_height(l);
            l.pos = Point::new(np.x, np.y + dy + height / 2.0);
            l.set = true;
            if height > stepy {
                dy += height - stepy;
            }
        }
        let h = g.edges[e].head;
        clip_and_install(g, e, h, &points);
    }
}

fn self_bottom(g: &mut LGraph<'_>, edges: &[usize], sizex: f64, stepy: f64) {
    let e = edges[0];
    let n = g.edges[e].tail;
    let cnt = edges.len();
    let stepx = (sizex / 2.0 / cnt as f64).max(2.0);
    let np = g.nodes[n].coord;
    let tp = super::geom::add(g.edges[e].tail_port.p, np);
    let hp = super::geom::add(g.edges[e].head_port.p, np);
    let mut sgn = if tp.x >= hp.x { 1.0 } else { -1.0 };
    let mut dy = g.nodes[n].ht / 2.0;
    let mut dx = 0.0;
    if convert_sides_to_points(g.edges[e].tail_port.side, g.edges[e].head_port.side) == 67 {
        sgn = -sgn;
    }
    let mut ty = dy.min(3.0 * (tp.y + dy - np.y));
    let mut hy = dy.min(3.0 * (hp.y + dy - np.y));
    for &e in edges {
        dy += stepy;
        ty += stepy;
        hy += stepy;
        dx += sgn * stepx;
        let points = [
            tp,
            Point::new(tp.x + dx, tp.y - ty / 3.0),
            Point::new(tp.x + dx, np.y - dy),
            Point::new((tp.x + hp.x) / 2.0, np.y - dy),
            Point::new(hp.x - dx, np.y - dy),
            Point::new(hp.x - dx, hp.y - hy / 3.0),
            hp,
        ];
        if let Some(l) = g.edges[e].label.as_mut() {
            let height = label_height(l);
            l.pos = Point::new(np.x, np.y - dy - height / 2.0);
            l.set = true;
            if height > stepy {
                dy += height - stepy;
            }
        }
        let h = g.edges[e].head;
        clip_and_install(g, e, h, &points);
    }
}

/// `makeSelfEdge(edges, cnt, sizex, sizey, sinfo)`.
pub fn make_self_edge(g: &mut LGraph<'_>, edges: &[usize], sizex: f64, sizey: f64) {
    let e = edges[0];
    let tp = &g.edges[e].tail_port;
    let hp = &g.edges[e].head_port;
    let (ts, hs) = (tp.side, hp.side);
    if (!tp.defined && !hp.defined)
        || (ts & LEFT == 0 && hs & LEFT == 0 && (ts != hs || ts & (TOP | BOTTOM) == 0))
    {
        self_right(g, edges, sizex, sizey);
    } else if ts & LEFT != 0 || hs & LEFT != 0 {
        if ts & RIGHT != 0 || hs & RIGHT != 0 {
            self_top(g, edges, sizex, sizey);
        } else {
            self_left(g, edges, sizex, sizey);
        }
    } else if ts & TOP != 0 {
        self_top(g, edges, sizex, sizey);
    } else if ts & BOTTOM != 0 {
        self_bottom(g, edges, sizex, sizey);
    }
}

pub const PORT_LABEL_DISTANCE: f64 = 10.0;
pub const PORT_LABEL_ANGLE: f64 = -25.0;

/// `place_portlabel(e, head_p)`; returns whether the label was placed.
fn place_portlabel(g: &mut LGraph<'_>, e: usize, head_p: bool) -> bool {
    let attrs = &g.src.edges[e].attrs;
    let la = attrs.get("labelangle").filter(|s| !s.is_empty());
    let ld = attrs.get("labeldistance").filter(|s| !s.is_empty());
    if la.is_none() && ld.is_none() {
        return false;
    }
    let Some(spl) = g.edges[e].spl.as_ref() else {
        return false;
    };
    let (pe, pf) = if !head_p {
        let bez = &spl[0];
        if bez.sflag != 0 {
            (bez.sp, bez.list[0])
        } else {
            let c = [bez.list[0], bez.list[1], bez.list[2], bez.list[3]];
            (bez.list[0], bezier_pt(&c, 0.1))
        }
    } else {
        let bez = &spl[spl.len() - 1];
        let n = bez.list.len();
        if bez.eflag != 0 {
            (bez.ep, bez.list[n - 1])
        } else {
            let c = [
                bez.list[n - 4],
                bez.list[n - 3],
                bez.list[n - 2],
                bez.list[n - 1],
            ];
            (bez.list[n - 1], bezier_pt(&c, 0.9))
        }
    };
    let angle = (pf.y - pe.y).atan2(pf.x - pe.x)
        + late_double(attrs, "labelangle", PORT_LABEL_ANGLE, -180.0).to_radians();
    let d = PORT_LABEL_DISTANCE * late_double(attrs, "labeldistance", 1.0, 0.0);
    let pos = Point::new(d.mul_add(angle.cos(), pe.x), d.mul_add(angle.sin(), pe.y));
    let l = if head_p {
        g.edges[e].head_label.as_mut()
    } else {
        g.edges[e].tail_label.as_mut()
    };
    if let Some(l) = l {
        l.pos = pos;
        l.set = true;
    }
    true
}

/// `makePortLabels(e)`: place head/tail labels when `labelangle` or
/// `labeldistance` is declared, growing the root bounding box.
pub fn make_port_labels(g: &mut LGraph<'_>, e: usize) {
    if !g.has_labelangle && !g.has_labeldistance {
        return;
    }
    if g.edges[e].head_label.as_ref().is_some_and(|l| !l.set) && place_portlabel(g, e, true) {
        let l = g.edges[e].head_label.clone().unwrap();
        add_label_bb(&mut g.bb, &l, false);
    }
    if g.edges[e].tail_label.as_ref().is_some_and(|l| !l.set) && place_portlabel(g, e, false) {
        let l = g.edges[e].tail_label.clone().unwrap();
        add_label_bb(&mut g.bb, &l, false);
    }
}

/// `addEdgeLabels(e)`.
pub fn add_edge_labels(g: &mut LGraph<'_>, e: usize) {
    make_port_labels(g, e);
}

fn end_points(spl: &[Bezier]) -> (Point, Point) {
    let b = &spl[0];
    let p = if b.sflag != 0 { b.sp } else { b.list[0] };
    let b = &spl[spl.len() - 1];
    let q = if b.eflag != 0 {
        b.ep
    } else {
        b.list[b.list.len() - 1]
    };
    (p, q)
}

fn polyline_midpoint(spl: &[Bezier]) -> Point {
    let mut d = 0.0;
    for b in spl {
        let mut k = 3;
        while k < b.list.len() {
            d += dist(b.list[k - 3], b.list[k]);
            k += 3;
        }
    }
    let mut dd = d / 2.0;
    for b in spl {
        let mut k = 3;
        while k < b.list.len() {
            let pf = b.list[k - 3];
            let qf = b.list[k];
            let seg = dist(pf, qf);
            if seg >= dd {
                return Point::new(
                    qf.x.mul_add(dd, pf.x * (seg - dd)) / seg,
                    qf.y.mul_add(dd, pf.y * (seg - dd)) / seg,
                );
            }
            dd -= seg;
            k += 3;
        }
    }
    spl[0].list[0]
}

/// `dotneato_closest(spl, pt)`.
pub fn dotneato_closest(spl: &[Bezier], pt: Point) -> Point {
    let mut besti = usize::MAX;
    let mut bestj = usize::MAX;
    let mut bestdist2 = f64::MAX;
    for (i, b) in spl.iter().enumerate() {
        for (j, p) in b.list.iter().enumerate() {
            let d2 = dist2(*p, pt);
            if bestj == usize::MAX || d2 < bestdist2 {
                besti = i;
                bestj = j;
                bestdist2 = d2;
            }
        }
    }
    let bz = &spl[besti];
    if bestj == bz.list.len() - 1 {
        bestj -= 1;
    }
    let j = 3 * (bestj / 3);
    let c = [bz.list[j], bz.list[j + 1], bz.list[j + 2], bz.list[j + 3]];
    let mut low = 0.0f64;
    let mut high = 1.0f64;
    let mut dlow2 = dist2(c[0], pt);
    let mut dhigh2 = dist2(c[3], pt);
    loop {
        let t = (low + high) / 2.0;
        let pt2 = bezier_pt(&c, t);
        if (dlow2 - dhigh2).abs() < 1.0 || (high - low).abs() < 0.00001 {
            return pt2;
        }
        if dlow2 < dhigh2 {
            high = t;
            dhigh2 = dist2(pt2, pt);
        } else {
            low = t;
            dlow2 = dist2(pt2, pt);
        }
    }
}

/// `edgeMidpoint(g, e)`.
pub fn edge_midpoint(et: EdgeType, spl: &[Bezier]) -> Point {
    let (p, q) = end_points(spl);
    if approxeqpt(p, q, MILLIPOINT) {
        p
    } else if et == EdgeType::Spline || et == EdgeType::Curved {
        dotneato_closest(spl, mid(p, q))
    } else {
        polyline_midpoint(spl)
    }
}
