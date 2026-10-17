//! Write-back of the dotgen results into the [`gv_core::Graph`] IR: node
//! positions, cluster and graph bounding boxes, edge splines and label
//! positions (`lp`, `head_lp`, `tail_lp`, `xlp`), all in final drawing
//! coordinates after `dotneato_postprocess`.

use super::Dot;
use super::types::*;
use crate::common::init::fmt_g5;
use gv_core::{BezierSegment, BoundingBox, Graph, Point, Spline};

/// Final layout results, detached from the [`Dot`] borrow of the input graph.
pub struct Output {
    node_pos: Vec<Option<Point>>,
    node_rank: Vec<(i32, i32)>,
    phase_attrs: Option<i32>,
    bb: BoundingBox,
    cluster_bb: Vec<(usize, BoundingBox)>,
    edges: Vec<Option<(Spline, Option<Point>)>>,
    /// Extra edge label attributes (`head_lp`, `tail_lp`, `xlp`).
    edge_attrs: Vec<Vec<(&'static str, String)>>,
    node_xlp: Vec<Option<String>>,
    /// `lp` of the root graph and of clusters (by subgraph index).
    graph_lp: Option<String>,
    cluster_lp: Vec<(usize, String)>,
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
            edge_attrs: Vec::new(),
            node_xlp: Vec::new(),
            graph_lp: None,
            cluster_lp: Vec::new(),
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
        let mut edge_attrs = Vec::new();
        let lp_of = |l: Option<L>| l.filter(|&l| d.labels[l].set).map(|l| d.labels[l].pos);
        let fmt = |p: Point| format!("{},{}", fmt_g5(p.x), fmt_g5(p.y));
        // attach_attrs_and_arrows: edges without splines (or IGNORED) get no
        // attributes; with splines, lp/head_lp/tail_lp are written whether
        // set or not and xlp only when set.
        let pos_of = |l: Option<L>| l.map(|l| d.labels[l].pos);
        for ei in 0..d.input.edges.len() {
            let ed = &d.edges[ei];
            let mut extra = Vec::new();
            let Some(bzs) = ed.spl.as_ref().filter(|_| ed.edge_type != IGNORED) else {
                edge_attrs.push(extra);
                edges.push(None);
                continue;
            };
            for (k, p) in [
                ("head_lp", pos_of(ed.head_label)),
                ("tail_lp", pos_of(ed.tail_label)),
                ("xlp", lp_of(ed.xlabel)),
            ] {
                if let Some(p) = p {
                    extra.push((k, fmt(p)));
                }
            }
            edge_attrs.push(extra);
            let spline = Spline {
                segments: bzs
                    .iter()
                    .map(|bz| BezierSegment {
                        points: bz.list.clone(),
                        start_arrow: (bz.sflag != 0).then_some(bz.sp),
                        end_arrow: (bz.eflag != 0).then_some(bz.ep),
                    })
                    .collect(),
            };
            edges.push(Some((spline, pos_of(ed.label))));
        }
        let node_xlp = (0..n).map(|i| lp_of(d.nodes[i].xlabel).map(fmt)).collect();
        let graph_lp = lp_of(d.graphs[0].label).map(fmt);
        let mut cluster_lp = Vec::new();
        fn rec_lp(d: &Dot<'_>, g: G, out: &mut Vec<(usize, Point)>) {
            for &c in &d.graphs[g].clust {
                if let (Some(si), Some(l)) = (d.graphs[c].sub_index, d.graphs[c].label)
                    && d.labels[l].set
                {
                    out.push((si, d.labels[l].pos));
                }
                rec_lp(d, c, out);
            }
        }
        let mut cl = Vec::new();
        rec_lp(d, 0, &mut cl);
        for (si, p) in cl {
            cluster_lp.push((si, fmt(p)));
        }
        Output {
            node_pos,
            node_rank,
            phase_attrs: None,
            bb,
            cluster_bb,
            edges,
            edge_attrs,
            node_xlp,
            graph_lp,
            cluster_lp,
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
            // `is_a_cluster` also honours `cluster=true`.
            graph.subgraphs[si].is_cluster = true;
        }
        for (i, e) in self.edges.into_iter().enumerate() {
            if let Some((s, lp)) = e {
                graph.edges[i].spline = s;
                graph.edges[i].label_pos = lp;
            }
        }
        for (i, extra) in self.edge_attrs.into_iter().enumerate() {
            for (k, v) in extra {
                graph.edges[i].attrs.insert(k.to_string(), v);
            }
        }
        if let Some(lp) = self.graph_lp {
            graph.graph_attrs.insert("lp".to_string(), lp);
        }
        for (si, lp) in self.cluster_lp {
            graph.subgraphs[si].graph_attrs.insert("lp".to_string(), lp);
        }
        for (i, xlp) in self.node_xlp.into_iter().enumerate() {
            if let Some(v) = xlp {
                graph.nodes[i].attrs.insert("xlp".to_string(), v);
            }
        }
    }
}
