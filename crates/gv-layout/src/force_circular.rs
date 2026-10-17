#![allow(clippy::needless_range_loop)]
//! Non-hierarchical Graphviz layout engines ported from Graphviz 16.1.0:
//! - `neato`: Kamada-Kawai spring / stress majorization (`lib/neatogen/stress.c`)
//! - `twopi`: Radial concentric-ring BFS tree layout (`lib/twopigen/circle.c`)
//! - `circo`: Circular biconnected/DFS cycle layout (`lib/circogen/circular.c`)

use crate::dot::{
    compute_cluster_bounding_boxes, node_bounding_box, normalize_graph_coordinates, parse_attr_f64,
};
use gv_core::{BoundingBox, Graph, Point, Result};
use gv_geom::splines::route_edge_spline;
use std::collections::{BTreeMap, VecDeque};
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

// ============================================================================
// 3. Twopi: Radial Concentric-Ring Layout (`lib/twopigen/circle.c`)
// ============================================================================

pub fn layout_twopi(graph: &mut Graph) -> Result<()> {
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

    let ranksep_in = parse_attr_f64(&graph.graph_attrs, "ranksep", 1.0).max(0.2);
    let ranksep_pt = ranksep_in * POINTS_PER_INCH;

    // Build undirected adjacency list.
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for edge in &graph.edges {
        let u = edge.tail.0 as usize;
        let v = edge.head.0 as usize;
        if u != v {
            adj[u].push(v);
            adj[v].push(u);
        }
    }
    for list in &mut adj {
        list.sort_unstable();
        list.dedup();
    }

    // Choose center/root node (`root` attribute on graph or node, or graph-theoretic tree center).
    let root_idx = select_twopi_root(graph, &adj);

    // BFS from `root_idx` to assign concentric ring depth (`SCENTER`), parent (`SPARENT`), and children (`setNStepsToCenter`).
    let mut depth = vec![usize::MAX; n];
    let mut parent = vec![Option::<usize>::None; n];
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut bfs_order = Vec::with_capacity(n);

    let mut q = VecDeque::new();
    depth[root_idx] = 0;
    q.push_back(root_idx);

    while let Some(u) = q.pop_front() {
        bfs_order.push(u);
        for &v in &adj[u] {
            if depth[v] == usize::MAX {
                depth[v] = depth[u] + 1;
                parent[v] = Some(u);
                children[u].push(v);
                q.push_back(v);
            }
        }
    }

    // Attach any disconnected components to ring 1 around `root_idx`.
    for i in 0..n {
        if depth[i] == usize::MAX {
            depth[i] = 1;
            parent[i] = Some(root_idx);
            children[root_idx].push(i);
            q.push_back(i);
            while let Some(u) = q.pop_front() {
                bfs_order.push(u);
                for &v in &adj[u] {
                    if depth[v] == usize::MAX {
                        depth[v] = depth[u] + 1;
                        parent[v] = Some(u);
                        children[u].push(v);
                        q.push_back(v);
                    }
                }
            }
        }
    }

    // Compute leaf count (`STSIZE`) bottom-up (`setSubtreeSize` in `circle.c`).
    let mut subtree_leaves = vec![0usize; n];
    for &u in bfs_order.iter().rev() {
        if children[u].is_empty() {
            subtree_leaves[u] = 1;
        } else {
            subtree_leaves[u] = children[u].iter().map(|&c| subtree_leaves[c]).sum();
        }
    }

    // Assign angular spans (`SPAN`) and angles (`THETA`) top-down (`setSubtreeSpans` & `setPositions` in `circle.c`).
    let mut span = vec![0.0f64; n];
    let mut theta = vec![0.0f64; n];
    span[root_idx] = TAU;
    theta[root_idx] = 0.0;

    for &u in &bfs_order {
        if children[u].is_empty() {
            continue;
        }
        let total_leaves = subtree_leaves[u].max(1) as f64;
        let ratio = span[u] / total_leaves;
        let mut cur_angle = if parent[u].is_none() {
            0.0
        } else {
            theta[u] - span[u] / 2.0
        };
        for &c in &children[u] {
            span[c] = ratio * (subtree_leaves[c] as f64);
            theta[c] = cur_angle + span[c] / 2.0;
            cur_angle += span[c];
        }
    }

    // Convert polar `(r = depth * ranksep_pt, theta)` to Cartesian coordinates.
    for (i, node) in graph.nodes.iter_mut().enumerate() {
        node.rank = depth[i] as i32;
        if i == root_idx {
            node.pos = Point::new(0.0, 0.0);
        } else {
            let r = (depth[i] as f64) * ranksep_pt;
            node.pos = Point::new(r * theta[i].cos(), r * theta[i].sin());
        }
    }

    compute_cluster_bounding_boxes(graph);
    route_all_non_hierarchical_splines(graph);
    normalize_graph_coordinates(graph);
    Ok(())
}

fn select_twopi_root(graph: &Graph, adj: &[Vec<usize>]) -> usize {
    if let Some(root_name) = graph.graph_attrs.get("root")
        && let Some(&nid) = graph.node_lookup.get(root_name.trim())
    {
        return nid.0 as usize;
    }
    for (i, node) in graph.nodes.iter().enumerate() {
        if node
            .attrs
            .get("root")
            .is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1"))
        {
            return i;
        }
    }

    // Find graph center by peeling leaves (`findCenterNode` in `circle.c`).
    let n = graph.nodes.len();
    let mut deg: Vec<usize> = adj.iter().map(Vec::len).collect();
    let mut q: VecDeque<usize> = (0..n).filter(|&i| deg[i] <= 1).collect();
    let mut step = vec![0usize; n];
    while let Some(u) = q.pop_front() {
        for &v in &adj[u] {
            if deg[v] > 1 {
                deg[v] -= 1;
                step[v] = step[v].max(step[u] + 1);
                if deg[v] == 1 {
                    q.push_back(v);
                }
            }
        }
    }
    (0..n).max_by_key(|&i| (step[i], adj[i].len())).unwrap_or(0)
}

// ============================================================================
// 4. Circo: Circular Layout (`lib/circogen/circular.c`)
// ============================================================================

pub fn layout_circo(graph: &mut Graph) -> Result<()> {
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

    let mindist_in = parse_attr_f64(
        &graph.graph_attrs,
        "mindist",
        parse_attr_f64(&graph.graph_attrs, "nodesep", 0.5),
    )
    .max(0.1);
    let mindist_pt = mindist_in * POINTS_PER_INCH;

    // Order nodes around the circle via DFS traversal of biconnected/connected components
    // so graph neighbors are placed adjacent on the perimeter.
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for edge in &graph.edges {
        let u = edge.tail.0 as usize;
        let v = edge.head.0 as usize;
        if u != v {
            adj[u].push(v);
            adj[v].push(u);
        }
    }
    for list in &mut adj {
        list.sort_unstable();
        list.dedup();
    }

    let mut visited = vec![false; n];
    let mut circle_order = Vec::with_capacity(n);
    fn dfs_circo(u: usize, adj: &[Vec<usize>], visited: &mut [bool], order: &mut Vec<usize>) {
        visited[u] = true;
        order.push(u);
        for &v in &adj[u] {
            if !visited[v] {
                dfs_circo(v, adj, visited, order);
            }
        }
    }
    for i in 0..n {
        if !visited[i] {
            dfs_circo(i, &adj, &mut visited, &mut circle_order);
        }
    }

    // Compute circumference from sum of node diameters + `mindist_pt`.
    let diameters: Vec<f64> = circle_order
        .iter()
        .map(|&idx| {
            let nd = &graph.nodes[idx];
            (nd.width * POINTS_PER_INCH).hypot(nd.height * POINTS_PER_INCH)
        })
        .collect();
    let total_circumference: f64 = diameters.iter().map(|&d| d + mindist_pt).sum();
    let radius = (total_circumference / TAU).max(36.0);

    let mut cur_arc = 0.0f64;
    for (pos_idx, &node_idx) in circle_order.iter().enumerate() {
        let seg_arc = diameters[pos_idx] + mindist_pt;
        let mid_arc = cur_arc + seg_arc / 2.0;
        let angle = (mid_arc / total_circumference) * TAU;
        graph.nodes[node_idx].order = pos_idx;
        graph.nodes[node_idx].pos = Point::new(radius * angle.cos(), radius * angle.sin());
        cur_arc += seg_arc;
    }

    compute_cluster_bounding_boxes(graph);
    route_all_non_hierarchical_splines(graph);
    normalize_graph_coordinates(graph);
    Ok(())
}
