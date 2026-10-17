//! Edge port resolution: `chkPort`, `poly_port`, `record_port` and
//! `compassPort` from `lib/common/shapes.c` and `lib/common/utils.c`.

use super::shapes::{InsideCtx, InsideFn};
use super::splines::bezier_clip;
use super::types::{LGraph, PortInfo};
use gv_core::{BoundingBox, Point};

pub const BOTTOM: u8 = 1 << 0;
pub const RIGHT: u8 = 1 << 1;
pub const TOP: u8 = 1 << 2;
pub const LEFT: u8 = 1 << 3;

/// `compassPoint`: where the ray from the centre towards `(x, y)` leaves the shape.
fn compass_point(g: &LGraph<'_>, n: usize, y: f64, x: f64) -> Point {
    let shape = &g.nodes[n].shape;
    let p = Point::new(x, y);
    let mut curve = [Point::new(0.0, 0.0), Point::new(0.0, 0.0), p, p];
    let mut ctx = InsideCtx::default();
    bezier_clip(&mut |q| shape.inside(&mut ctx, q), &mut curve, true);
    curve[0]
}

/// `compassPort`; returns the port and whether the compass was unrecognised.
fn compass_port(
    g: &LGraph<'_>,
    n: usize,
    bp: Option<BoundingBox>,
    compass: &str,
    sides: u8,
    use_ictxt: bool,
) -> (PortInfo, bool) {
    let nd = &g.nodes[n];
    let (b, mut p, mut defined) = match bp {
        Some(b) => (
            b,
            Point::new((b.ll.x + b.ur.x) / 2.0, (b.ll.y + b.ur.y) / 2.0),
            true,
        ),
        None => (
            BoundingBox::new(-nd.lw, -nd.ht / 2.0, nd.lw, nd.ht / 2.0),
            Point::new(0.0, 0.0),
            false,
        ),
    };
    let maxv = b.ur.x.max(b.ur.y) * 4.0;
    let ctr = p;
    let mut rv = false;
    let mut constrain = false;
    let mut clip = true;
    let mut side = 0u8;
    let mut dyna = false;
    let cp = |y: f64, x: f64| compass_point(g, n, y, x);
    let c: Vec<char> = compass.chars().collect();
    if !c.is_empty() {
        let next = c.get(1).copied();
        match c[0] {
            'e' => {
                if next.is_some() {
                    rv = true;
                } else {
                    if use_ictxt {
                        p = cp(ctr.y, maxv)
                    } else {
                        p.x = b.ur.x
                    }
                    constrain = true;
                    defined = true;
                    clip = false;
                    side = sides & RIGHT;
                }
            }
            's' => {
                p.y = b.ll.y;
                constrain = true;
                clip = false;
                match next {
                    None => {
                        defined = true;
                        if use_ictxt {
                            p = cp(-maxv, ctr.x)
                        } else {
                            p.x = ctr.x
                        }
                        side = sides & BOTTOM;
                    }
                    Some('e') => {
                        defined = true;
                        if use_ictxt {
                            p = cp(-maxv, maxv)
                        } else {
                            p.x = b.ur.x
                        }
                        side = sides & (BOTTOM | RIGHT);
                    }
                    Some('w') => {
                        defined = true;
                        if use_ictxt {
                            p = cp(-maxv, -maxv)
                        } else {
                            p.x = b.ll.x
                        }
                        side = sides & (BOTTOM | LEFT);
                    }
                    _ => {
                        p.y = ctr.y;
                        constrain = false;
                        clip = true;
                        rv = true;
                    }
                }
            }
            'w' => {
                if next.is_some() {
                    rv = true;
                } else {
                    if use_ictxt {
                        p = cp(ctr.y, -maxv)
                    } else {
                        p.x = b.ll.x
                    }
                    constrain = true;
                    defined = true;
                    clip = false;
                    side = sides & LEFT;
                }
            }
            'n' => {
                p.y = b.ur.y;
                constrain = true;
                clip = false;
                match next {
                    None => {
                        defined = true;
                        if use_ictxt {
                            p = cp(maxv, ctr.x)
                        } else {
                            p.x = ctr.x
                        }
                        side = sides & TOP;
                    }
                    Some('e') => {
                        defined = true;
                        if use_ictxt {
                            p = cp(maxv, maxv)
                        } else {
                            p.x = b.ur.x
                        }
                        side = sides & (TOP | RIGHT);
                    }
                    Some('w') => {
                        defined = true;
                        if use_ictxt {
                            p = cp(maxv, -maxv)
                        } else {
                            p.x = b.ll.x
                        }
                        side = sides & (TOP | LEFT);
                    }
                    _ => {
                        p.y = ctr.y;
                        constrain = false;
                        clip = true;
                        rv = true;
                    }
                }
            }
            '_' => {
                dyna = true;
                side = sides;
            }
            'c' => {}
            _ => rv = true,
        }
    }
    (
        PortInfo {
            p,
            defined,
            constrained: constrain,
            clip,
            bp,
            side,
            dyna,
        },
        rv,
    )
}

/// `portToTbl` / `portToCell` (`htmltable.c`): the box and sides of the
/// table or cell whose `PORT` matches (case-insensitively).
fn port_to_tbl(tbl: &gv_geom::html_label::HtmlTbl, id: &str) -> Option<(BoundingBox, u8)> {
    if tbl
        .data
        .port
        .as_deref()
        .is_some_and(|p| p.eq_ignore_ascii_case(id))
    {
        return Some((tbl.data.bx, tbl.data.sides));
    }
    for cell in &tbl.cells {
        if cell
            .data
            .port
            .as_deref()
            .is_some_and(|p| p.eq_ignore_ascii_case(id))
        {
            return Some((cell.data.bx, cell.data.sides));
        }
        if let gv_geom::html_label::HtmlContent::Table(t) = &cell.child
            && let Some(r) = port_to_tbl(t, id)
        {
            return Some(r);
        }
    }
    None
}

/// `chkPort(portfn, n, s)` for port string `s` (`name[:compass]`).
fn resolve_port(g: &LGraph<'_>, n: usize, s: &str) -> PortInfo {
    if s.is_empty() {
        return PortInfo {
            clip: true,
            ..PortInfo::default()
        };
    }
    let (portname, compass) = match s.split_once(':') {
        Some((a, b)) => (a, Some(b)),
        None => (s, None),
    };
    let src = &g.src.nodes[n];
    let nd = &g.nodes[n];
    let env = gv_geom::NodeSizeEnv::from_graph_attrs(&g.src.name, &g.src.graph_attrs);
    let geom = gv_geom::node_geometry(src, &env);
    let all = BOTTOM | RIGHT | TOP | LEFT;
    match nd.shape.inside {
        InsideFn::Record => {
            let field = geom
                .record
                .as_ref()
                .and_then(|rec| rec.find_port(portname).map(|f| (f.bbox, rec.bbox)));
            match field {
                Some((fb, rb)) => {
                    // f->sides: the sides of the record box the field lies on.
                    let eq = |a: f64, b: f64| (a - b).abs() < 1e-6;
                    let mut sides = 0u8;
                    if eq(fb.ll.y, rb.ll.y) {
                        sides |= BOTTOM;
                    }
                    if eq(fb.ur.x, rb.ur.x) {
                        sides |= RIGHT;
                    }
                    if eq(fb.ur.y, rb.ur.y) {
                        sides |= TOP;
                    }
                    if eq(fb.ll.x, rb.ll.x) {
                        sides |= LEFT;
                    }
                    compass_port(g, n, Some(fb), compass.unwrap_or("_"), sides, false).0
                }
                None => {
                    let whole = BoundingBox::new(-nd.lw, -nd.ht / 2.0, nd.rw, nd.ht / 2.0);
                    compass_port(g, n, Some(whole), portname, all, false).0
                }
            }
        }
        _ => {
            if let Some(gv_geom::html_label::HtmlLabel::Table(tbl)) =
                geom.label.html_label.as_deref()
                && let Some((bp, sides)) = port_to_tbl(tbl, portname)
            {
                return compass_port(g, n, Some(bp), compass.unwrap_or("_"), sides, false).0;
            }
            let is_box = matches!(
                src.attrs.get("shape").map(String::as_str),
                Some("box" | "rect" | "rectangle")
            );
            compass_port(g, n, None, portname, all, !is_box).0
        }
    }
}

fn port_string(p: &gv_core::Port) -> String {
    match (&p.name, &p.compass) {
        (Some(a), Some(b)) => format!("{a}:{b}"),
        (Some(a), None) => a.clone(),
        (None, Some(b)) => b.clone(),
        (None, None) => String::new(),
    }
}

/// Resolves `tailport`/`headport` of every edge, keeping `*clip=false`.
pub fn resolve_all_ports(g: &mut LGraph<'_>) {
    for ei in 0..g.edges.len() {
        let src = &g.src.edges[ei];
        let (t, h) = (g.edges[ei].tail, g.edges[ei].head);
        let ts = port_string(&src.tail_port);
        let hs = port_string(&src.head_port);
        let tclip = g.edges[ei].tail_port.clip;
        let hclip = g.edges[ei].head_port.clip;
        let mut tp = resolve_port(g, t, &ts);
        let mut hp = resolve_port(g, h, &hs);
        if !tclip {
            tp.clip = false;
        }
        if !hclip {
            hp.clip = false;
        }
        g.edges[ei].tail_port = tp;
        g.edges[ei].head_port = hp;
    }
}

/// `closestSide(n, other, oldport)`: compass of the side of the port box
/// nearest to `other`, or `None` for the centre.
fn closest_side(g: &LGraph<'_>, n: usize, other: usize, old: &PortInfo) -> Option<&'static str> {
    const SIDE_PORT: [&str; 4] = ["s", "e", "n", "w"];
    let pt = g.nodes[n].coord;
    let opt = g.nodes[other].coord;
    let sides = old.side;
    if sides == 0 || sides == (TOP | BOTTOM | LEFT | RIGHT) {
        return None;
    }
    let nd = &g.nodes[n];
    let b = old
        .bp
        .unwrap_or(BoundingBox::new(-nd.lw, -nd.ht / 2.0, nd.lw, nd.ht / 2.0));
    let mut rv = None;
    let mut mind = 0.0;
    for i in 0..4 {
        if sides & (1 << i) == 0 {
            continue;
        }
        let p = match i {
            0 => Point::new((b.ll.x + b.ur.x) / 2.0, b.ll.y),
            1 => Point::new(b.ur.x, (b.ll.y + b.ur.y) / 2.0),
            2 => Point::new((b.ll.x + b.ur.x) / 2.0, b.ur.y),
            _ => Point::new(b.ll.x, (b.ll.y + b.ur.y) / 2.0),
        };
        let p = Point::new(p.x + pt.x, p.y + pt.y);
        let d = super::geom::dist2(p, opt);
        if rv.is_none() || d < mind {
            mind = d;
            rv = Some(SIDE_PORT[i]);
        }
    }
    rv
}

/// `resolvePorts(e)`: pick the side of dynamic (`_`) ports facing the other end.
pub fn resolve_ports(g: &mut LGraph<'_>, e: usize) {
    let (t, h) = (g.edges[e].tail, g.edges[e].head);
    if g.edges[e].tail_port.dyna {
        let old = g.edges[e].tail_port.clone();
        let c = closest_side(g, t, h, &old).unwrap_or("");
        let mut np = compass_port(g, t, old.bp, c, old.side, false).0;
        if !old.clip {
            np.clip = false;
        }
        g.edges[e].tail_port = np;
    }
    if g.edges[e].head_port.dyna {
        let old = g.edges[e].head_port.clone();
        let c = closest_side(g, h, t, &old).unwrap_or("");
        let mut np = compass_port(g, h, old.bp, c, old.side, false).0;
        if !old.clip {
            np.clip = false;
        }
        g.edges[e].head_port = np;
    }
}
