#![allow(clippy::needless_range_loop)]
//! Non-hierarchical Graphviz layout engines ported from Graphviz 16.1.0:
//! - `neato`: Kamada-Kawai spring / stress majorization (`lib/neatogen/stress.c`)

use crate::dot::{
    compute_cluster_bounding_boxes, node_bounding_box, normalize_graph_coordinates, parse_attr_f64,
};
use gv_core::{BoundingBox, Graph, Point, Result};
use gv_geom::splines::route_edge_spline;
use std::collections::BTreeMap;
use std::f64::consts::TAU;

const POINTS_PER_INCH: f64 = 72.0;

/// Common initialization for non-hierarchical engines: computes node dimensions and checks for empty graphs.
fn init_non_hierarchical_nodes(graph: &mut Graph) -> bool {
    gv_geom::node_size::size_graph_nodes(graph);
    if graph.nodes.is_empty() {
        graph.bb = BoundingBox::new(0.0, 0.0, 0.0, 0.0);
        return false;
    }
    true
}

/// Routes all edges in a non-hierarchical layout using `gv_geom::splines::route_edge_spline`
/// with parallel multi-edge separation and self-loop handling.
fn route_all_non_hierarchical_splines(graph: &mut Graph) {
    let node_bboxes: Vec<BoundingBox> = graph.nodes.iter().map(node_bounding_box).collect();

    // Group parallel edges between the same unordered node pair.
    let mut pair_groups: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for (ei, edge) in graph.edges.iter().enumerate() {
        let a = edge.tail.0.min(edge.head.0);
        let b = edge.tail.0.max(edge.head.0);
        pair_groups.entry((a, b)).or_default().push(ei);
    }

    let mut multi_offset = vec![0.0f64; graph.edges.len()];
    let mut self_loop_index = vec![0usize; graph.edges.len()];
    for indices in pair_groups.values() {
        if indices.len() <= 1 {
            continue;
        }
        let first = &graph.edges[indices[0]];
        if first.tail == first.head {
            for (k, &ei) in indices.iter().enumerate() {
                self_loop_index[ei] = k;
            }
        } else {
            let count = indices.len();
            for (k, &ei) in indices.iter().enumerate() {
                let centered = (k as f64) - ((count - 1) as f64) / 2.0;
                multi_offset[ei] = centered * 12.0;
            }
        }
    }

    let splines_attr = graph
        .graph_attrs
        .get("splines")
        .map(|s| s.trim().to_ascii_lowercase());
    let avoid_obstacles = matches!(
        splines_attr.as_deref(),
        Some("true" | "spline" | "curved" | "ortho" | "polyline")
    );

    for ei in 0..graph.edges.len() {
        let tail_id = graph.edges[ei].tail;
        let head_id = graph.edges[ei].head;
        let tail_node = graph.nodes[tail_id.0 as usize].clone();
        let head_node = graph.nodes[head_id.0 as usize].clone();

        let obstacles: Vec<BoundingBox> = if avoid_obstacles {
            node_bboxes
                .iter()
                .enumerate()
                .filter(|&(ni, _)| ni != tail_id.0 as usize && ni != head_id.0 as usize)
                .map(|(_, &bb)| bb)
                .collect()
        } else {
            Vec::new()
        };

        let (mut spline, mut label_pos) = route_edge_spline(
            &tail_node,
            &head_node,
            &graph.edges[ei],
            &obstacles,
            "TB",
            graph.kind == gv_core::GraphKind::Directed,
        );

        if tail_id == head_id && self_loop_index[ei] > 0 {
            let extra = (self_loop_index[ei] as f64) * 14.0;
            for seg in &mut spline.segments {
                let len = seg.points.len();
                for (pi, pt) in seg.points.iter_mut().enumerate() {
                    if pi > 0 && pi + 1 < len {
                        pt.x += extra;
                    }
                }
            }
            if let Some(lp) = &mut label_pos {
                lp.x += extra;
            }
        } else if multi_offset[ei].abs() > 1e-6 {
            let dir = Point::new(
                head_node.pos.x - tail_node.pos.x,
                head_node.pos.y - tail_node.pos.y,
            )
            .normalized_or_zero();
            let perp = Point::new(-dir.y, dir.x);
            let off = multi_offset[ei];
            for seg in &mut spline.segments {
                let len = seg.points.len();
                for (pi, pt) in seg.points.iter_mut().enumerate() {
                    if pi > 0 && pi + 1 < len {
                        pt.x += perp.x * off;
                        pt.y += perp.y * off;
                    }
                }
            }
            if let Some(lp) = &mut label_pos {
                lp.x += perp.x * off;
                lp.y += perp.y * off;
            }
        }

        graph.edges[ei].spline = spline;
        graph.edges[ei].label_pos = label_pos;
    }
}

/// Resolves node-node overlaps by iteratively pushing overlapping bounding boxes apart.
fn remove_node_overlaps(graph: &mut Graph, pad_pt: f64) {
    let n = graph.nodes.len();
    if n <= 1 {
        return;
    }

    for _iter in 0..40 {
        let mut moved = false;
        for i in 0..n {
            for j in (i + 1)..n {
                let hw_i = (graph.nodes[i].width * POINTS_PER_INCH) / 2.0 + pad_pt;
                let hh_i = (graph.nodes[i].height * POINTS_PER_INCH) / 2.0 + pad_pt;
                let hw_j = (graph.nodes[j].width * POINTS_PER_INCH) / 2.0 + pad_pt;
                let hh_j = (graph.nodes[j].height * POINTS_PER_INCH) / 2.0 + pad_pt;

                let dx = graph.nodes[j].pos.x - graph.nodes[i].pos.x;
                let dy = graph.nodes[j].pos.y - graph.nodes[i].pos.y;
                let overlap_x = (hw_i + hw_j) - dx.abs();
                let overlap_y = (hh_i + hh_j) - dy.abs();

                if overlap_x > 0.0 && overlap_y > 0.0 {
                    moved = true;
                    if overlap_x < overlap_y {
                        let push = overlap_x / 2.0 + 0.5;
                        let sign = if dx >= 0.0 { 1.0 } else { -1.0 };
                        graph.nodes[i].pos.x -= sign * push;
                        graph.nodes[j].pos.x += sign * push;
                    } else {
                        let push = overlap_y / 2.0 + 0.5;
                        let sign = if dy >= 0.0 { 1.0 } else { -1.0 };
                        graph.nodes[i].pos.y -= sign * push;
                        graph.nodes[j].pos.y += sign * push;
                    }
                }
            }
        }
        if !moved {
            break;
        }
    }
}

// ============================================================================
// 1. Neato: Kamada-Kawai Stress Majorization (`lib/neatogen/stress.c`)
// ============================================================================

pub fn layout_neato(graph: &mut Graph) -> Result<()> {
    if !init_non_hierarchical_nodes(graph) {
        return Ok(());
    }
    let n = graph.nodes.len();
    if n == 1 {
        graph.nodes[0].pos = Point::new(0.0, 0.0);
        route_all_non_hierarchical_splines(graph);
        normalize_graph_coordinates(graph);
        return Ok(());
    }

    let default_len_in = parse_attr_f64(&graph.graph_attrs, "len", 1.0).max(0.1);
    let default_len_pt = default_len_in * POINTS_PER_INCH;

    // Compute average node diameter so ideal edge lengths respect node sizes.
    let avg_diam_pt: f64 = graph
        .nodes
        .iter()
        .map(|nd| nd.width.max(nd.height) * POINTS_PER_INCH)
        .sum::<f64>()
        / (n as f64);
    let base_edge_len = default_len_pt.max(avg_diam_pt + 24.0);

    // All-pairs shortest paths (Floyd-Warshall with edge `len` attributes).
    let disconnected_dist = (n as f64).sqrt().max(2.0) * base_edge_len;
    let mut dist = vec![vec![f64::INFINITY; n]; n];
    for (i, row) in dist.iter_mut().enumerate() {
        row[i] = 0.0;
    }
    for edge in &graph.edges {
        let u = edge.tail.0 as usize;
        let v = edge.head.0 as usize;
        if u == v {
            continue;
        }
        let len_pt = edge
            .attrs
            .get("len")
            .and_then(|s| s.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v > 0.0)
            .map(|v| v * POINTS_PER_INCH)
            .unwrap_or(base_edge_len);
        dist[u][v] = dist[u][v].min(len_pt);
        dist[v][u] = dist[v][u].min(len_pt);
    }

    for k in 0..n {
        for i in 0..n {
            let dik = dist[i][k];
            if !dik.is_finite() {
                continue;
            }
            for j in 0..n {
                let dkj = dist[k][j];
                if dkj.is_finite() && dik + dkj < dist[i][j] {
                    dist[i][j] = dik + dkj;
                }
            }
        }
    }

    for row in &mut dist {
        for d in row.iter_mut() {
            if !d.is_finite() {
                *d = disconnected_dist;
            }
        }
    }

    // Deterministic circular initialization scaled by radius.
    let radius = base_edge_len * (n as f64) / TAU.max(1.0);
    for (i, node) in graph.nodes.iter_mut().enumerate() {
        let angle = TAU * (i as f64) / (n as f64);
        node.pos = Point::new(radius * angle.cos(), radius * angle.sin());
    }

    // Stress majorization iterations (`stress.c`):
    // Minimize $\sum_{i<j} w_{ij} (\|x_i - x_j\| - d_{ij})^2$ with $w_{ij} = 1 / d_{ij}^2$.
    let max_iter = (parse_attr_f64(&graph.graph_attrs, "maxiter", 200.0) as usize).clamp(20, 500);
    let epsilon = parse_attr_f64(&graph.graph_attrs, "epsilon", 1e-3).max(1e-6);

    for _iter in 0..max_iter {
        let mut max_step = 0.0f64;
        for i in 0..n {
            let mut num_x = 0.0f64;
            let mut num_y = 0.0f64;
            let mut denom = 0.0f64;
            let pi = graph.nodes[i].pos;

            for j in 0..n {
                if i == j {
                    continue;
                }
                let dij = dist[i][j].max(1.0);
                let wij = 1.0 / (dij * dij);
                let pj = graph.nodes[j].pos;
                let diff = Point::new(pi.x - pj.x, pi.y - pj.y);
                let dir = diff.normalized_or_zero();
                let dir = if dir.x == 0.0 && dir.y == 0.0 {
                    let ang = TAU * ((i + j) as f64) / (n as f64);
                    Point::new(ang.cos(), ang.sin())
                } else {
                    dir
                };
                num_x += wij * (pj.x + dij * dir.x);
                num_y += wij * (pj.y + dij * dir.y);
                denom += wij;
            }

            if denom > 1e-12 {
                let next_pos = Point::new(num_x / denom, num_y / denom);
                max_step = max_step.max(pi.dist(next_pos));
                graph.nodes[i].pos = next_pos;
            }
        }
        if max_step < epsilon {
            break;
        }
    }

    if graph
        .graph_attrs
        .get("overlap")
        .is_some_and(|v| !matches!(v.trim().to_ascii_lowercase().as_str(), "true" | ""))
    {
        remove_node_overlaps(graph, 4.0);
    }

    compute_cluster_bounding_boxes(graph);
    route_all_non_hierarchical_splines(graph);
    normalize_graph_coordinates(graph);
    Ok(())
}
