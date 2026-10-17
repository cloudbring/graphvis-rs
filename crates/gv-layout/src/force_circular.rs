#![allow(clippy::needless_range_loop)]
//! Non-hierarchical Graphviz layout engines ported from Graphviz 16.1.0:
//! - `neato`: Kamada-Kawai spring / stress majorization (`lib/neatogen/stress.c`)
//! - `fdp` & `sfdp`: Fruchterman-Reingold / spring-electrical force-directed layout (`lib/fdpgen/layout.c`)
//! - `twopi`: Radial concentric-ring BFS tree layout (`lib/twopigen/circle.c`)
//! - `circo`: Circular biconnected/DFS cycle layout (`lib/circogen/circular.c`)
//! - `osage`: Clustered recursive array/grid packing layout (`lib/osage/osageinit.c`)
//! - `patchwork`: Squarified treemap layout (`lib/patchwork/patchwork.c` & `lib/patchwork/tree_map.c`)

use crate::dot::{
    compute_cluster_bounding_boxes, node_bounding_box, normalize_graph_coordinates, parse_attr_f64,
};
use gv_core::{BoundingBox, Graph, NodeId, Point, Result, SubgraphId};
use gv_geom::splines::route_edge_spline;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::f64::consts::{PI, TAU};

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
// 2. Fdp & Sfdp: Spring-Electrical Force-Directed Layout (`lib/fdpgen/layout.c`)
// ============================================================================

pub fn layout_fdp(graph: &mut Graph, is_sfdp: bool) -> Result<()> {
    if !init_non_hierarchical_nodes(graph) {
        return Ok(());
    }
    let n = graph.nodes.len();
    if n == 1 {
        graph.nodes[0].pos = Point::new(0.0, 0.0);
        compute_cluster_bounding_boxes(graph);
        route_all_non_hierarchical_splines(graph);
        normalize_graph_coordinates(graph);
        return Ok(());
    }

    let default_k = if is_sfdp { 0.8 } else { 1.0 };
    let k_in = parse_attr_f64(&graph.graph_attrs, "K", default_k).max(0.1);
    let avg_diam_pt: f64 = graph
        .nodes
        .iter()
        .map(|nd| nd.width.max(nd.height) * POINTS_PER_INCH)
        .sum::<f64>()
        / (n as f64);
    let k_pt = (k_in * POINTS_PER_INCH).max(avg_diam_pt + 20.0);

    // Deterministic golden-angle spiral initialization so no two nodes start collinear or coincident.
    let golden_angle = PI * (3.0 - 5.0f64.sqrt());
    for (i, node) in graph.nodes.iter_mut().enumerate() {
        let r = k_pt * 0.75 * ((i + 1) as f64).sqrt();
        let theta = (i as f64) * golden_angle;
        node.pos = Point::new(r * theta.cos(), r * theta.sin());
    }

    // Identify cluster membership for cluster cohesion forces in `fdp`.
    let mut node_cluster: Vec<Option<SubgraphId>> = vec![None; n];
    for subg in &graph.subgraphs {
        if subg.is_cluster {
            for &nid in &subg.nodes {
                node_cluster[nid.0 as usize] = Some(subg.id);
            }
        }
    }

    let iterations = if is_sfdp { 160usize } else { 120usize };
    let initial_temp = k_pt * (n as f64).sqrt().max(1.5);
    let mut disp = vec![Point::new(0.0, 0.0); n];

    for step in 0..iterations {
        let temp = initial_temp * (1.0 - (step as f64) / (iterations as f64)).max(0.02);
        disp.fill(Point::new(0.0, 0.0));

        // Repulsive forces between all node pairs: $f_r(d) = K^2 / d$.
        for i in 0..n {
            for j in (i + 1)..n {
                let diff = Point::new(
                    graph.nodes[i].pos.x - graph.nodes[j].pos.x,
                    graph.nodes[i].pos.y - graph.nodes[j].pos.y,
                );
                let dist = diff.x.hypot(diff.y).max(1.0);
                let dir = diff.normalized_or_zero();
                let force = (k_pt * k_pt) / dist;
                disp[i].x += dir.x * force;
                disp[i].y += dir.y * force;
                disp[j].x -= dir.x * force;
                disp[j].y -= dir.y * force;
            }
        }

        // Attractive forces along edges: $f_a(d) = w \cdot d^2 / K$.
        for edge in &graph.edges {
            let u = edge.tail.0 as usize;
            let v = edge.head.0 as usize;
            if u == v {
                continue;
            }
            let weight = parse_attr_f64(&edge.attrs, "weight", 1.0).max(0.1);
            let diff = Point::new(
                graph.nodes[u].pos.x - graph.nodes[v].pos.x,
                graph.nodes[u].pos.y - graph.nodes[v].pos.y,
            );
            let dist = diff.x.hypot(diff.y).max(1.0);
            let dir = diff.normalized_or_zero();
            let force = weight * (dist * dist) / k_pt;
            disp[u].x -= dir.x * force;
            disp[u].y -= dir.y * force;
            disp[v].x += dir.x * force;
            disp[v].y += dir.y * force;
        }

        // Cluster cohesion: gently pull nodes in the same cluster toward their cluster centroid.
        for subg in &graph.subgraphs {
            if !subg.is_cluster || subg.nodes.len() <= 1 {
                continue;
            }
            let mut cx = 0.0;
            let mut cy = 0.0;
            for &nid in &subg.nodes {
                cx += graph.nodes[nid.0 as usize].pos.x;
                cy += graph.nodes[nid.0 as usize].pos.y;
            }
            cx /= subg.nodes.len() as f64;
            cy /= subg.nodes.len() as f64;
            for &nid in &subg.nodes {
                let idx = nid.0 as usize;
                if node_cluster[idx] == Some(subg.id) {
                    disp[idx].x += (cx - graph.nodes[idx].pos.x) * 0.25;
                    disp[idx].y += (cy - graph.nodes[idx].pos.y) * 0.25;
                }
            }
        }

        // Weak gravity toward origin to keep disconnected components compact.
        for i in 0..n {
            disp[i].x -= graph.nodes[i].pos.x * 0.03;
            disp[i].y -= graph.nodes[i].pos.y * 0.03;
        }

        // Apply temperature-clamped displacements.
        for i in 0..n {
            let d_len = disp[i].x.hypot(disp[i].y);
            if d_len > 1e-9 {
                let step_len = d_len.min(temp);
                let dir = disp[i].normalized_or_zero();
                graph.nodes[i].pos.x += dir.x * step_len;
                graph.nodes[i].pos.y += dir.y * step_len;
            }
        }
    }

    remove_node_overlaps(graph, 6.0);
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

// ============================================================================
// 5. Osage: Clustered Array/Grid Packing Layout (`lib/osage/osageinit.c`)
// ============================================================================

pub fn layout_osage(graph: &mut Graph) -> Result<()> {
    if !init_non_hierarchical_nodes(graph) {
        return Ok(());
    }

    let pack_pt = parse_attr_f64(&graph.graph_attrs, "pack", 16.0 / POINTS_PER_INCH).max(0.05)
        * POINTS_PER_INCH;

    // Identify top-level subgraphs and track which nodes belong to subgraphs.
    let child_subgs: BTreeSet<SubgraphId> = graph
        .subgraphs
        .iter()
        .flat_map(|s| s.subgraphs.iter().copied())
        .collect();
    let top_subgs: Vec<SubgraphId> = graph
        .subgraphs
        .iter()
        .filter(|s| !child_subgs.contains(&s.id))
        .map(|s| s.id)
        .collect();

    let mut assigned_nodes = vec![false; graph.nodes.len()];
    for &sid in &top_subgs {
        osage_pack_subgraph(graph, sid, &mut assigned_nodes, pack_pt);
    }

    // Pack top-level items (top-level subgraphs + remaining unassigned root nodes) in an array grid.
    #[derive(Clone, Copy)]
    enum PackItem {
        Subgraph(SubgraphId),
        Node(NodeId),
    }

    let mut items: Vec<(PackItem, i64, f64, f64)> = Vec::new();
    for &sid in &top_subgs {
        let subg = &graph.subgraphs[sid.0 as usize];
        if subg.bb.width() > 0.0 || subg.bb.height() > 0.0 {
            let sortv = subg
                .graph_attrs
                .get("sortv")
                .and_then(|s| s.trim().parse::<i64>().ok())
                .unwrap_or(0);
            items.push((
                PackItem::Subgraph(sid),
                sortv,
                subg.bb.width(),
                subg.bb.height(),
            ));
        }
    }
    for (ni, node) in graph.nodes.iter().enumerate() {
        if !assigned_nodes[ni] {
            let sortv = node
                .attrs
                .get("sortv")
                .and_then(|s| s.trim().parse::<i64>().ok())
                .unwrap_or(0);
            items.push((
                PackItem::Node(node.id),
                sortv,
                node.width * POINTS_PER_INCH,
                node.height * POINTS_PER_INCH,
            ));
        }
    }

    items.sort_by_key(|&(_, sortv, _, _)| sortv);
    let placements = pack_boxes_in_grid(&items, pack_pt);

    for (idx, (item, _, w, h)) in items.into_iter().enumerate() {
        let ll = placements[idx];
        match item {
            PackItem::Node(nid) => {
                graph.nodes[nid.0 as usize].pos = Point::new(ll.x + w / 2.0, ll.y + h / 2.0);
            }
            PackItem::Subgraph(sid) => {
                let old_ll = graph.subgraphs[sid.0 as usize].bb.ll;
                let dx = ll.x - old_ll.x;
                let dy = ll.y - old_ll.y;
                translate_subgraph(graph, sid, dx, dy);
            }
        }
    }

    route_all_non_hierarchical_splines(graph);
    normalize_graph_coordinates(graph);
    Ok(())
}

fn osage_pack_subgraph(
    graph: &mut Graph,
    subg_id: SubgraphId,
    assigned_nodes: &mut [bool],
    pack_pt: f64,
) {
    let child_subgs = graph.subgraphs[subg_id.0 as usize].subgraphs.clone();
    for &cid in &child_subgs {
        osage_pack_subgraph(graph, cid, assigned_nodes, pack_pt);
    }

    #[derive(Clone, Copy)]
    enum SubItem {
        Subgraph(SubgraphId),
        Node(NodeId),
    }

    let mut items: Vec<(SubItem, i64, f64, f64)> = Vec::new();
    for &cid in &child_subgs {
        let csub = &graph.subgraphs[cid.0 as usize];
        if csub.bb.width() > 0.0 || csub.bb.height() > 0.0 {
            let sortv = csub
                .graph_attrs
                .get("sortv")
                .and_then(|s| s.trim().parse::<i64>().ok())
                .unwrap_or(0);
            items.push((
                SubItem::Subgraph(cid),
                sortv,
                csub.bb.width(),
                csub.bb.height(),
            ));
        }
    }

    let direct_nodes = graph.subgraphs[subg_id.0 as usize].nodes.clone();
    for nid in direct_nodes {
        let ni = nid.0 as usize;
        if !assigned_nodes[ni] {
            assigned_nodes[ni] = true;
            let node = &graph.nodes[ni];
            let sortv = node
                .attrs
                .get("sortv")
                .and_then(|s| s.trim().parse::<i64>().ok())
                .unwrap_or(0);
            items.push((
                SubItem::Node(nid),
                sortv,
                node.width * POINTS_PER_INCH,
                node.height * POINTS_PER_INCH,
            ));
        }
    }

    if items.is_empty() {
        graph.subgraphs[subg_id.0 as usize].bb = BoundingBox::new(0.0, 0.0, 0.0, 0.0);
        return;
    }

    items.sort_by_key(|&(_, sortv, _, _)| sortv);
    let placements = pack_boxes_in_grid(&items, pack_pt);

    let mut max_x = 0.0f64;
    let mut max_y = 0.0f64;
    let margin = 8.0;

    for (idx, (item, _, w, h)) in items.into_iter().enumerate() {
        let ll = Point::new(placements[idx].x + margin, placements[idx].y + margin);
        max_x = max_x.max(ll.x + w + margin);
        max_y = max_y.max(ll.y + h + margin);
        match item {
            SubItem::Node(nid) => {
                graph.nodes[nid.0 as usize].pos = Point::new(ll.x + w / 2.0, ll.y + h / 2.0);
            }
            SubItem::Subgraph(cid) => {
                let old_ll = graph.subgraphs[cid.0 as usize].bb.ll;
                translate_subgraph(graph, cid, ll.x - old_ll.x, ll.y - old_ll.y);
            }
        }
    }

    graph.subgraphs[subg_id.0 as usize].bb = BoundingBox::new(0.0, 0.0, max_x, max_y);
}

fn translate_subgraph(graph: &mut Graph, subg_id: SubgraphId, dx: f64, dy: f64) {
    let idx = subg_id.0 as usize;
    graph.subgraphs[idx].bb.ll.x += dx;
    graph.subgraphs[idx].bb.ll.y += dy;
    graph.subgraphs[idx].bb.ur.x += dx;
    graph.subgraphs[idx].bb.ur.y += dy;

    let nodes = graph.subgraphs[idx].nodes.clone();
    for nid in nodes {
        graph.nodes[nid.0 as usize].pos.x += dx;
        graph.nodes[nid.0 as usize].pos.y += dy;
    }
    let children = graph.subgraphs[idx].subgraphs.clone();
    for cid in children {
        let c_idx = cid.0 as usize;
        graph.subgraphs[c_idx].bb.ll.x += dx;
        graph.subgraphs[c_idx].bb.ll.y += dy;
        graph.subgraphs[c_idx].bb.ur.x += dx;
        graph.subgraphs[c_idx].bb.ur.y += dy;
    }
}

fn pack_boxes_in_grid<T>(items: &[(T, i64, f64, f64)], sep: f64) -> Vec<Point> {
    let n = items.len();
    if n == 0 {
        return Vec::new();
    }
    let cols = (n as f64).sqrt().ceil().max(1.0) as usize;
    let rows = n.div_ceil(cols);

    let mut col_widths = vec![0.0f64; cols];
    let mut row_heights = vec![0.0f64; rows];
    for (idx, &(_, _, w, h)) in items.iter().enumerate() {
        let r = idx / cols;
        let c = idx % cols;
        col_widths[c] = col_widths[c].max(w);
        row_heights[r] = row_heights[r].max(h);
    }

    let mut col_x = vec![0.0f64; cols];
    for c in 1..cols {
        col_x[c] = col_x[c - 1] + col_widths[c - 1] + sep;
    }
    let mut row_y = vec![0.0f64; rows];
    for r in (0..rows.saturating_sub(1)).rev() {
        row_y[r] = row_y[r + 1] + row_heights[r + 1] + sep;
    }

    let mut result = Vec::with_capacity(n);
    for (idx, &(_, _, w, h)) in items.iter().enumerate() {
        let r = idx / cols;
        let c = idx % cols;
        let ll_x = col_x[c] + (col_widths[c] - w) / 2.0;
        let ll_y = row_y[r] + (row_heights[r] - h) / 2.0;
        result.push(Point::new(ll_x, ll_y));
    }
    result
}

// ============================================================================
// 6. Patchwork: Squarified Treemap Layout (`lib/patchwork/patchwork.c` & `tree_map.c`)
// ============================================================================

const PATCHWORK_SCALE: f64 = 1000.0;

pub fn layout_patchwork(graph: &mut Graph) -> Result<()> {
    if !init_non_hierarchical_nodes(graph) {
        return Ok(());
    }

    // Identify top-level clusters and unassigned nodes to build the treemap hierarchy (`mkTree` in `patchwork.c`).
    let child_subgs: BTreeSet<SubgraphId> = graph
        .subgraphs
        .iter()
        .flat_map(|s| s.subgraphs.iter().copied())
        .collect();
    let top_subgs: Vec<SubgraphId> = graph
        .subgraphs
        .iter()
        .filter(|s| !child_subgs.contains(&s.id))
        .map(|s| s.id)
        .collect();

    let mut node_claimed = vec![false; graph.nodes.len()];
    let mut root_children = Vec::new();
    for &sid in &top_subgs {
        if let Some(child) = build_patchwork_subtree(graph, sid, &mut node_claimed) {
            root_children.push(child);
        }
    }
    for (ni, node) in graph.nodes.iter().enumerate() {
        if !node_claimed[ni] {
            node_claimed[ni] = true;
            let area = parse_attr_f64(&node.attrs, "area", 1.0).max(0.01) * PATCHWORK_SCALE;
            root_children.push(PatchworkNode {
                kind: PatchworkKind::Node(node.id),
                area,
                children: Vec::new(),
            });
        }
    }

    let total_area: f64 = root_children
        .iter()
        .map(|c| c.area)
        .sum::<f64>()
        .max(PATCHWORK_SCALE);
    let side = total_area.sqrt().max(72.0);
    let root_rect = BoundingBox::new(0.0, 0.0, side, side);

    layout_patchwork_children(graph, &mut root_children, root_rect);
    route_all_non_hierarchical_splines(graph);
    normalize_graph_coordinates(graph);
    Ok(())
}

#[derive(Clone)]
enum PatchworkKind {
    Subgraph(SubgraphId),
    Node(NodeId),
}

#[derive(Clone)]
struct PatchworkNode {
    kind: PatchworkKind,
    area: f64,
    children: Vec<PatchworkNode>,
}

fn build_patchwork_subtree(
    graph: &Graph,
    subg_id: SubgraphId,
    node_claimed: &mut [bool],
) -> Option<PatchworkNode> {
    let subg = &graph.subgraphs[subg_id.0 as usize];
    let mut children = Vec::new();

    for &cid in &subg.subgraphs {
        if let Some(cnode) = build_patchwork_subtree(graph, cid, node_claimed) {
            children.push(cnode);
        }
    }
    for &nid in &subg.nodes {
        let ni = nid.0 as usize;
        if !node_claimed[ni] {
            node_claimed[ni] = true;
            let area =
                parse_attr_f64(&graph.nodes[ni].attrs, "area", 1.0).max(0.01) * PATCHWORK_SCALE;
            children.push(PatchworkNode {
                kind: PatchworkKind::Node(nid),
                area,
                children: Vec::new(),
            });
        }
    }

    let area = if children.is_empty() {
        parse_attr_f64(&subg.graph_attrs, "area", 1.0).max(0.01) * PATCHWORK_SCALE
    } else {
        children.iter().map(|c| c.area).sum()
    };

    Some(PatchworkNode {
        kind: PatchworkKind::Subgraph(subg_id),
        area,
        children,
    })
}

fn layout_patchwork_children(graph: &mut Graph, children: &mut [PatchworkNode], rect: BoundingBox) {
    if children.is_empty() {
        return;
    }
    // Sort children by descending area (`nodecmp` in `patchwork.c`).
    children.sort_by(|a, b| {
        b.area
            .partial_cmp(&a.area)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let areas: Vec<f64> = children.iter().map(|c| c.area).collect();
    let sub_rects = squarify_treemap(&areas, rect);

    for (child, crect) in children.iter_mut().zip(sub_rects) {
        match child.kind {
            PatchworkKind::Node(nid) => {
                let n = &mut graph.nodes[nid.0 as usize];
                let w = crect.width().max(4.0);
                let h = crect.height().max(4.0);
                n.pos = Point::new(crect.ll.x + w / 2.0, crect.ll.y + h / 2.0);
                n.width = w / POINTS_PER_INCH;
                n.height = h / POINTS_PER_INCH;
            }
            PatchworkKind::Subgraph(sid) => {
                graph.subgraphs[sid.0 as usize].bb = crect;
                let inset =
                    parse_attr_f64(&graph.subgraphs[sid.0 as usize].graph_attrs, "inset", 0.0)
                        .max(0.0);
                let inner = BoundingBox::new(
                    (crect.ll.x + inset).min(crect.ur.x),
                    (crect.ll.y + inset).min(crect.ur.y),
                    (crect.ur.x - inset).max(crect.ll.x),
                    (crect.ur.y - inset).max(crect.ll.y),
                );
                layout_patchwork_children(graph, &mut child.children, inner);
            }
        }
    }
}

/// Squarified treemap subdivision ported from `lib/patchwork/tree_map.c`.
fn squarify_treemap(areas: &[f64], rect: BoundingBox) -> Vec<BoundingBox> {
    let n = areas.len();
    let mut out = vec![BoundingBox::default(); n];
    if n == 0 {
        return out;
    }

    let total_area: f64 = areas.iter().sum();
    let rect_area = (rect.width() * rect.height()).max(1.0);
    let scale = if total_area > 0.0 {
        rect_area / total_area
    } else {
        1.0
    };
    let scaled: Vec<f64> = areas.iter().map(|&a| (a * scale).max(1e-6)).collect();

    let mut cur_rect = rect;
    let mut idx = 0usize;

    while idx < n {
        let w = cur_rect.width().max(1e-6);
        let h = cur_rect.height().max(1e-6);
        let shorter = w.min(h);

        // Greedily add items to the current strip while the worst aspect ratio improves (`tree_map.c`).
        let mut strip_end = idx + 1;
        let mut strip_sum = scaled[idx];
        let mut best_aspect = worst_strip_aspect(&scaled[idx..strip_end], strip_sum, shorter);

        while strip_end < n {
            let next_sum = strip_sum + scaled[strip_end];
            let next_aspect = worst_strip_aspect(&scaled[idx..=strip_end], next_sum, shorter);
            if next_aspect <= best_aspect {
                strip_sum = next_sum;
                best_aspect = next_aspect;
                strip_end += 1;
            } else {
                break;
            }
        }

        // Lay out `idx..strip_end` matching `tree_map.c:55-82`.
        if w <= h {
            // Tall/square rect (`fillrec.size[0] <= fillrec.size[1]`): horizontal strip at top, left to right.
            let strip_h = (strip_sum / w).min(h);
            let y_bottom = (cur_rect.ur.y - strip_h).max(cur_rect.ll.y);
            let mut x = cur_rect.ll.x;
            for (i, &item_area) in scaled.iter().enumerate().take(strip_end).skip(idx) {
                let item_w = if i + 1 == strip_end {
                    (cur_rect.ur.x - x).max(0.0)
                } else {
                    (item_area / strip_h.max(1e-6)).min(cur_rect.ur.x - x)
                };
                out[i] = BoundingBox::new(x, y_bottom, x + item_w, cur_rect.ur.y);
                x += item_w;
            }
            cur_rect.ur.y = y_bottom;
        } else {
            // Wide rect (`fillrec.size[0] > fillrec.size[1]`): vertical strip at left, top to bottom.
            let strip_w = (strip_sum / h).min(w);
            let mut y_top = cur_rect.ur.y;
            for (i, &item_area) in scaled.iter().enumerate().take(strip_end).skip(idx) {
                let item_h = if i + 1 == strip_end {
                    (y_top - cur_rect.ll.y).max(0.0)
                } else {
                    (item_area / strip_w.max(1e-6)).min(y_top - cur_rect.ll.y)
                };
                out[i] = BoundingBox::new(
                    cur_rect.ll.x,
                    y_top - item_h,
                    cur_rect.ll.x + strip_w,
                    y_top,
                );
                y_top -= item_h;
            }
            cur_rect.ll.x += strip_w;
        }

        idx = strip_end;
    }

    out
}

fn worst_strip_aspect(strip: &[f64], sum: f64, side: f64) -> f64 {
    if strip.is_empty() || sum <= 1e-12 || side <= 1e-12 {
        return f64::INFINITY;
    }
    let s2 = side * side;
    let sum2 = sum * sum;
    let mut worst = 0.0f64;
    for &r in strip {
        let r = r.max(1e-12);
        let aspect = ((s2 * r) / sum2).max(sum2 / (s2 * r));
        worst = worst.max(aspect);
    }
    worst
}
