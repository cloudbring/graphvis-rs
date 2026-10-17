//! [`SplineGraph`] for dot: exposes the dot arenas to the shared spline code
//! of `lib/common` (`clip_and_install`, `beginpath`/`endpath`, self loops,
//! port labels), plus the dot sides of `resolvePort` and `record_path`
//! (lib/common/shapes.c).

use super::Dot;
use super::types::*;
use crate::common::arrows::arrow_flags;
use crate::common::shapes::NodeShape;
use crate::common::splines::{LabelKind, LabelView, SplineGraph};
use gv_core::{AttrMap, GraphKind, Point};

static EMPTY_ATTRS: std::sync::LazyLock<AttrMap> = std::sync::LazyLock::new(AttrMap::new);

impl Dot<'_> {
    fn label_index(&self, e: E, k: LabelKind) -> Option<L> {
        let ed = &self.edges[e];
        match k {
            LabelKind::Main => ed.label,
            LabelKind::Head => ed.head_label,
            LabelKind::Tail => ed.tail_label,
        }
    }

    /// Whether some edge declares attribute `k` (`agattr(g, AGEDGE, k, NULL)`).
    pub fn edge_attr_declared(&self, k: &str) -> bool {
        self.input.edge_defaults.contains_key(k)
            || self.input.edges.iter().any(|e| e.attrs.contains_key(k))
    }

    /// `closestSide(n, other, oldport)` (shapes.c).
    fn closest_side(&self, n: N, other: N, oldport: &Port) -> &'static str {
        const SIDE_PORT: [&str; 4] = ["s", "e", "n", "w"];
        let rkd = self.graphs[0].rankdir;
        let cvt = |p: Point| match rkd {
            RANKDIR_BT => Point::new(p.x, -p.y),
            RANKDIR_LR => Point::new(-p.y, p.x),
            RANKDIR_RL => Point::new(p.y, p.x),
            _ => p,
        };
        let pt = cvt(self.nodes[n].coord);
        let opt = cvt(self.nodes[other].coord);
        let sides = oldport.side;
        if sides == 0 || sides == (TOP | BOTTOM | LEFT | RIGHT) {
            return "";
        }
        let b = if let Some(bp) = oldport.bp {
            bp
        } else {
            let nd = &self.nodes[n];
            if self.graphs[0].flip() {
                BoxF::new(-nd.ht / 2.0, -nd.lw, nd.ht / 2.0, nd.lw)
            } else {
                BoxF::new(-nd.lw, -nd.ht / 2.0, nd.lw, nd.ht / 2.0)
            }
        };
        let mut rv: Option<&'static str> = None;
        let mut mind = 0.0;
        for (i, name) in SIDE_PORT.iter().enumerate() {
            if sides & (1 << i) == 0 {
                continue;
            }
            let mut p = match i {
                BOTTOM_IX => Point::new((b.ll.x + b.ur.x) / 2.0, b.ll.y),
                RIGHT_IX => Point::new(b.ur.x, (b.ll.y + b.ur.y) / 2.0),
                TOP_IX => Point::new((b.ll.x + b.ur.x) / 2.0, b.ur.y),
                _ => Point::new(b.ll.x, (b.ll.y + b.ur.y) / 2.0),
            };
            p.x += pt.x;
            p.y += pt.y;
            let d = (p.x - opt.x) * (p.x - opt.x) + (p.y - opt.y) * (p.y - opt.y);
            if rv.is_none() || d < mind {
                mind = d;
                rv = Some(name);
            }
        }
        rv.unwrap_or("")
    }
}

impl SplineGraph for Dot<'_> {
    fn tail(&self, e: usize) -> usize {
        self.edges[e].tail
    }
    fn head(&self, e: usize) -> usize {
        self.edges[e].head
    }
    fn coord(&self, n: usize) -> Point {
        self.nodes[n].coord
    }
    fn lw(&self, n: usize) -> f64 {
        self.nodes[n].lw
    }
    fn rw(&self, n: usize) -> f64 {
        self.nodes[n].rw
    }
    fn ht(&self, n: usize) -> f64 {
        self.nodes[n].ht
    }
    fn shape(&self, n: usize) -> Option<&NodeShape> {
        self.nodes[n].shape.as_ref()
    }
    fn rankdir(&self) -> i32 {
        self.graphs[0].rankdir
    }
    fn flip(&self) -> bool {
        self.graphs[0].flip()
    }
    fn ranksep(&self) -> f64 {
        f64::from(self.graphs[0].ranksep)
    }
    fn rank(&self, n: usize) -> i32 {
        self.nodes[n].rank
    }
    fn order(&self, n: usize) -> i32 {
        self.nodes[n].order
    }
    fn is_normal_node(&self, n: usize) -> bool {
        self.nodes[n].node_type == NORMAL
    }
    fn to_orig(&self, e: usize) -> Option<usize> {
        self.edges[e].to_orig
    }
    fn is_normal_edge(&self, e: usize) -> bool {
        self.edges[e].edge_type == NORMAL
    }
    fn is_ignored_edge(&self, e: usize) -> bool {
        self.edges[e].edge_type == IGNORED
    }
    fn tail_port(&self, e: usize) -> &Port {
        &self.edges[e].tail_port
    }
    fn head_port(&self, e: usize) -> &Port {
        &self.edges[e].head_port
    }
    fn tail_port_mut(&mut self, e: usize) -> &mut Port {
        &mut self.edges[e].tail_port
    }
    fn head_port_mut(&mut self, e: usize) -> &mut Port {
        &mut self.edges[e].head_port
    }
    fn edge_attrs(&self, e: usize) -> &AttrMap {
        match self.edges[e].real {
            Some(r) => &self.input.edges[r].attrs,
            None => &EMPTY_ATTRS,
        }
    }
    /// `arrow_flags(e)`, including the `ED_conc_opp_flag` merge of the
    /// opposing edge's arrowheads.
    fn arrow_flags(&self, e: usize) -> (u32, u32) {
        let directed = self.input.kind == GraphKind::Directed;
        let (mut s, mut f) = arrow_flags(self.edge_attrs(e), directed);
        if self.edges[e].conc_opp_flag {
            let (t, h) = (self.edges[e].tail, self.edges[e].head);
            if let Some(o) = self.cg_find_edge(0, h, t) {
                let (s0, e0) = arrow_flags(self.edge_attrs(o), directed);
                f |= s0;
                s |= e0;
            }
        }
        (s, f)
    }
    fn swap_ends(&self, e: usize) -> bool {
        self.swap_ends_p(e)
    }
    fn spline_merge(&self, n: usize) -> bool {
        self.spline_merge_p(n)
    }
    fn fast_in(&self, n: usize) -> &[usize] {
        &self.nodes[n].in_
    }
    fn fast_out(&self, n: usize) -> &[usize] {
        &self.nodes[n].out
    }
    fn spl(&self, e: usize) -> Option<&Vec<Bezier>> {
        self.edges[e].spl.as_ref()
    }
    fn spl_mut(&mut self, e: usize) -> &mut Option<Vec<Bezier>> {
        &mut self.edges[e].spl
    }
    fn bb_mut(&mut self) -> &mut BoxF {
        &mut self.graphs[0].bb
    }
    fn label(&self, e: usize, k: LabelKind) -> Option<LabelView> {
        let l = &self.labels[self.label_index(e, k)?];
        Some(LabelView {
            dimen: l.dimen,
            pos: l.pos,
            set: l.set,
        })
    }
    fn set_label_pos(&mut self, e: usize, k: LabelKind, pos: Point) {
        if let Some(l) = self.label_index(e, k) {
            self.labels[l].pos = pos;
            self.labels[l].set = true;
        }
    }
    fn has_port_label_attrs(&self) -> bool {
        self.edge_attr_declared("labelangle") || self.edge_attr_declared("labeldistance")
    }
    /// `resolvePort(n, other, oldport)`.
    fn resolve_port(&self, n: usize, other: usize, old: &Port) -> Port {
        let compass = self.closest_side(n, other, old);
        let mut rv = Port {
            name: old.name.clone(),
            ..Port::default()
        };
        self.compass_port(n, old.bp, &mut rv, compass, old.side, false);
        rv
    }
    /// `record_path(n, prt, side, rv, &k)`; polygons (`poly_path`) have none.
    fn pboxfn(&self, n: usize, prt: &Port, side: u8) -> Option<(u8, Vec<BoxF>)> {
        let nd = &self.nodes[n];
        if !nd.is_record || nd.rec_fields.is_empty() {
            return None;
        }
        if !prt.defined {
            return Some((0, Vec::new()));
        }
        let p = prt.p;
        let flip = self.graphs[0].flip();
        for f in &nd.rec_fields {
            let (ls, rs) = if flip {
                (f.ll.y, f.ur.y)
            } else {
                (f.ll.x, f.ur.x)
            };
            if ls <= p.x && p.x <= rs {
                let c = nd.coord;
                let mut b = if flip {
                    BoxF::new(f.ll.y + c.x, f.ll.x + c.y, f.ur.y + c.x, f.ur.x + c.y)
                } else {
                    BoxF::new(c.x + ls, c.y - nd.ht / 2.0, c.x + rs, 0.0)
                };
                b.ur.y = c.y + nd.ht / 2.0;
                return Some((side, vec![b]));
            }
        }
        // No field contains the port: C returns `side` with the end's previous
        // boxes; fall back to the node box.
        None
    }
}
