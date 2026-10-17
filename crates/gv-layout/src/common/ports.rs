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
            '_' => side = sides,
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
        },
        rv,
    )
}

fn is_html(label: &str) -> bool {
    let t = label.trim();
    t.starts_with('<') && t.ends_with('>')
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
    let mut node = src.clone();
    node.width = nd.width;
    node.height = nd.height;
    node.pos = Point::default();
    let label = src.attrs.get("label").map(String::as_str).unwrap_or("\\N");
    match nd.shape.inside {
        InsideFn::Record => {
            let field = gv_geom::record_layout(&node, &g.src.name, "TB")
                .and_then(|rec| rec.find_port(portname).map(|f| f.bbox));
            match field {
                Some(fb) => {
                    compass_port(
                        g,
                        n,
                        Some(fb),
                        compass.unwrap_or("_"),
                        BOTTOM | RIGHT | TOP | LEFT,
                        false,
                    )
                    .0
                }
                None => {
                    let whole = BoundingBox::new(-nd.lw, -nd.ht / 2.0, nd.rw, nd.ht / 2.0);
                    compass_port(
                        g,
                        n,
                        Some(whole),
                        portname,
                        BOTTOM | RIGHT | TOP | LEFT,
                        false,
                    )
                    .0
                }
            }
        }
        _ => {
            let sides = BOTTOM | RIGHT | TOP | LEFT;
            if is_html(label)
                && let Some(tbl) = gv_geom::html_table_layout(&node, &g.src.name)
                && let Some(pb) = tbl.find_port_box(portname)
            {
                return compass_port(g, n, Some(pb), compass.unwrap_or("_"), sides, false).0;
            }
            let is_box = matches!(
                src.attrs.get("shape").map(String::as_str),
                Some("box" | "rect" | "rectangle")
            );
            compass_port(g, n, None, portname, sides, !is_box).0
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
