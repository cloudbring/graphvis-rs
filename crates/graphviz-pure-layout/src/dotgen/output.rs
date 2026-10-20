//! Write-back of the dotgen results into the [`graphviz_pure_core::Graph`] IR: node
//! positions, cluster and graph bounding boxes, edge splines and label
//! positions (`lp`, `head_lp`, `tail_lp`, `xlp`), all in final drawing
//! coordinates after `dotneato_postprocess`.

use super::Dot;
use super::types::*;
use graphviz_pure_core::{BezierSegment, BoundingBox, Graph, Point, Spline};

/// Final layout results, detached from the [`Dot`] borrow of the input graph.
pub struct Output {
    node_pos: Vec<Option<Point>>,
    node_rank: Vec<(i32, i32)>,
    /// `ND_lw + ND_rw`, which the output and the renderers take as the node width,
    /// when `translate_drawing` did not reset it (`gv_nodesize` runs for rankdir != TB).
    node_lwrw: Vec<Option<f64>>,
    phase_attrs: Option<i32>,
    bb: BoundingBox,
    cluster_bb: Vec<(usize, BoundingBox)>,
    /// Spline, `lp`, and whether the label is unplaced (`!ED_label(e)->set`).
    edges: Vec<Option<(Spline, Option<Point>, bool)>>,
    /// Extra edge label attributes (`head_lp`, `tail_lp`, `xlp`).
    edge_attrs: Vec<Vec<(&'static str, String)>>,
    node_xlp: Vec<Option<String>>,
    /// `lp` of the root graph and of clusters (by subgraph index).
    graph_lp: Option<String>,
    cluster_lp: Vec<(usize, String)>,
    /// `GD_clust` of the root (`None`) and of each cluster, as subgraph indices.
    cluster_list: Vec<(Option<usize>, String)>,
}

impl Output {
    pub fn phase(d: &Dot<'_>, maxphase: i32) -> Output {
        let n = d.input.nodes.len();
        Output {
            node_pos: vec![Some(Point::new(0.0, 0.0)); n],
            node_rank: (0..n)
                .map(|i| (d.nodes[i].rank, d.nodes[i].order))
                .collect(),
            node_lwrw: vec![None; n],
            phase_attrs: Some(maxphase),
            bb: BoundingBox::new(0.0, 0.0, 0.0, 0.0),
            cluster_bb: Vec::new(),
            edges: Vec::new(),
            edge_attrs: Vec::new(),
            node_xlp: Vec::new(),
            graph_lp: None,
            cluster_lp: Vec::new(),
            cluster_list: Vec::new(),
        }
    }

    pub fn collect(d: &Dot<'_>) -> Output {
        let n = d.input.nodes.len();
        let node_pos = (0..n).map(|i| Some(d.nodes[i].coord)).collect();
        let node_rank = (0..n)
            .map(|i| (d.nodes[i].rank, d.nodes[i].order))
            .collect();
        // With splines=curved, dot_splines_ swaps ND_rw and ND_mval twice (resetRW, then
        // the self-loop restore), so a node with self loops keeps the loop space in
        // ND_rw and is drawn wider.
        let tb = d.graphs[0].rankdir & 3 == 0;
        let node_lwrw = (0..n)
            .map(|i| tb.then(|| d.nodes[i].lw + d.nodes[i].rw))
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
        // The order (and, after copyClusterInfo, the repeats) emit_clusters walks.
        let mut cluster_list = Vec::new();
        fn rec_list(d: &Dot<'_>, g: G, out: &mut Vec<(Option<usize>, String)>) {
            let list: Vec<String> = d.graphs[g]
                .clust
                .iter()
                .filter_map(|&c| d.graphs[c].sub_index.map(|si| si.to_string()))
                .collect();
            // A graph with no subgraph of its own would otherwise land on the root.
            if g == 0 || d.graphs[g].sub_index.is_some() {
                out.push((d.graphs[g].sub_index, list.join(" ")));
            }
            for &c in &d.graphs[g].clust {
                rec_list(d, c, out);
            }
        }
        rec_list(d, 0, &mut cluster_list);
        let mut edges = Vec::new();
        let mut edge_attrs = Vec::new();
        let lp_of = |l: Option<L>| l.filter(|&l| d.labels[l].set).map(|l| d.labels[l].pos);
        // Exact notation, as the renderers expect (`nop.rs`).
        let fmt = |p: Point| format!("{:?},{:?}", p.x, p.y);
        // attach_attrs_and_arrows: edges without splines (or IGNORED) get no
        // attributes; with splines, lp/head_lp/tail_lp are written whether
        // set or not and xlp only when set. An IGNORED edge can still carry a
        // spline (orthoEdges under concentrate routes the first edge of each
        // node pair whatever its type), which emit_edge_graphics draws: the
        // renderer gets it, marked so that attach_attrs writes no attributes.
        let pos_of = |l: Option<L>| l.map(|l| d.labels[l].pos);
        for ei in 0..d.input.edges.len() {
            let ed = &d.edges[ei];
            let mut extra = Vec::new();
            let Some(bzs) = ed.spl.as_ref() else {
                edge_attrs.push(extra);
                // A lost edge (no spline) still draws its placed label (emit_edge);
                // the output writer skips edges without splines. `routed` is false
                // when the root has no edge type: with `splines=none`, and in the pack
                // branch, where only the components route.
                let routed = d.et != crate::common::types::EdgeType::None;
                match ed
                    .label
                    .filter(|&l| routed && ed.edge_type != IGNORED && d.labels[l].set)
                {
                    Some(l) => edges.push(Some((Spline::default(), Some(d.labels[l].pos), false))),
                    // An unset label keeps its position (`(0,0)`, moved by
                    // `shiftEdge` in the pack branch; `map_edge` skips edges
                    // without splines). `edge_in_box` (emit.c) tests
                    // `ED_label(e)` without looking at `set`, so the renderer
                    // needs that position to decide whether to emit the edge.
                    None => edges.push(
                        ed.label
                            .filter(|&l| !d.labels[l].set)
                            .map(|l| (Spline::default(), Some(d.labels[l].pos), true)),
                    ),
                }
                continue;
            };
            let unplaced: String = [('h', ed.head_label), ('t', ed.tail_label)]
                .into_iter()
                .filter(|(_, l)| l.is_some_and(|l| !d.labels[l].set))
                .map(|(c, _)| c)
                .collect();
            if !unplaced.is_empty() {
                extra.push((graphviz_pure_core::UNPLACED_PORT_LABELS_KEY, unplaced));
            }
            let ignored = ed.edge_type == IGNORED;
            if ignored {
                extra.push((graphviz_pure_core::IGNORED_EDGE_KEY, String::new()));
            }
            for (k, ik, p) in [
                ("head_lp", "\u{1}ignorededgehead_lp", pos_of(ed.head_label)),
                ("tail_lp", "\u{1}ignorededgetail_lp", pos_of(ed.tail_label)),
                ("xlp", "\u{1}ignorededgexlp", lp_of(ed.xlabel)),
            ] {
                if let Some(p) = p {
                    if ignored && let Some(v) = d.input.edges[ei].attrs.get(k) {
                        extra.push((ik, v.clone()));
                    }
                    extra.push((k, fmt(p)));
                }
            }
            for (k, p) in [
                (graphviz_pure_core::TAIL_PORT_NAME_KEY, &ed.tail_port),
                (graphviz_pure_core::HEAD_PORT_NAME_KEY, &ed.head_port),
            ] {
                extra.push((k, p.name.clone().unwrap_or_default()));
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
            let unplaced = ed.label.is_some_and(|l| !d.labels[l].set);
            edges.push(Some((spline, pos_of(ed.label), unplaced)));
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
            node_lwrw,
            phase_attrs: None,
            bb,
            cluster_bb,
            edges,
            edge_attrs,
            node_xlp,
            graph_lp,
            cluster_lp,
            cluster_list,
        }
    }

    pub fn apply(self, graph: &mut Graph) {
        for (i, p) in self.node_pos.into_iter().enumerate() {
            if let Some(p) = p {
                graph.nodes[i].pos = p;
            }
            if let Some(w) = self.node_lwrw[i]
                && (w - graph.nodes[i].width * 72.0).abs() > 1e-9
            {
                graph.nodes[i].width = w / 72.0;
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
            // In GD_clust, so drawn even with an empty box (an abandoned layout).
            graph.subgraphs[si].graph_attrs.insert(
                graphviz_pure_core::REGISTERED_CLUSTER_KEY.to_string(),
                String::new(),
            );
        }
        for (i, e) in self.edges.into_iter().enumerate() {
            if let Some((s, lp, unplaced)) = e {
                graph.edges[i].spline = s;
                graph.edges[i].label_pos = lp;
                graph.edges[i].label_unplaced = unplaced;
            }
        }
        for (i, extra) in self.edge_attrs.into_iter().enumerate() {
            for (k, v) in extra {
                graph.edges[i].attrs.insert(k.to_string(), v);
            }
        }
        if let Some(lp) = self.graph_lp {
            graph
                .graph_attrs
                .insert(crate::nop::LABEL_POS_KEY.to_string(), lp);
        }
        for (si, lp) in self.cluster_lp {
            graph.subgraphs[si]
                .graph_attrs
                .insert(crate::nop::LABEL_POS_KEY.to_string(), lp);
        }
        for (si, list) in self.cluster_list {
            let attrs = match si {
                Some(si) => &mut graph.subgraphs[si].graph_attrs,
                None => &mut graph.graph_attrs,
            };
            attrs.insert(graphviz_pure_core::CLUSTER_LIST_KEY.to_string(), list);
        }
        for (i, xlp) in self.node_xlp.into_iter().enumerate() {
            if let Some(v) = xlp {
                graph.nodes[i].attrs.insert("xlp".to_string(), v);
            }
        }
    }
}
