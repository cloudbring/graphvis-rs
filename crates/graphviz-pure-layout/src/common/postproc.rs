//! Layout post-processing, ported from `lib/common/postproc.c`
//! (`gv_postprocess`, `translate_drawing`, `translate_bb`, `place_graph_label`,
//! `place_root_label`) for engines without `rankdir` (the neato family).
//! External label placement (`addXLabels`) lives in [`super::xlabels`].

use super::init::pad;
use super::types::{LABEL_AT_LEFT, LABEL_AT_RIGHT, LABEL_AT_TOP, LGraph};
use graphviz_pure_core::Point;

/// For engines that return before `dotneato_postprocess` on a graph without nodes
/// (`twopi_layout`, `circo_layout`, `patchwork_layout`): the graph and cluster labels
/// keep their zeroed position, which C draws and prints at the origin.
pub(crate) fn labels_at_origin(graph: &mut graphviz_pure_core::Graph) {
    graph.layout.label_pos = Some(Point::default());
    for sg in &mut graph.subgraphs {
        sg.layout.label_pos = Some(Point::default());
    }
}

/// `place_graph_label(g)`: positions unset cluster labels.
pub fn place_graph_label(g: &mut LGraph<'_>) {
    fn rec(g: &mut LGraph<'_>, c: usize) {
        let cl = &mut g.clusters[c];
        let bb = cl.bb;
        let lp = cl.label_pos;
        if let Some(l) = cl.label.as_mut()
            && !l.set
        {
            let (d, y) = if lp & LABEL_AT_TOP != 0 {
                let d = cl.border[2];
                (d, bb.ur.y - d.y / 2.0)
            } else {
                let d = cl.border[0];
                (d, bb.ll.y + d.y / 2.0)
            };
            let x = if lp & LABEL_AT_RIGHT != 0 {
                bb.ur.x - d.x / 2.0
            } else if lp & LABEL_AT_LEFT != 0 {
                bb.ll.x + d.x / 2.0
            } else {
                (bb.ll.x + bb.ur.x) / 2.0
            };
            l.pos = Point::new(x, y);
            l.set = true;
        }
        for k in g.clusters[c].children.clone() {
            rec(g, k);
        }
    }
    for c in g.root_clusters.clone() {
        rec(g, c);
    }
}

fn map_point(p: Point, off: Point) -> Point {
    Point::new(p.x - off.x, p.y - off.y)
}

/// `translate_drawing(g)` with `Rankdir == 0`.
fn translate_drawing(g: &mut LGraph<'_>, off: Point) {
    if off.x == 0.0 && off.y == 0.0 {
        return;
    }
    for n in &mut g.nodes {
        n.coord = map_point(n.coord, off);
        if let Some(l) = n.xlabel.as_mut() {
            l.pos = map_point(l.pos, off);
        }
    }
    for e in &mut g.edges {
        let Some(spl) = e.spl.as_mut() else { continue };
        for b in spl.iter_mut() {
            for p in &mut b.list {
                *p = map_point(*p, off);
            }
            if b.sflag != 0 {
                b.sp = map_point(b.sp, off);
            }
            if b.eflag != 0 {
                b.ep = map_point(b.ep, off);
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
            l.pos = map_point(l.pos, off);
        }
    }
    let tb = |b: &mut graphviz_pure_core::BoundingBox| {
        b.ll = map_point(b.ll, off);
        b.ur = map_point(b.ur, off);
    };
    tb(&mut g.bb);
    if let Some(l) = g.label.as_mut() {
        l.pos = map_point(l.pos, off);
    }
    for c in &mut g.clusters {
        tb(&mut c.bb);
        if let Some(l) = c.label.as_mut() {
            l.pos = map_point(l.pos, off);
        }
    }
}

/// `gv_postprocess(g, allowTranslation)` for `rankdir=TB` without flipping.
pub fn gv_postprocess(g: &mut LGraph<'_>, allow_translation: bool) {
    place_graph_label(g);
    super::xlabels::add_xlabels(g);
    let mut dimen = Point::default();
    if let Some(l) = g.label.as_ref().filter(|l| !l.set) {
        dimen = pad(l.dimen);
        if g.label_pos & LABEL_AT_TOP != 0 {
            g.bb.ur.y += dimen.y;
        } else {
            g.bb.ll.y -= dimen.y;
        }
        if dimen.x > g.bb.ur.x - g.bb.ll.x {
            let diff = (dimen.x - (g.bb.ur.x - g.bb.ll.x)) / 2.0;
            g.bb.ll.x -= diff;
            g.bb.ur.x += diff;
        }
    }
    if allow_translation {
        let off = g.bb.ll;
        translate_drawing(g, off);
    }
    let lp = g.label_pos;
    let bb = g.bb;
    if let Some(l) = g.label.as_mut().filter(|l| !l.set) {
        let x = if lp & LABEL_AT_RIGHT != 0 {
            bb.ur.x - dimen.x / 2.0
        } else if lp & LABEL_AT_LEFT != 0 {
            bb.ll.x + dimen.x / 2.0
        } else {
            (bb.ll.x + bb.ur.x) / 2.0
        };
        let y = if lp & LABEL_AT_TOP != 0 {
            bb.ur.y - dimen.y / 2.0
        } else {
            bb.ll.y + dimen.y / 2.0
        };
        l.pos = Point::new(x, y);
        l.set = true;
    }
}
