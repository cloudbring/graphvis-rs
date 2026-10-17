//! Phase-1 edge routing and write-back of the dotgen results into the
//! [`gv_core::Graph`] IR.
//!
//! Edges are routed with the existing approximate router
//! (`gv_geom::route_edge_spline`) through the virtual node chains computed by
//! the port. This stands in for `dot_splines` (dotsplines.c/routespl.c), which
//! is phase 2; see `docs/porting/dotgen.md`. Bounding-box bookkeeping follows
//! C: splines and edge labels expand `GD_bb(root)` in layout space before
//! `gv_postprocess` translates the drawing.

use super::Dot;
use super::init::{ccwrotatepf, cwrotatepf};
use super::types::*;
use gv_core::{BezierSegment, BoundingBox, Graph, NodeData, Point, Spline};

impl Dot<'_> {
    /// `resetRW` (dotsplines.c): undo the self-loop width added in position.
    fn reset_rw(&mut self) {
        for n in self.nodes_of(0) {
            let nd = &mut self.nodes[n];
            std::mem::swap(&mut nd.rw, &mut nd.mval);
        }
    }

    /// Virtual nodes of the chain that represents real edge `e`, ordered tail to head.
    pub fn edge_chain(&self, e: E) -> Vec<N> {
        let mut f = self.edges[e].to_virt;
        let mut guard = 0;
        while let Some(x) = f {
            if self.edges[x].real.is_some() && x != e && guard < 64 {
                f = self.edges[x].to_virt;
                guard += 1;
                continue;
            }
            break;
        }
        let Some(first) = f else { return Vec::new() };
        let mut chain = Vec::new();
        let mut cur = Some(first);
        let mut steps = 0;
        while let Some(x) = cur {
            let h = self.edges[x].head;
            if self.nodes[h].node_type != VIRTUAL || steps > self.nodes.len() {
                break;
            }
            chain.push(h);
            cur = self.nodes[h].out.first().copied();
            steps += 1;
        }
        let (et, eh) = (self.edges[e].tail, self.edges[e].head);
        let ftail = self.edges[first].tail;
        if ftail == eh && ftail != et {
            chain.reverse();
        } else if ftail != et && !chain.is_empty() {
            // Chains of edges in clusters start at a cluster skeleton/leader; orient by rank.
            let tr = self.nodes[et].rank;
            let hr = self.nodes[eh].rank;
            let c0 = self.nodes[chain[0]].rank;
            if (hr < tr) != (c0 > self.nodes[*chain.last().unwrap()].rank) && hr < tr {
                chain.reverse();
            }
        }
        chain
    }

    fn update_bb_bz(bb: &mut BoxF, cp: &[Point; 4], depth: u32) {
        let outside = cp
            .iter()
            .any(|p| p.x > bb.ur.x || p.x < bb.ll.x || p.y > bb.ur.y || p.y < bb.ll.y);
        if !outside {
            return;
        }
        if check_control_points(cp) || depth > 20 {
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
            let (l, r) = bezier_split(cp);
            Self::update_bb_bz(bb, &l, depth + 1);
            Self::update_bb_bz(bb, &r, depth + 1);
        }
    }

    /// `addLabelBB` / `updateBB`
    fn update_bb_label(&mut self, l: L) {
        let flip = self.graphs[0].flip();
        let lb = &self.labels[l];
        let (w, h) = if flip {
            (lb.dimen.y, lb.dimen.x)
        } else {
            (lb.dimen.x, lb.dimen.y)
        };
        let p = lb.pos;
        let bb = &mut self.graphs[0].bb;
        bb.ll.x = bb.ll.x.min(p.x - w / 2.0);
        bb.ur.x = bb.ur.x.max(p.x + w / 2.0);
        bb.ll.y = bb.ll.y.min(p.y - h / 2.0);
        bb.ur.y = bb.ur.y.max(p.y + h / 2.0);
    }

    /// Phase-1 stand-in for `dot_splines`.
    pub fn phase1_splines(&mut self) {
        let rankdir = self.graphs[0].rankdir;
        let rd = rankdir * 90;
        let rankdir_s = match rankdir {
            RANKDIR_LR => "LR",
            RANKDIR_BT => "BT",
            RANKDIR_RL => "RL",
            _ => "TB",
        };
        self.reset_rw();
        let directed = self.input.kind == gv_core::GraphKind::Directed;
        let ne = self.input.edges.len();
        self.spl = vec![None; ne];
        // Real nodes in provisional drawing space (rotated, not yet shifted).
        let mut nd: Vec<NodeData> = self.input.nodes.clone();
        for (i, n) in nd.iter_mut().enumerate() {
            n.pos = ccwrotatepf(self.nodes[i].coord, rd);
        }
        let boxes: Vec<BoundingBox> = nd
            .iter()
            .map(|n| {
                let hw = n.width * 36.0;
                let hh = n.height * 36.0;
                BoundingBox::new(n.pos.x - hw, n.pos.y - hh, n.pos.x + hw, n.pos.y + hh)
            })
            .collect();
        // Parallel edges between the same pair get lateral offsets.
        let mut groups: std::collections::BTreeMap<(usize, usize), Vec<usize>> = Default::default();
        for (i, e) in self.input.edges.iter().enumerate() {
            let (a, b) = (e.tail.0 as usize, e.head.0 as usize);
            groups.entry((a.min(b), a.max(b))).or_default().push(i);
        }
        let sep = self.graphs[0].nodesep as f64;
        let mut offset = vec![0.0f64; ne];
        let mut loop_idx = vec![0usize; ne];
        for idx in groups.values() {
            if idx.len() <= 1 {
                continue;
            }
            let e0 = &self.input.edges[idx[0]];
            if e0.tail == e0.head {
                for (k, &i) in idx.iter().enumerate() {
                    loop_idx[i] = k;
                }
            } else {
                let c = idx.len() as f64;
                for (k, &i) in idx.iter().enumerate() {
                    offset[i] = (k as f64 - (c - 1.0) / 2.0) * sep;
                }
            }
        }
        for ei in 0..ne {
            let ie = &self.input.edges[ei];
            let (t, h) = (ie.tail.0 as usize, ie.head.0 as usize);
            if self.nodes[t].cg_root.is_none() || !self.graphs[0].nodes.contains(&t) {
                continue;
            }
            let obstacles: Vec<BoundingBox> = boxes
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != t && *i != h)
                .map(|(_, b)| *b)
                .collect();
            let (mut spline, mut lpos) =
                gv_geom::route_edge_spline(&nd[t], &nd[h], ie, &obstacles, rankdir_s, directed);
            let chain = if t != h {
                self.edge_chain(ei)
            } else {
                Vec::new()
            };
            if t == h && loop_idx[ei] > 0 {
                let extra = loop_idx[ei] as f64 * 14.0;
                for seg in &mut spline.segments {
                    let np = seg.points.len();
                    for (pi, p) in seg.points.iter_mut().enumerate() {
                        if pi > 0 && pi + 1 < np {
                            p.x += extra;
                        }
                    }
                }
                if let Some(lp) = &mut lpos {
                    lp.x += extra;
                }
            } else if !chain.is_empty()
                && spline.segments.len() == 1
                && spline.segments[0].points.len() >= 4
            {
                let seg = &spline.segments[0];
                let p0 = seg.points[0];
                let p3 = *seg.points.last().unwrap();
                let mut wps = vec![p0];
                let mut dev: f64 = 0.0;
                for &v in &chain {
                    let mut wp = ccwrotatepf(self.nodes[v].coord, rd);
                    if offset[ei] != 0.0 {
                        let d = cwrotatepf(Point::new(offset[ei], 0.0), 0);
                        let dd = ccwrotatepf(d, rd);
                        wp.x += dd.x;
                        wp.y += dd.y;
                    }
                    dev = dev.max(point_line_dist(wp, p0, p3));
                    wps.push(wp);
                }
                wps.push(p3);
                if dev > 4.0 || offset[ei] != 0.0 {
                    spline = catmull_rom(&wps, seg.start_arrow, seg.end_arrow);
                }
            } else if offset[ei] != 0.0 {
                lateral_offset(&mut spline, &mut lpos, nd[t].pos, nd[h].pos, offset[ei]);
            }
            // Store in layout space.
            let mut bzs = Vec::new();
            for seg in &spline.segments {
                bzs.push(Bezier {
                    list: seg.points.iter().map(|&p| cwrotatepf(p, rd)).collect(),
                    sp: seg.start_arrow.map(|p| cwrotatepf(p, rd)),
                    ep: seg.end_arrow.map(|p| cwrotatepf(p, rd)),
                });
            }
            for bz in &bzs {
                let pts = &bz.list;
                let mut k = 0;
                while k + 3 < pts.len() {
                    let cp = [pts[k], pts[k + 1], pts[k + 2], pts[k + 3]];
                    let mut bb = self.graphs[0].bb;
                    Self::update_bb_bz(&mut bb, &cp, 0);
                    self.graphs[0].bb = bb;
                    k += 3;
                }
            }
            self.spl[ei] = Some(bzs);
            // Edge label position.
            if let Some(l) = self.edges[ei].label {
                let flip = self.graphs[0].flip();
                let vn = chain
                    .iter()
                    .copied()
                    .find(|&v| self.nodes[v].label == Some(l));
                let fl = (self.n_real_nodes..self.nodes.len())
                    .find(|&v| self.nodes[v].alg == Some(ei) && self.nodes[v].label == Some(l));
                if let Some(v) = vn {
                    let d = self.labels[l].dimen;
                    let width = if flip { d.y } else { d.x };
                    let c = self.nodes[v].coord;
                    self.labels[l].pos = Point::new(c.x + width / 2.0, c.y);
                    self.labels[l].set = true;
                    self.update_bb_label(l);
                } else if let Some(v) = fl {
                    self.labels[l].pos = self.nodes[v].coord;
                    self.labels[l].set = true;
                    self.update_bb_label(l);
                } else if let Some(p) = lpos {
                    self.labels[l].pos = cwrotatepf(p, rd);
                    self.labels[l].set = true;
                    self.update_bb_label(l);
                }
            }
        }
    }
}

fn check_control_points(cp: &[Point; 4]) -> bool {
    let d1 = pt_to_line2(cp[0], cp[3], cp[1]);
    let d2 = pt_to_line2(cp[0], cp[3], cp[2]);
    d1 < 4.0 && d2 < 4.0
}

fn pt_to_line2(a: Point, b: Point, p: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let mut a2 = (p.y - a.y) * dx - (p.x - a.x) * dy;
    a2 *= a2;
    if a2 < 1e-10 {
        return 0.0;
    }
    a2 / (dx * dx + dy * dy)
}

fn bezier_split(v: &[Point; 4]) -> ([Point; 4], [Point; 4]) {
    let mut t = [[Point::default(); 4]; 4];
    t[0] = *v;
    for i in 1..4 {
        for j in 0..(4 - i) {
            t[i][j] = Point::new(
                0.5 * t[i - 1][j].x + 0.5 * t[i - 1][j + 1].x,
                0.5 * t[i - 1][j].y + 0.5 * t[i - 1][j + 1].y,
            );
        }
    }
    let left = [t[0][0], t[1][0], t[2][0], t[3][0]];
    let right = [t[3][0], t[2][1], t[1][2], t[0][3]];
    (left, right)
}

fn point_line_dist(p: Point, a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = dx.hypot(dy);
    if len <= 1e-9 {
        return p.dist(a);
    }
    ((dy * p.x - dx * p.y + b.x * a.y - b.y * a.x).abs()) / len
}

fn catmull_rom(w: &[Point], sa: Option<Point>, ea: Option<Point>) -> Spline {
    let n = w.len();
    let mut pts = vec![w[0]];
    for i in 0..n - 1 {
        let p0 = if i == 0 { w[0] } else { w[i - 1] };
        let p1 = w[i];
        let p2 = w[i + 1];
        let p3 = if i + 2 < n { w[i + 2] } else { w[n - 1] };
        pts.push(Point::new(
            p1.x + (p2.x - p0.x) / 6.0,
            p1.y + (p2.y - p0.y) / 6.0,
        ));
        pts.push(Point::new(
            p2.x - (p3.x - p1.x) / 6.0,
            p2.y - (p3.y - p1.y) / 6.0,
        ));
        pts.push(p2);
    }
    Spline {
        segments: vec![BezierSegment {
            points: pts,
            start_arrow: sa,
            end_arrow: ea,
        }],
    }
}

fn lateral_offset(s: &mut Spline, lp: &mut Option<Point>, from: Point, to: Point, off: f64) {
    let dir = Point::new(to.x - from.x, to.y - from.y).normalized_or_zero();
    let perp = if dir.x.abs() < 1e-9 && dir.y.abs() < 1e-9 {
        Point::new(1.0, 0.0)
    } else {
        Point::new(-dir.y, dir.x)
    };
    for seg in &mut s.segments {
        let n = seg.points.len();
        for (i, p) in seg.points.iter_mut().enumerate() {
            if i > 0 && i + 1 < n {
                p.x += perp.x * off;
                p.y += perp.y * off;
            }
        }
    }
    if let Some(p) = lp {
        p.x += perp.x * off;
        p.y += perp.y * off;
    }
}

/// Final layout results, detached from the [`Dot`] borrow of the input graph.
pub struct Output {
    node_pos: Vec<Option<Point>>,
    node_rank: Vec<(i32, i32)>,
    phase_attrs: Option<i32>,
    bb: BoundingBox,
    cluster_bb: Vec<(usize, BoundingBox)>,
    edges: Vec<Option<(Spline, Option<Point>)>>,
}

impl Output {
    pub fn phase(d: &Dot<'_>, maxphase: i32) -> Output {
        let n = d.input.nodes.len();
        Output {
            node_pos: vec![Some(Point::new(0.0, 0.0)); n],
            node_rank: (0..n)
                .map(|i| (d.nodes[i].rank, d.nodes[i].order))
                .collect(),
            phase_attrs: Some(maxphase),
            bb: BoundingBox::new(0.0, 0.0, 0.0, 0.0),
            cluster_bb: Vec::new(),
            edges: Vec::new(),
        }
    }

    pub fn collect(d: &Dot<'_>) -> Output {
        let n = d.input.nodes.len();
        let node_pos = (0..n).map(|i| Some(d.nodes[i].coord)).collect();
        let node_rank = (0..n)
            .map(|i| (d.nodes[i].rank, d.nodes[i].order))
            .collect();
        let b = d.graphs[0].bb;
        let bb = BoundingBox::new(b.ll.x, b.ll.y, b.ur.x, b.ur.y);
        let mut cluster_bb = Vec::new();
        fn rec(d: &Dot<'_>, g: G, out: &mut Vec<(usize, BoundingBox)>) {
            for &c in &d.graphs[g].clust {
                if let Some(si) = d.graphs[c].sub_index {
                    let b = d.graphs[c].bb;
                    out.push((si, BoundingBox::new(b.ll.x, b.ll.y, b.ur.x, b.ur.y)));
                }
                rec(d, c, out);
            }
        }
        rec(d, 0, &mut cluster_bb);
        let mut edges = Vec::new();
        for ei in 0..d.input.edges.len() {
            let Some(Some(bzs)) = d.spl.get(ei) else {
                edges.push(None);
                continue;
            };
            let spline = Spline {
                segments: bzs
                    .iter()
                    .map(|bz| BezierSegment {
                        points: bz.list.clone(),
                        start_arrow: bz.sp,
                        end_arrow: bz.ep,
                    })
                    .collect(),
            };
            let lp = d.edges[ei]
                .label
                .filter(|&l| d.labels[l].set)
                .map(|l| d.labels[l].pos);
            edges.push(Some((spline, lp)));
        }
        Output {
            node_pos,
            node_rank,
            phase_attrs: None,
            bb,
            cluster_bb,
            edges,
        }
    }

    pub fn apply(self, graph: &mut Graph) {
        for (i, p) in self.node_pos.into_iter().enumerate() {
            if let Some(p) = p {
                graph.nodes[i].pos = p;
            }
            let (r, o) = self.node_rank[i];
            graph.nodes[i].rank = r;
            graph.nodes[i].order = o.max(0) as usize;
            if let Some(ph) = self.phase_attrs {
                graph.nodes[i]
                    .attrs
                    .insert("rank".to_string(), r.to_string());
                if ph >= 2 {
                    graph.nodes[i]
                        .attrs
                        .insert("order".to_string(), o.to_string());
                }
            }
        }
        graph.bb = self.bb;
        for (si, bb) in self.cluster_bb {
            graph.subgraphs[si].bb = bb;
        }
        for (i, e) in self.edges.into_iter().enumerate() {
            if let Some((s, lp)) = e {
                graph.edges[i].spline = s;
                graph.edges[i].label_pos = lp;
            }
        }
    }
}
