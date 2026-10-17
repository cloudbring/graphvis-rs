//! Hierarchical `dot` layout engine ported from Graphviz 16.1.0 (`lib/dotgen` and `lib/common/ns.c`).
//!
//! Implements the 5-stage Sugiyama / Network-Simplex layout pipeline:
//! 0. **Node Sizing & Attribute Parsing**: Computes node label/shape dimensions in inches and parses
//!    `rankdir`, `nodesep`, `ranksep`, `margin`, and `pad`.
//! 1. **Cycle Breaking (`acyclic.c`)**: Collapses `rank=same|min|source|max|sink` subgraphs via
//!    Union-Find and runs greedy DFS cycle breaking (feedback arc set) respecting edge weights.
//! 2. **Rank Assignment (`rank.c` & `common/ns.c`)**: Decomposes into weakly connected components,
//!    computes an initial topological ranking, constructs a tight feasible spanning tree, runs the
//!    Network Simplex cut-value pivot loop to minimize $\sum w_e (r(v) - r(u))$, and balances ranks
//!    (`TB_balance`).
//! 3. **Crossing Minimization (`mincross.c`)**: Expands multi-rank edges into virtual node chains with
//!    weighted virtual edges (`C_VV = 4`, `C_VS = 2`, `C_EE = 1`), builds initial BFS rank orderings,
//!    performs alternating down/up weighted-median sweeps (`medians` + `reorder`), and runs local
//!    adjacent-exchange `transpose` refinement.
//! 4. **Coordinate Assignment (`position.c`)**: Computes rank positions (`set_ycoords`) with cluster
//!    margin/label expansion, solves within-rank positions (`set_xcoords`) using the auxiliary graph
//!    Network Simplex + median attraction refinement, applies `rankdir` (`TB`, `BT`, `LR`, `RL`), and
//!    computes cluster bounding boxes (`subgraph.bb`).
//! 5. **Spline Routing & Postprocessing (`dotsplines.c`)**: Routes all edges (including multi-edges,
//!    flat edges, and self-loops) around intermediate rank obstacles via `gv_geom::splines::route_edge_spline`,
//!    and translates the entire layout so `graph.bb.ll == (0.0, 0.0)`.

use gv_core::{
    AttrMap, BezierSegment, BoundingBox, EdgeId, Graph, NodeId, Point, Result, Spline, SubgraphId,
    font::{
        DEFAULT_FONT_NAME, DEFAULT_FONT_SIZE, LabelSubstContext,
        measure_multiline_label_with_context,
    },
};
use gv_geom::{
    ShapeKind, compute_node_dimensions_with_rankdir, is_html_label, parse_html_table,
    resolve_port_offset, route_edge_spline,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const POINTS_PER_INCH: f64 = 72.0;
const DEFAULT_NODESEP_IN: f64 = 0.25;
const DEFAULT_RANKSEP_IN: f64 = 0.50;
const MIN_NODESEP_IN: f64 = 0.02;
const MIN_RANKSEP_IN: f64 = 0.02;
const CLUSTER_MARGIN_PT: f64 = 8.0;
const NS_SEARCH_SIZE: usize = 30;
const NS_MAX_ITER: usize = 2000;
const MINCROSS_MAX_ITER: usize = 24;
const MINCROSS_MIN_QUIT: usize = 8;
const MINCROSS_CONVERGENCE: f64 = 0.995;
const MC_SCALE: i32 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RankDir {
    Tb,
    Bt,
    Lr,
    Rl,
}

impl RankDir {
    pub fn parse(s: Option<&str>) -> Self {
        match s.unwrap_or("TB").trim().to_ascii_uppercase().as_str() {
            "BT" => Self::Bt,
            "LR" => Self::Lr,
            "RL" => Self::Rl,
            _ => Self::Tb,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tb => "TB",
            Self::Bt => "BT",
            Self::Lr => "LR",
            Self::Rl => "RL",
        }
    }

    #[inline]
    pub fn is_flipped(self) -> bool {
        matches!(self, Self::Lr | Self::Rl)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RankSetKind {
    Normal,
    Same,
    Min,
    Source,
    Max,
    Sink,
}

impl RankSetKind {
    fn parse(s: Option<&str>) -> Self {
        match s.unwrap_or("").trim().to_ascii_lowercase().as_str() {
            "same" => Self::Same,
            "min" => Self::Min,
            "source" => Self::Source,
            "max" => Self::Max,
            "sink" => Self::Sink,
            _ => Self::Normal,
        }
    }
}

/// Runs the full 5-stage `dot` layout pipeline on `graph`.
pub fn layout_dot(graph: &mut Graph) -> Result<()> {
    let rankdir = RankDir::parse(graph.graph_attrs.get("rankdir").map(String::as_str));

    // Stage 0: Size all nodes using gv-geom shapes & font metrics with rankdir context.
    let graph_name = graph.name.clone();
    for node in &mut graph.nodes {
        compute_node_dimensions_with_rankdir(node, &graph_name, rankdir.as_str());
    }

    if graph.nodes.is_empty() {
        graph.bb = BoundingBox::new(0.0, 0.0, 0.0, 0.0);
        return Ok(());
    }

    let nodesep_pt = (parse_attr_f64(&graph.graph_attrs, "nodesep", DEFAULT_NODESEP_IN)
        .max(MIN_NODESEP_IN)
        * POINTS_PER_INCH)
        .round();
    let ranksep_raw = graph
        .graph_attrs
        .get("ranksep")
        .map(String::as_str)
        .unwrap_or("0.5");
    let equal_ranksep = ranksep_raw.to_ascii_lowercase().contains("equally");
    let ranksep_in = ranksep_raw
        .split_whitespace()
        .next()
        .and_then(|tok| tok.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(DEFAULT_RANKSEP_IN)
        .max(MIN_RANKSEP_IN);
    let mut ranksep_pt = (ranksep_in * POINTS_PER_INCH).round();

    // Check if any non-self edge has an edge label (`edgelabel_ranks` in `rank.c:176`).
    let has_edge_labels = graph
        .edges
        .iter()
        .any(|e| e.tail != e.head && e.attrs.get("label").is_some_and(|l| !l.trim().is_empty()));
    if has_edge_labels {
        ranksep_pt = ((ranksep_pt as i32 + 1) / 2) as f64;
    }

    // Stage 1 & 2: Cycle breaking (acyclic) + Network Simplex rank assignment.
    let reversed_edges = assign_ranks(graph, has_edge_labels);

    // Stage 3: Crossing minimization (mincross) with virtual node chains.
    let mut vgraph = build_virtual_graph(graph, &reversed_edges, rankdir, nodesep_pt);
    minimize_crossings(graph, &mut vgraph, rankdir);

    // Copy final order back to real nodes.
    for (r, rank_nodes) in vgraph.ranks.iter().enumerate() {
        for (ord, &vn_idx) in rank_nodes.iter().enumerate() {
            vgraph.nodes[vn_idx].rank = r as i32;
            vgraph.nodes[vn_idx].order = ord;
            if let Some(real_id) = vgraph.nodes[vn_idx].real_node {
                graph.nodes[real_id.0 as usize].rank = r as i32;
                graph.nodes[real_id.0 as usize].order = ord;
            }
        }
    }

    // Stage 4: Coordinate assignment (position) + cluster bounding boxes.
    assign_coordinates(
        graph,
        &mut vgraph,
        rankdir,
        nodesep_pt,
        ranksep_pt,
        equal_ranksep,
        has_edge_labels,
    );

    // Stage 5: Edge spline routing (dotsplines) + coordinate normalization.
    route_all_dot_splines(graph, &vgraph, rankdir);
    normalize_graph_coordinates(graph);

    Ok(())
}

// ============================================================================
// Stage 1 & 2: Acyclic & Network Simplex Ranking (`acyclic.c`, `rank.c`, `ns.c`)
// ============================================================================

#[derive(Debug, Clone)]
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            let p = self.parent[x];
            self.parent[x] = self.parent[p];
            x = p;
        }
        x
    }

    fn union(&mut self, a: usize, b: usize) -> usize {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb {
            return ra;
        }
        let (keep, drop) = if ra < rb { (ra, rb) } else { (rb, ra) };
        self.parent[drop] = keep;
        keep
    }
}

#[derive(Debug, Clone)]
struct NsEdge {
    tail: usize,
    head: usize,
    minlen: i32,
    weight: i32,
    cutvalue: i32,
    tree_index: Option<usize>,
}

/// Breaks cycles and assigns optimal integer ranks (`0..=max_rank`) to `graph.nodes`.
/// Returns the set of `EdgeId`s that were reversed during cycle breaking.
fn assign_ranks(graph: &mut Graph, has_edge_labels: bool) -> BTreeSet<EdgeId> {
    let n_orig = graph.nodes.len();
    let mut uf = UnionFind::new(n_orig);
    let mut rank_kinds = vec![RankSetKind::Normal; n_orig];
    let mut min_leader: Option<usize> = None;
    let mut max_leader: Option<usize> = None;

    // Collapse rank=same|min|source|max|sink subgraphs (`collapse_rankset` in `rank.c`).
    for subg in &graph.subgraphs {
        if subg.is_cluster {
            continue;
        }
        let kind = RankSetKind::parse(subg.graph_attrs.get("rank").map(String::as_str));
        if kind == RankSetKind::Normal || subg.nodes.is_empty() {
            continue;
        }
        let first = subg.nodes[0].0 as usize;
        let mut leader = uf.find(first);
        for &nid in &subg.nodes[1..] {
            leader = uf.union(leader, nid.0 as usize);
        }
        rank_kinds[leader] = kind;
        match kind {
            RankSetKind::Min | RankSetKind::Source => {
                min_leader = Some(match min_leader {
                    Some(prev) => {
                        let merged = uf.union(prev, leader);
                        if kind == RankSetKind::Source || rank_kinds[prev] == RankSetKind::Source {
                            rank_kinds[merged] = RankSetKind::Source;
                        } else {
                            rank_kinds[merged] = RankSetKind::Min;
                        }
                        merged
                    }
                    None => leader,
                });
            }
            RankSetKind::Max | RankSetKind::Sink => {
                max_leader = Some(match max_leader {
                    Some(prev) => {
                        let merged = uf.union(prev, leader);
                        if kind == RankSetKind::Sink || rank_kinds[prev] == RankSetKind::Sink {
                            rank_kinds[merged] = RankSetKind::Sink;
                        } else {
                            rank_kinds[merged] = RankSetKind::Max;
                        }
                        merged
                    }
                    None => leader,
                });
            }
            _ => {}
        }
    }

    // Map each leader to a dense index `0..n_leaders`.
    let mut leader_to_dense = BTreeMap::new();
    let mut dense_to_leader = Vec::new();
    for i in 0..n_orig {
        let l = uf.find(i);
        if let std::collections::btree_map::Entry::Vacant(e) = leader_to_dense.entry(l) {
            let d = dense_to_leader.len();
            e.insert(d);
            dense_to_leader.push(l);
        }
    }
    let n_dense = dense_to_leader.len();

    // Collect directed ranking edges on dense nodes (ignoring self-loops, flat edges, and constraint=false).
    #[derive(Clone)]
    struct RawRankEdge {
        orig_ids: Vec<EdgeId>,
        u: usize,
        v: usize,
        weight: i32,
        minlen: i32,
    }

    let mut pair_map: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    let mut raw_edges: Vec<RawRankEdge> = Vec::new();

    for edge in &graph.edges {
        if edge
            .attrs
            .get("constraint")
            .is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "false" | "0" | "no"))
        {
            continue;
        }
        let u = leader_to_dense[&uf.find(edge.tail.0 as usize)];
        let v = leader_to_dense[&uf.find(edge.head.0 as usize)];
        if u == v {
            continue;
        }
        let weight = (parse_attr_f64(&edge.attrs, "weight", 1.0).round() as i32).max(0);
        let mut minlen = (parse_attr_f64(&edge.attrs, "minlen", 1.0).round() as i32).max(0);
        if has_edge_labels {
            minlen *= 2;
        }

        if let Some(&idx) = pair_map.get(&(u, v)) {
            raw_edges[idx].orig_ids.push(edge.id);
            raw_edges[idx].weight = raw_edges[idx].weight.saturating_add(weight.max(1));
            raw_edges[idx].minlen = raw_edges[idx].minlen.max(minlen);
        } else {
            let idx = raw_edges.len();
            pair_map.insert((u, v), idx);
            raw_edges.push(RawRankEdge {
                orig_ids: vec![edge.id],
                u,
                v,
                weight: weight.max(1),
                minlen,
            });
        }
    }

    // Add source/sink/min/max rank constraints if present.
    let min_dense = min_leader.map(|l| leader_to_dense[&uf.find(l)]);
    let max_dense = max_leader.map(|l| leader_to_dense[&uf.find(l)]);
    if let Some(md) = min_dense {
        let min_len = if rank_kinds[dense_to_leader[md]] == RankSetKind::Source {
            1
        } else {
            0
        };
        for d in 0..n_dense {
            if d != md && Some(d) != max_dense {
                raw_edges.push(RawRankEdge {
                    orig_ids: Vec::new(),
                    u: md,
                    v: d,
                    weight: 0,
                    minlen: min_len,
                });
            }
        }
    }
    if let Some(xd) = max_dense {
        let min_len = if rank_kinds[dense_to_leader[xd]] == RankSetKind::Sink {
            1
        } else {
            0
        };
        for d in 0..n_dense {
            if d != xd {
                raw_edges.push(RawRankEdge {
                    orig_ids: Vec::new(),
                    u: d,
                    v: xd,
                    weight: 0,
                    minlen: min_len,
                });
            }
        }
    }

    // Stage 1 (`acyclic.c`): DFS cycle breaking in graph insertion order (`0..n_dense`).
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n_dense];
    for (ei, e) in raw_edges.iter().enumerate() {
        adj[e.u].push(ei);
    }
    let start_order: Vec<usize> = (0..n_dense).collect();

    let mut visited = vec![false; n_dense];
    let mut on_stack = vec![false; n_dense];
    let mut reversed_raw = vec![false; raw_edges.len()];

    fn dfs_acyclic(
        u: usize,
        adj: &[Vec<usize>],
        raw_edges: &[RawRankEdge],
        visited: &mut [bool],
        on_stack: &mut [bool],
        reversed_raw: &mut [bool],
    ) {
        visited[u] = true;
        on_stack[u] = true;
        for &ei in &adj[u] {
            let v = raw_edges[ei].v;
            if on_stack[v] {
                reversed_raw[ei] = true;
            } else if !visited[v] {
                dfs_acyclic(v, adj, raw_edges, visited, on_stack, reversed_raw);
            }
        }
        on_stack[u] = false;
    }

    for start in start_order {
        if !visited[start] {
            dfs_acyclic(
                start,
                &adj,
                &raw_edges,
                &mut visited,
                &mut on_stack,
                &mut reversed_raw,
            );
        }
    }

    let mut reversed_edge_ids = BTreeSet::new();
    let mut dag_map: BTreeMap<(usize, usize), (i32, i32)> = BTreeMap::new();
    for (ei, e) in raw_edges.iter().enumerate() {
        let (tail, head) = if reversed_raw[ei] {
            for &id in &e.orig_ids {
                reversed_edge_ids.insert(id);
            }
            (e.v, e.u)
        } else {
            (e.u, e.v)
        };
        let entry = dag_map.entry((tail, head)).or_insert((0, 0));
        entry.0 = entry.0.max(e.minlen);
        entry.1 = entry.1.saturating_add(e.weight);
    }

    // Check if local cluster ranking (`collapse_cluster` + `interclust1` in `rank.c` & `class1.c`) applies.
    let clusterrank_global = graph
        .graph_attrs
        .get("clusterrank")
        .is_some_and(|s| s.trim().eq_ignore_ascii_case("global"));
    let mut dense_cluster: Vec<Option<SubgraphId>> = vec![None; n_dense];
    if !clusterrank_global {
        for subg in &graph.subgraphs {
            if subg.is_cluster {
                for &nid in &subg.nodes {
                    let d = leader_to_dense[&uf.find(nid.0 as usize)];
                    dense_cluster[d] = Some(subg.id);
                }
            }
        }
    }
    let has_clusters = dense_cluster.iter().any(Option::is_some);

    let mut local_cluster_rank = vec![0i32; n_dense];
    let mut clust_uf = UnionFind::new(n_dense);

    if has_clusters {
        // 1. Rank each cluster locally (`dot1_rank` in `rank.c:330`).
        let mut clust_nodes_map: BTreeMap<SubgraphId, Vec<usize>> = BTreeMap::new();
        for (d, &cid_opt) in dense_cluster.iter().enumerate() {
            if let Some(cid) = cid_opt {
                clust_nodes_map.entry(cid).or_default().push(d);
            }
        }
        for cnodes in clust_nodes_map.values() {
            if cnodes.is_empty() {
                continue;
            }
            let mut local_idx = BTreeMap::new();
            for (li, &d) in cnodes.iter().enumerate() {
                local_idx.insert(d, li);
            }
            let mut c_edges = Vec::new();
            for (&(u, v), &(minlen, weight)) in &dag_map {
                if let (Some(&lu), Some(&lv)) = (local_idx.get(&u), local_idx.get(&v))
                    && lu != lv
                {
                    c_edges.push(NsEdge {
                        tail: lu,
                        head: lv,
                        minlen,
                        weight,
                        cutvalue: 0,
                        tree_index: None,
                    });
                }
            }
            let c_ranks = network_simplex_solve(cnodes.len(), &mut c_edges, 0);
            let min_cr = c_ranks.iter().copied().min().unwrap_or(0);
            let mut leader = cnodes[0];
            for (li, &d) in cnodes.iter().enumerate() {
                local_cluster_rank[d] = c_ranks[li] - min_cr;
                leader = clust_uf.union(leader, d);
            }
        }
    }

    // Map each cluster-collapsed leader to a global ranking node `0..n_global`.
    let mut global_map = BTreeMap::new();
    let mut global_leaders = Vec::new();
    for d in 0..n_dense {
        let gl = clust_uf.find(d);
        if let std::collections::btree_map::Entry::Vacant(e) = global_map.entry(gl) {
            let idx = global_leaders.len();
            e.insert(idx);
            global_leaders.push(gl);
        }
    }
    let n_global_real = global_leaders.len();
    let mut total_global_nodes = n_global_real;
    let mut global_edges: Vec<NsEdge> = Vec::new();
    let mut direct_global_pair: BTreeMap<(usize, usize), usize> = BTreeMap::new();

    for (&(u, v), &(minlen, weight)) in &dag_map {
        let gu = global_map[&clust_uf.find(u)];
        let gv = global_map[&clust_uf.find(v)];
        if gu == gv {
            continue;
        }
        if has_clusters && (dense_cluster[u].is_some() || dense_cluster[v].is_some()) {
            // `interclust1` (`class1.c:28`): virtual slack node `sn` with `CL_BACK = 10` on tail leg.
            let t_rank = local_cluster_rank[u];
            let h_rank = local_cluster_rank[v];
            let offset = minlen + t_rank - h_rank;
            let (t_len, h_len) = if offset > 0 {
                (0, offset)
            } else {
                (-offset, 0)
            };
            let sn = total_global_nodes;
            total_global_nodes += 1;
            global_edges.push(NsEdge {
                tail: sn,
                head: gu,
                minlen: t_len,
                weight: 10 * weight.max(1),
                cutvalue: 0,
                tree_index: None,
            });
            global_edges.push(NsEdge {
                tail: sn,
                head: gv,
                minlen: h_len,
                weight: weight.max(1),
                cutvalue: 0,
                tree_index: None,
            });
        } else if let Some(&ei) = direct_global_pair.get(&(gu, gv)) {
            global_edges[ei].minlen = global_edges[ei].minlen.max(minlen);
            global_edges[ei].weight = global_edges[ei].weight.saturating_add(weight);
        } else {
            let ei = global_edges.len();
            direct_global_pair.insert((gu, gv), ei);
            global_edges.push(NsEdge {
                tail: gu,
                head: gv,
                minlen,
                weight,
                cutvalue: 0,
                tree_index: None,
            });
        }
    }

    // Build weakly connected components of `0..total_global_nodes` and run Network Simplex on each component.
    let mut undir_adj: Vec<Vec<usize>> = vec![Vec::new(); total_global_nodes];
    for e in &global_edges {
        undir_adj[e.tail].push(e.head);
        undir_adj[e.head].push(e.tail);
    }

    let mut comp_id = vec![usize::MAX; total_global_nodes];
    let mut components: Vec<Vec<usize>> = Vec::new();
    for i in 0..total_global_nodes {
        if comp_id[i] != usize::MAX {
            continue;
        }
        let cid = components.len();
        let mut comp = Vec::new();
        let mut q = VecDeque::new();
        comp_id[i] = cid;
        q.push_back(i);
        while let Some(u) = q.pop_front() {
            comp.push(u);
            for &v in &undir_adj[u] {
                if comp_id[v] == usize::MAX {
                    comp_id[v] = cid;
                    q.push_back(v);
                }
            }
        }
        comp.sort_unstable();
        components.push(comp);
    }

    let mut global_ranks = vec![0i32; total_global_nodes];

    for comp in &components {
        if comp.len() <= 1 {
            global_ranks[comp[0]] = 0;
            continue;
        }
        let mut local_of = BTreeMap::new();
        for (li, &gnode) in comp.iter().enumerate() {
            local_of.insert(gnode, li);
        }
        let mut ns_edges = Vec::new();
        for e in &global_edges {
            if comp_id[e.tail] == comp_id[comp[0]] {
                ns_edges.push(NsEdge {
                    tail: local_of[&e.tail],
                    head: local_of[&e.head],
                    minlen: e.minlen,
                    weight: e.weight,
                    cutvalue: 0,
                    tree_index: None,
                });
            }
        }

        let balance_mode = if has_clusters { 0 } else { 1 };
        let local_ranks = network_simplex_solve(comp.len(), &mut ns_edges, balance_mode);
        for (li, &gnode) in comp.iter().enumerate() {
            global_ranks[gnode] = local_ranks[li];
        }
    }

    // Write computed ranks back to original graph nodes and normalize minimum rank to 0.
    let mut min_final_rank = i32::MAX;
    for i in 0..n_orig {
        let leader = uf.find(i);
        let d = leader_to_dense[&leader];
        let gl = global_map[&clust_uf.find(d)];
        let r = global_ranks[gl] + local_cluster_rank[d];
        graph.nodes[i].rank = r;
        min_final_rank = min_final_rank.min(r);
    }
    if min_final_rank != 0 && min_final_rank != i32::MAX {
        for node in &mut graph.nodes {
            node.rank -= min_final_rank;
        }
    }

    reversed_edge_ids
}

/// Solves $\min \sum w_e (\text{rank}(\text{head}) - \text{rank}(\text{tail}))$
/// subject to $\text{rank}(\text{head}) - \text{rank}(\text{tail}) \ge \text{minlen}(e)$
/// using Graphviz's Network Simplex algorithm (`lib/common/ns.c`).
fn network_simplex_solve(n_nodes: usize, edges: &mut [NsEdge], balance_mode: u8) -> Vec<i32> {
    if n_nodes == 0 {
        return Vec::new();
    }
    if n_nodes == 1 || edges.is_empty() {
        return vec![0; n_nodes];
    }

    let mut out_edges: Vec<Vec<usize>> = vec![Vec::new(); n_nodes];
    let mut in_edges: Vec<Vec<usize>> = vec![Vec::new(); n_nodes];
    for (ei, e) in edges.iter().enumerate() {
        out_edges[e.tail].push(ei);
        in_edges[e.head].push(ei);
    }

    // 1. Initial feasible ranking via topological sort (`init_rank` in `ns.c`).
    let mut rank = vec![0i32; n_nodes];
    let mut in_deg: Vec<usize> = (0..n_nodes).map(|u| in_edges[u].len()).collect();
    let mut q: VecDeque<usize> = (0..n_nodes).filter(|&u| in_deg[u] == 0).collect();
    let mut processed = 0usize;

    while let Some(u) = q.pop_front() {
        processed += 1;
        let mut r = 0i32;
        for &ei in &in_edges[u] {
            let tail = edges[ei].tail;
            r = r.max(rank[tail] + edges[ei].minlen);
        }
        rank[u] = r;
        for &ei in &out_edges[u] {
            let head = edges[ei].head;
            in_deg[head] = in_deg[head].saturating_sub(1);
            if in_deg[head] == 0 {
                q.push_back(head);
            }
        }
    }

    // Safety fallback if any cycle slipped through: Bellman-Ford relaxation.
    if processed < n_nodes {
        for _ in 0..n_nodes {
            let mut changed = false;
            for e in edges.iter() {
                if rank[e.head] < rank[e.tail] + e.minlen {
                    rank[e.head] = rank[e.tail] + e.minlen;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    // Tighten pure slack nodes (`make_edge_pairs` in `position.c:344` and `interclust1` in `class1.c:48`)
    for u in 0..n_nodes {
        if in_edges[u].is_empty()
            && !out_edges[u].is_empty()
            && edges[out_edges[u][0]].weight > 0
            && (balance_mode == 2 || out_edges[u].len() == 2)
            && let Some(max_r) = out_edges[u]
                .iter()
                .map(|&ei| rank[edges[ei].head] - edges[ei].minlen)
                .min()
        {
            rank[u] = max_r;
        }
    }

    // 2. Construct initial tight spanning tree (`feasible_tree` in `ns.c`).
    let mut tree_edges: Vec<usize> = Vec::with_capacity(n_nodes - 1);
    let mut tree_adj: Vec<Vec<usize>> = vec![Vec::new(); n_nodes];
    let mut in_tree = vec![false; n_nodes];

    // Grow tight tree from node 0; whenever tight growth stalls, find the minimum-slack edge
    // connecting the current tree to an outside node and shift the tree's ranks by ±slack.
    in_tree[0] = true;
    let mut tree_nodes = vec![0usize];

    while tree_nodes.len() < n_nodes {
        // DFS/BFS along zero-slack edges from nodes already in `tree_nodes`.
        let mut idx = 0;
        while idx < tree_nodes.len() {
            let u = tree_nodes[idx];
            idx += 1;
            for &ei in &out_edges[u] {
                let v = edges[ei].head;
                if !in_tree[v] && (rank[v] - rank[u] - edges[ei].minlen) == 0 {
                    in_tree[v] = true;
                    edges[ei].tree_index = Some(tree_edges.len());
                    tree_edges.push(ei);
                    tree_adj[u].push(ei);
                    tree_adj[v].push(ei);
                    tree_nodes.push(v);
                }
            }
            for &ei in &in_edges[u] {
                let v = edges[ei].tail;
                if !in_tree[v] && (rank[u] - rank[v] - edges[ei].minlen) == 0 {
                    in_tree[v] = true;
                    edges[ei].tree_index = Some(tree_edges.len());
                    tree_edges.push(ei);
                    tree_adj[u].push(ei);
                    tree_adj[v].push(ei);
                    tree_nodes.push(v);
                }
            }
        }

        if tree_nodes.len() == n_nodes {
            break;
        }

        // Find non-tree edge with minimum slack connecting `in_tree` and `!in_tree`.
        let mut best_ei: Option<usize> = None;
        let mut best_slack = i32::MAX;
        for (ei, e) in edges.iter().enumerate() {
            if in_tree[e.tail] == in_tree[e.head] {
                continue;
            }
            let slack = (rank[e.head] - rank[e.tail]) - e.minlen;
            if slack < best_slack {
                best_slack = slack;
                best_ei = Some(ei);
                if slack == 0 {
                    break;
                }
            }
        }

        if let Some(ei) = best_ei {
            let e_tail_in = in_tree[edges[ei].tail];
            // Adjust ranks of nodes in `tree_nodes` so `ei` becomes tight (`slack == 0`).
            let delta = if e_tail_in { best_slack } else { -best_slack };
            if delta != 0 {
                for &u in &tree_nodes {
                    rank[u] += delta;
                }
            }
            let new_node = if e_tail_in {
                edges[ei].head
            } else {
                edges[ei].tail
            };
            in_tree[new_node] = true;
            edges[ei].tree_index = Some(tree_edges.len());
            tree_edges.push(ei);
            tree_adj[edges[ei].tail].push(ei);
            tree_adj[edges[ei].head].push(ei);
            tree_nodes.push(new_node);
        } else {
            // Disconnected fallback (should not happen for a connected component).
            if let Some(unreached) = (0..n_nodes).find(|&u| !in_tree[u]) {
                in_tree[unreached] = true;
                tree_nodes.push(unreached);
            } else {
                break;
            }
        }
    }

    // 3. Initialize DFS post-order ranges (`low`, `lim`, `par`) and cut values (`init_cutvalues`).
    let mut low = vec![0i32; n_nodes];
    let mut lim = vec![0i32; n_nodes];
    let mut par: Vec<Option<usize>> = vec![None; n_nodes];

    recompute_tree_dfs_and_cutvalues(
        n_nodes, edges, &out_edges, &in_edges, &tree_adj, &mut low, &mut lim, &mut par,
    );

    // 4. Network Simplex pivot loop (`leave_edge` -> `enter_edge` -> `update`).
    let mut search_idx = 0usize;
    for _iter in 0..NS_MAX_ITER {
        // `leave_edge`: cyclically scan `tree_edges` for up to `NS_SEARCH_SIZE` negative cut-value edges.
        let mut leaving_tree_pos: Option<usize> = None;
        let mut best_cutval = 0i32;
        let mut found_neg = 0usize;
        let n_tree = tree_edges.len();
        if n_tree == 0 {
            break;
        }

        for offset in 0..n_tree {
            let pos = (search_idx + offset) % n_tree;
            let ei = tree_edges[pos];
            let cv = edges[ei].cutvalue;
            if cv < 0 {
                if leaving_tree_pos.is_none() || cv < best_cutval {
                    leaving_tree_pos = Some(pos);
                    best_cutval = cv;
                }
                found_neg += 1;
                if found_neg >= NS_SEARCH_SIZE {
                    search_idx = (pos + 1) % n_tree;
                    break;
                }
            }
        }

        let Some(leave_pos) = leaving_tree_pos else {
            break;
        };
        let leave_ei = tree_edges[leave_pos];

        // `enter_edge`: identify the child subtree (`low[v]..=lim[v]`) of `leave_ei`.
        let u_leave = edges[leave_ei].tail;
        let v_leave = edges[leave_ei].head;
        let (sub_root, tail_in_sub) = if lim[u_leave] < lim[v_leave] {
            (u_leave, true)
        } else {
            (v_leave, false)
        };
        let sub_low = low[sub_root];
        let sub_lim = lim[sub_root];

        // Search for a non-tree edge crossing in the opposite direction that reduces rank length:
        // If `tail_in_sub` is true, removing `leave_ei` and decreasing ranks of `sub_root`'s subtree
        // requires an entering edge whose `head` is inside `sub_root`'s subtree and `tail` is outside!
        // If `tail_in_sub` is false (`head` of `leave_ei` is in `sub_root`'s subtree), increasing ranks
        // of `sub_root`'s subtree requires an entering edge whose `tail` is inside and `head` is outside!
        let mut enter_ei: Option<usize> = None;
        let mut best_enter_slack = i32::MAX;

        for (ei, e) in edges.iter().enumerate() {
            if e.tree_index.is_some() {
                continue;
            }
            let t_in = sub_low <= lim[e.tail] && lim[e.tail] <= sub_lim;
            let h_in = sub_low <= lim[e.head] && lim[e.head] <= sub_lim;
            let valid_crossing = if tail_in_sub {
                !t_in && h_in
            } else {
                t_in && !h_in
            };
            if valid_crossing {
                let slack = (rank[e.head] - rank[e.tail]) - e.minlen;
                if slack < best_enter_slack || enter_ei.is_none() {
                    best_enter_slack = slack;
                    enter_ei = Some(ei);
                    if slack == 0 {
                        break;
                    }
                }
            }
        }

        let Some(f_ei) = enter_ei else {
            // No valid entering edge (should not occur when cutvalue < 0, but guard against loop).
            edges[leave_ei].cutvalue = 0;
            continue;
        };

        // `update`: adjust ranks of `sub_root` subtree by `±best_enter_slack`.
        if best_enter_slack > 0 {
            let delta = if tail_in_sub {
                -best_enter_slack
            } else {
                best_enter_slack
            };
            for node in 0..n_nodes {
                if sub_low <= lim[node] && lim[node] <= sub_lim {
                    rank[node] += delta;
                }
            }
        }

        // Exchange `leave_ei` with `f_ei` in the spanning tree.
        edges[leave_ei].tree_index = None;
        edges[leave_ei].cutvalue = 0;
        edges[f_ei].tree_index = Some(leave_pos);
        tree_edges[leave_pos] = f_ei;

        tree_adj[u_leave].retain(|&x| x != leave_ei);
        tree_adj[v_leave].retain(|&x| x != leave_ei);
        tree_adj[edges[f_ei].tail].push(f_ei);
        tree_adj[edges[f_ei].head].push(f_ei);

        recompute_tree_dfs_and_cutvalues(
            n_nodes, edges, &out_edges, &in_edges, &tree_adj, &mut low, &mut lim, &mut par,
        );
    }

    // 5. Optional `LR_balance` (`ns.c:775`, `balance_mode == 2`) for auxiliary coordinate graphs:
    // For any tree edge `e` with `cutvalue == 0`, find the minimum-slack entering edge `f` and shift by `delta / 2`.
    if balance_mode == 2 {
        for &leave_ei in &tree_edges {
            if edges[leave_ei].cutvalue != 0 {
                continue;
            }
            let u_leave = edges[leave_ei].tail;
            let v_leave = edges[leave_ei].head;
            let (sub_root, tail_in_sub) = if lim[u_leave] < lim[v_leave] {
                (u_leave, true)
            } else {
                (v_leave, false)
            };
            let sub_low = low[sub_root];
            let sub_lim = lim[sub_root];

            let mut best_enter_slack = i32::MAX;
            for e in edges.iter() {
                if e.tree_index.is_some() {
                    continue;
                }
                let t_in = sub_low <= lim[e.tail] && lim[e.tail] <= sub_lim;
                let h_in = sub_low <= lim[e.head] && lim[e.head] <= sub_lim;
                let valid_crossing = if tail_in_sub {
                    !t_in && h_in
                } else {
                    t_in && !h_in
                };
                if valid_crossing {
                    let slack = (rank[e.head] - rank[e.tail]) - e.minlen;
                    if slack < best_enter_slack {
                        best_enter_slack = slack;
                    }
                }
            }
            if best_enter_slack > 1 && best_enter_slack < i32::MAX {
                let half = best_enter_slack / 2;
                let delta = if tail_in_sub { -half } else { half };
                for node in 0..n_nodes {
                    if sub_low <= lim[node] && lim[node] <= sub_lim {
                        rank[node] += delta;
                    }
                }
            }
        }
    }

    // Normalize ranks so min_rank == 0, and optionally run `TB_balance` (`balance_mode == 1`).
    let min_r = rank.iter().copied().min().unwrap_or(0);
    for r in &mut rank {
        *r -= min_r;
    }

    if balance_mode == 1 {
        let max_r = rank.iter().copied().max().unwrap_or(0).max(0) as usize;
        let mut rank_counts = vec![0usize; max_r + 1];
        for &r in &rank {
            rank_counts[r as usize] += 1;
        }
        for u in 0..n_nodes {
            if in_edges[u].is_empty() || out_edges[u].is_empty() {
                continue;
            }
            let in_wt: i32 = in_edges[u].iter().map(|&ei| edges[ei].weight).sum();
            let out_wt: i32 = out_edges[u].iter().map(|&ei| edges[ei].weight).sum();
            if in_wt == out_wt {
                let low_r = in_edges[u]
                    .iter()
                    .map(|&ei| rank[edges[ei].tail] + edges[ei].minlen)
                    .max()
                    .unwrap_or(0)
                    .max(0) as usize;
                let high_r = out_edges[u]
                    .iter()
                    .map(|&ei| rank[edges[ei].head] - edges[ei].minlen)
                    .min()
                    .unwrap_or(max_r as i32)
                    .max(0) as usize;
                if low_r < high_r && high_r <= max_r {
                    let mut choice = rank[u] as usize;
                    for candidate in low_r..=high_r {
                        if rank_counts[candidate] < rank_counts[choice] {
                            choice = candidate;
                        }
                    }
                    rank_counts[rank[u] as usize] -= 1;
                    rank_counts[choice] += 1;
                    rank[u] = choice as i32;
                }
            }
        }
    }

    rank
}

/// Computes post-order intervals `[low[v], lim[v]]` on the spanning tree rooted at `0`
/// and computes exact cut values bottom-up (`dfs_range` + `dfs_cutval` + `x_cutval` in `ns.c`).
#[allow(clippy::too_many_arguments)]
fn recompute_tree_dfs_and_cutvalues(
    n_nodes: usize,
    edges: &mut [NsEdge],
    out_edges: &[Vec<usize>],
    in_edges: &[Vec<usize>],
    tree_adj: &[Vec<usize>],
    low: &mut [i32],
    lim: &mut [i32],
    par: &mut [Option<usize>],
) {
    par.fill(None);
    let mut order_post = Vec::with_capacity(n_nodes);
    let mut counter = 1i32;

    #[allow(clippy::too_many_arguments)]
    fn dfs_tree(
        u: usize,
        parent_edge: Option<usize>,
        edges: &[NsEdge],
        tree_adj: &[Vec<usize>],
        low: &mut [i32],
        lim: &mut [i32],
        par: &mut [Option<usize>],
        counter: &mut i32,
        order_post: &mut Vec<usize>,
    ) {
        par[u] = parent_edge;
        let start = *counter;
        for &ei in &tree_adj[u] {
            if Some(ei) == parent_edge {
                continue;
            }
            let child = if edges[ei].tail == u {
                edges[ei].head
            } else {
                edges[ei].tail
            };
            dfs_tree(
                child,
                Some(ei),
                edges,
                tree_adj,
                low,
                lim,
                par,
                counter,
                order_post,
            );
        }
        low[u] = start;
        lim[u] = *counter;
        *counter += 1;
        order_post.push(u);
    }

    dfs_tree(
        0,
        None,
        edges,
        tree_adj,
        low,
        lim,
        par,
        &mut counter,
        &mut order_post,
    );

    // Compute cut values in bottom-up post-order (`x_cutval` / `x_val` in `ns.c`).
    for &v in &order_post {
        let Some(f_ei) = par[v] else {
            continue;
        };
        let dir = if edges[f_ei].tail == v { 1i32 } else { -1i32 };
        let mut sum = 0i32;

        for &ei in out_edges[v].iter().chain(in_edges[v].iter()) {
            let e = &edges[ei];
            let other = if e.tail == v { e.head } else { e.tail };
            let in_subtree = low[v] <= lim[other] && lim[other] <= lim[v];
            let (f_cross, mut rv) = if !in_subtree {
                (true, e.weight)
            } else {
                let base = if e.tree_index.is_some() {
                    e.cutvalue
                } else {
                    0
                };
                (false, base - e.weight)
            };
            let mut d = if dir > 0 {
                if e.head == v { 1 } else { -1 }
            } else if e.tail == v {
                1
            } else {
                -1
            };
            if f_cross {
                d = -d;
            }
            if d < 0 {
                rv = -rv;
            }
            sum = sum.saturating_add(rv);
        }
        edges[f_ei].cutvalue = sum;
    }
}

// ============================================================================
// Stage 3: Crossing Minimization (`mincross.c`)
// ============================================================================

#[derive(Debug, Clone)]
struct VNode {
    real_node: Option<NodeId>,
    cluster: Option<SubgraphId>,
    rank: i32,
    order: usize,
    /// Left extent along the within-rank order axis (in points).
    lw: f64,
    /// Right extent along the within-rank order axis (in points).
    rw: f64,
    /// Extent along the rank axis (in points).
    ht: f64,
    in_edges: Vec<usize>,
    out_edges: Vec<usize>,
    flat_out: Vec<usize>,
    flat_in: Vec<usize>,
    mval: f64,
    /// Computed coordinate along the within-rank order axis (in points).
    order_coord: f64,
    /// Computed coordinate along the rank axis (in points).
    rank_coord: f64,
}

#[derive(Debug, Clone)]
struct VEdge {
    tail: usize,
    head: usize,
    weight: i32,
    xpenalty: i32,
    tail_port_x: i32,
    head_port_x: i32,
}

#[derive(Debug, Clone)]
struct VirtualGraph {
    nodes: Vec<VNode>,
    edges: Vec<VEdge>,
    ranks: Vec<Vec<usize>>,
    /// For each original `EdgeId`, the chain of virtual node indices (`[tail_vn, v1, ..., head_vn]`).
    edge_chains: BTreeMap<EdgeId, Vec<usize>>,
}

/// Builds the rank-layered graph with virtual nodes for edges spanning $> 1$ rank (`class2` in `class2.c`).
fn build_virtual_graph(
    graph: &Graph,
    reversed_edges: &BTreeSet<EdgeId>,
    rankdir: RankDir,
    nodesep_pt: f64,
) -> VirtualGraph {
    let max_rank = graph.nodes.iter().map(|n| n.rank).max().unwrap_or(0).max(0) as usize;

    // Determine innermost cluster containing each real node.
    let mut node_cluster: Vec<Option<SubgraphId>> = vec![None; graph.nodes.len()];
    fn assign_node_clusters(
        graph: &Graph,
        subg_id: SubgraphId,
        node_cluster: &mut [Option<SubgraphId>],
    ) {
        let subg = &graph.subgraphs[subg_id.0 as usize];
        if subg.is_cluster {
            for &nid in &subg.nodes {
                node_cluster[nid.0 as usize] = Some(subg_id);
            }
        }
        for &child_id in &subg.subgraphs {
            assign_node_clusters(graph, child_id, node_cluster);
        }
    }
    let child_subgs: BTreeSet<SubgraphId> = graph
        .subgraphs
        .iter()
        .flat_map(|s| s.subgraphs.iter().copied())
        .collect();
    for subg in &graph.subgraphs {
        if !child_subgs.contains(&subg.id) {
            assign_node_clusters(graph, subg.id, &mut node_cluster);
        }
    }

    let mut vnodes = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        let (span_pt, thick_pt) = if rankdir.is_flipped() {
            (node.height * POINTS_PER_INCH, node.width * POINTS_PER_INCH)
        } else {
            (node.width * POINTS_PER_INCH, node.height * POINTS_PER_INCH)
        };
        // Account for self-loops adding extra right width (`make_LR_constraints` in `position.c`).
        let self_loop_count = graph
            .edges
            .iter()
            .filter(|e| e.tail == node.id && e.head == node.id)
            .count();
        let extra_rw = (self_loop_count as f64) * 18.0;

        vnodes.push(VNode {
            real_node: Some(node.id),
            cluster: node_cluster[node.id.0 as usize],
            rank: node.rank,
            order: node.order,
            lw: span_pt / 2.0,
            rw: span_pt / 2.0 + extra_rw,
            ht: thick_pt,
            in_edges: Vec::new(),
            out_edges: Vec::new(),
            flat_out: Vec::new(),
            flat_in: Vec::new(),
            mval: -1e9,
            order_coord: 0.0,
            rank_coord: 0.0,
        });
    }

    let mut vedges = Vec::new();
    let mut edge_chains = BTreeMap::new();

    for edge in &graph.edges {
        if edge.tail == edge.head {
            continue;
        }
        let u_orig = edge.tail.0 as usize;
        let v_orig = edge.head.0 as usize;
        let r_u = vnodes[u_orig].rank;
        let r_v = vnodes[v_orig].rank;
        let base_weight = (parse_attr_f64(&edge.attrs, "weight", 1.0).round() as i32).max(1);

        if r_u == r_v {
            // Flat edge within the same rank.
            let (tail, head) = if reversed_edges.contains(&edge.id) {
                (v_orig, u_orig)
            } else {
                (u_orig, v_orig)
            };
            let vei = vedges.len();
            vedges.push(VEdge {
                tail,
                head,
                weight: base_weight,
                xpenalty: 1,
                tail_port_x: 0,
                head_port_x: 0,
            });
            vnodes[tail].flat_out.push(vei);
            vnodes[head].flat_in.push(vei);
            edge_chains.insert(edge.id, vec![u_orig, v_orig]);
            continue;
        }

        let resolve_endpoint_port_x = |node_idx: usize, port: &gv_core::Port| -> i32 {
            let pt =
                resolve_port_offset(&graph.nodes[node_idx], port, &graph.name, rankdir.as_str());
            let axis_off = if rankdir.is_flipped() { pt.y } else { pt.x };
            (axis_off.round() as i32) + compass_port_offset(port.compass.as_deref())
        };

        // Orient from lower rank to higher rank.
        let (low_node, high_node, low_port, high_port) = if r_u < r_v {
            (
                u_orig,
                v_orig,
                resolve_endpoint_port_x(u_orig, &edge.tail_port),
                resolve_endpoint_port_x(v_orig, &edge.head_port),
            )
        } else {
            (
                v_orig,
                u_orig,
                resolve_endpoint_port_x(v_orig, &edge.head_port),
                resolve_endpoint_port_x(u_orig, &edge.tail_port),
            )
        };
        let r_low = vnodes[low_node].rank;
        let r_high = vnodes[high_node].rank;
        let span = (r_high - r_low) as usize;

        let shared_cluster = if vnodes[low_node].cluster == vnodes[high_node].cluster {
            vnodes[low_node].cluster
        } else {
            None
        };

        // Measure edge label dimensions (`label_vnode` in `class2.c:25`).
        let edge_label_dimen = edge
            .attrs
            .get("label")
            .filter(|s| !s.trim().is_empty())
            .map(|lbl| {
                let font_name = edge
                    .attrs
                    .get("fontname")
                    .map(String::as_str)
                    .unwrap_or(DEFAULT_FONT_NAME);
                let font_size = parse_attr_f64(&edge.attrs, "fontsize", DEFAULT_FONT_SIZE);
                if is_html_label(lbl, ShapeKind::Plain)
                    && let Some(tbl) = parse_html_table(lbl, "", &graph.name, font_name, font_size)
                {
                    Point::new(tbl.bbox.width(), tbl.bbox.height())
                } else {
                    let layout = measure_multiline_label_with_context(
                        lbl,
                        &LabelSubstContext::default(),
                        font_name,
                        font_size,
                    );
                    Point::new(layout.width, layout.height.floor())
                }
            });

        let mut chain = Vec::with_capacity(span + 1);
        chain.push(low_node);

        for step in 1..span {
            let r_inter = r_low + step as i32;
            let is_mid = step == span / 2;
            let (lw, rw, ht) = if is_mid && let Some(dimen) = edge_label_dimen {
                if rankdir.is_flipped() {
                    (nodesep_pt, dimen.y, dimen.x)
                } else {
                    (nodesep_pt, dimen.x, dimen.y)
                }
            } else {
                // `plain_vnode` in `class2.c:48`
                (nodesep_pt / 2.0, nodesep_pt / 2.0, 1.0)
            };
            let vn_idx = vnodes.len();
            vnodes.push(VNode {
                real_node: None,
                cluster: shared_cluster,
                rank: r_inter,
                order: 0,
                lw,
                rw,
                ht,
                in_edges: Vec::new(),
                out_edges: Vec::new(),
                flat_out: Vec::new(),
                flat_in: Vec::new(),
                mval: -1e9,
                order_coord: 0.0,
                rank_coord: 0.0,
            });
            chain.push(vn_idx);
        }
        chain.push(high_node);

        // Create virtual edges along the chain with `virtual_weight` multipliers (`mincross.c`).
        for idx in 0..span {
            let t_vn = chain[idx];
            let h_vn = chain[idx + 1];
            let t_virt = vnodes[t_vn].real_node.is_none();
            let h_virt = vnodes[h_vn].real_node.is_none();
            let mult = match (t_virt, h_virt) {
                (true, true) => 4,                  // C_VV
                (true, false) | (false, true) => 2, // C_VS
                (false, false) => 1,                // C_EE
            };
            let vei = vedges.len();
            vedges.push(VEdge {
                tail: t_vn,
                head: h_vn,
                weight: base_weight * mult,
                xpenalty: mult,
                tail_port_x: if idx == 0 { low_port } else { 0 },
                head_port_x: if idx + 1 == span { high_port } else { 0 },
            });
            vnodes[t_vn].out_edges.push(vei);
            vnodes[h_vn].in_edges.push(vei);
        }

        // Store chain in tail -> head direction of the original edge.
        if r_u > r_v {
            chain.reverse();
        }
        edge_chains.insert(edge.id, chain);
    }

    let mut ranks = vec![Vec::new(); max_rank + 1];
    for (idx, vn) in vnodes.iter().enumerate() {
        ranks[vn.rank as usize].push(idx);
    }

    VirtualGraph {
        nodes: vnodes,
        edges: vedges,
        ranks,
        edge_chains,
    }
}

fn compass_port_offset(compass: Option<&str>) -> i32 {
    match compass.unwrap_or("").trim().to_ascii_lowercase().as_str() {
        "nw" | "sw" | "w" => -16,
        "ne" | "se" | "e" => 16,
        _ => 0,
    }
}

/// Runs Graphviz's crossing minimization (`dot_mincross` in `mincross.c`):
/// 1. Initial BFS ordering (`build_ranks`) + flat-edge topological reorder (`flat_reorder`).
/// 2. Alternating down/up weighted-median sweeps (`medians` + `reorder`) + `transpose`.
/// 3. Saves/restores best ordering and runs final `transpose(false)`.
fn minimize_crossings(graph: &Graph, vgraph: &mut VirtualGraph, rankdir: RankDir) {
    let n_ranks = vgraph.ranks.len();
    if n_ranks == 0 {
        return;
    }

    let mut best_ranks = vgraph.ranks.clone();
    let mut best_cross = i64::MAX;

    // 3-pass crossing minimization matching `mincross` in `lib/dotgen/mincross.c:695`:
    // pass 0: forward BFS `build_ranks(0)` + 4 mincross_step iterations
    // pass 1: reverse BFS `build_ranks(1)` + 4 mincross_step iterations
    // pass 2: restore best + up to `MINCROSS_MAX_ITER` (24) mincross_step iterations
    for pass in 0..=2 {
        let max_this_pass = if pass <= 1 {
            build_initial_ranks(vgraph, pass, rankdir);
            flat_reorder_ranks(graph, vgraph, rankdir);
            let c0 = total_crossings(vgraph);
            if c0 > 0 {
                transpose_all(vgraph, false, rankdir);
            }
            let c_init = total_crossings(vgraph);
            if (pass == 0 && c_init <= best_cross) || (pass == 1 && c_init < best_cross) {
                best_cross = c_init;
                best_ranks = vgraph.ranks.clone();
            }
            4.min(MINCROSS_MAX_ITER)
        } else {
            restore_ranks(vgraph, &best_ranks);
            MINCROSS_MAX_ITER
        };

        let mut trying = 0usize;
        for iter in 0..max_this_pass {
            if best_cross == 0 || trying >= MINCROSS_MIN_QUIT {
                break;
            }
            trying += 1;

            let reverse = (iter % 4) > 1;
            if iter % 2 == 0 {
                for r in 1..n_ranks {
                    compute_medians(vgraph, r, r - 1, rankdir);
                    reorder_rank(vgraph, r, reverse, rankdir);
                }
            } else {
                for r in (0..n_ranks.saturating_sub(1)).rev() {
                    compute_medians(vgraph, r, r + 1, rankdir);
                    reorder_rank(vgraph, r, reverse, rankdir);
                }
            }
            transpose_all(vgraph, !reverse, rankdir);

            let cur_cross = total_crossings(vgraph);
            let accept = if pass == 1 {
                cur_cross < best_cross
            } else {
                cur_cross <= best_cross
            };
            if accept {
                if (cur_cross as f64) < MINCROSS_CONVERGENCE * (best_cross as f64) {
                    trying = 0;
                }
                best_cross = cur_cross;
                best_ranks = vgraph.ranks.clone();
            }
        }
        if best_cross == 0 {
            break;
        }
    }

    restore_ranks(vgraph, &best_ranks);
    if best_cross > 0 {
        transpose_all(vgraph, false, rankdir);
    }

    // Group nodes belonging to the same cluster contiguously within each rank if clusters exist.
    if graph.subgraphs.iter().any(|s| s.is_cluster) {
        group_clusters_in_ranks(vgraph);
        transpose_all_within_clusters(vgraph, rankdir);
    }
}

fn build_initial_ranks(vgraph: &mut VirtualGraph, pass: usize, rankdir: RankDir) {
    let n_vnodes = vgraph.nodes.len();
    let mut visited = vec![false; n_vnodes];
    for r in &mut vgraph.ranks {
        r.clear();
    }

    let has_clusters = vgraph.nodes.iter().any(|n| n.cluster.is_some());
    let mut start_nodes: Vec<usize> = (0..n_vnodes).collect();
    if has_clusters {
        if pass == 0 {
            start_nodes.sort_by_key(|&u| {
                (
                    usize::from(!vgraph.nodes[u].in_edges.is_empty()),
                    vgraph.nodes[u].rank,
                    u,
                )
            });
        } else {
            start_nodes.sort_by_key(|&u| {
                (
                    usize::from(!vgraph.nodes[u].out_edges.is_empty()),
                    std::cmp::Reverse(vgraph.nodes[u].rank),
                    u,
                )
            });
        }
    } else if pass == 0 {
        start_nodes.sort_by_key(|&u| (usize::from(!vgraph.nodes[u].in_edges.is_empty()), u));
    } else {
        start_nodes.sort_by_key(|&u| (usize::from(!vgraph.nodes[u].out_edges.is_empty()), u));
    }

    let mut q = VecDeque::new();
    let mut installed_clusters = BTreeSet::new();
    for start in start_nodes {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        q.push_back(start);
        while let Some(u) = q.pop_front() {
            if let Some(cid) = vgraph.nodes[u].cluster
                && installed_clusters.insert(cid)
            {
                // `install_cluster` (`mincross.c:1244`): install all nodes of cluster `cid` into their ranks now.
                let mut clust_nodes: Vec<usize> = (0..n_vnodes)
                    .filter(|&v| vgraph.nodes[v].cluster == Some(cid))
                    .collect();
                if pass == 0 {
                    clust_nodes.sort_by_key(|&v| (vgraph.nodes[v].rank, v));
                } else {
                    clust_nodes.sort_by_key(|&v| (std::cmp::Reverse(vgraph.nodes[v].rank), v));
                }
                for cn in clust_nodes {
                    visited[cn] = true;
                    let r = vgraph.nodes[cn].rank as usize;
                    let ord = vgraph.ranks[r].len();
                    vgraph.ranks[r].push(cn);
                    vgraph.nodes[cn].order = ord;

                    let edge_list = if pass == 0 {
                        vgraph.nodes[cn].out_edges.clone()
                    } else {
                        vgraph.nodes[cn].in_edges.clone()
                    };
                    for vei in edge_list {
                        let next = if pass == 0 {
                            vgraph.edges[vei].head
                        } else {
                            vgraph.edges[vei].tail
                        };
                        if !visited[next] {
                            visited[next] = true;
                            q.push_back(next);
                        }
                    }
                }
                continue;
            }
            if vgraph.nodes[u].cluster.is_some() {
                continue;
            }
            let r = vgraph.nodes[u].rank as usize;
            let ord = vgraph.ranks[r].len();
            vgraph.ranks[r].push(u);
            vgraph.nodes[u].order = ord;

            let edge_list = if pass == 0 {
                vgraph.nodes[u].out_edges.clone()
            } else {
                vgraph.nodes[u].in_edges.clone()
            };
            for vei in edge_list {
                let next = if pass == 0 {
                    vgraph.edges[vei].head
                } else {
                    vgraph.edges[vei].tail
                };
                if !visited[next] {
                    visited[next] = true;
                    q.push_back(next);
                }
            }
        }
    }

    if rankdir.is_flipped() {
        for r in 0..vgraph.ranks.len() {
            vgraph.ranks[r].reverse();
            for (ord, &vn) in vgraph.ranks[r].iter().enumerate() {
                vgraph.nodes[vn].order = ord;
            }
        }
    }
}

/// Reorders nodes within each rank to respect `ordering="out"`/`"in"` and flat edges (`flat_reorder` in `mincross.c`).
fn flat_reorder_ranks(graph: &Graph, vgraph: &mut VirtualGraph, rankdir: RankDir) {
    let ordering_out = graph
        .graph_attrs
        .get("ordering")
        .is_some_and(|s| s.trim().eq_ignore_ascii_case("out"));

    for r in 0..vgraph.ranks.len() {
        if vgraph.ranks[r].len() <= 1 {
            continue;
        }
        // Check if any node in rank `r` has flat edges or `ordering=out` constraints from `r - 1`.
        let has_flat = vgraph.ranks[r]
            .iter()
            .any(|&u| !vgraph.nodes[u].flat_out.is_empty() || !vgraph.nodes[u].flat_in.is_empty());
        if !has_flat && !ordering_out {
            continue;
        }

        let rank_nodes = vgraph.ranks[r].clone();
        let mut pos_in_rank = BTreeMap::new();
        for (idx, &u) in rank_nodes.iter().enumerate() {
            pos_in_rank.insert(u, idx);
        }

        let k = rank_nodes.len();
        let mut adj = vec![Vec::new(); k];
        let mut in_deg = vec![0usize; k];

        for (i, &u) in rank_nodes.iter().enumerate() {
            for &vei in &vgraph.nodes[u].flat_out {
                let v = vgraph.edges[vei].head;
                if let Some(&j) = pos_in_rank.get(&v)
                    && i != j
                {
                    // In `mincross.c:1346`, `if (!GD_flip(g)) LIST_REVERSE(&temprank);` places `u` before `v`
                    // for TB/BT, and `v` before `u` (smaller y -> larger y) for LR/RL.
                    if rankdir.is_flipped() {
                        adj[j].push(i);
                        in_deg[i] += 1;
                    } else {
                        adj[i].push(j);
                        in_deg[j] += 1;
                    }
                }
            }
        }

        let mut q: VecDeque<usize> = (0..k).filter(|&i| in_deg[i] == 0).collect();
        let mut sorted = Vec::with_capacity(k);
        while let Some(i) = q.pop_front() {
            sorted.push(rank_nodes[i]);
            for &j in &adj[i] {
                in_deg[j] -= 1;
                if in_deg[j] == 0 {
                    q.push_back(j);
                }
            }
        }
        if sorted.len() == k {
            vgraph.ranks[r] = sorted;
            for (ord, &vn) in vgraph.ranks[r].iter().enumerate() {
                vgraph.nodes[vn].order = ord;
            }
        }
    }
}

fn restore_ranks(vgraph: &mut VirtualGraph, saved: &[Vec<usize>]) {
    vgraph.ranks = saved.to_vec();
    for (r, rank_nodes) in vgraph.ranks.iter().enumerate() {
        for (ord, &vn) in rank_nodes.iter().enumerate() {
            vgraph.nodes[vn].rank = r as i32;
            vgraph.nodes[vn].order = ord;
        }
    }
}

/// Computes weighted medians for rank `r0` from adjacent rank `r1` (`medians` in `mincross.c`).
fn compute_medians(vgraph: &mut VirtualGraph, r0: usize, r1: usize, rankdir: RankDir) {
    let rank_nodes = vgraph.ranks[r0].clone();
    let mut vals = Vec::new();

    for &u in &rank_nodes {
        vals.clear();
        if r1 > r0 {
            for &vei in &vgraph.nodes[u].out_edges {
                let e = &vgraph.edges[vei];
                if e.xpenalty > 0 {
                    let head_ord = vgraph.nodes[e.head].order as i32;
                    vals.push(MC_SCALE * head_ord + e.head_port_x);
                }
            }
        } else {
            for &vei in &vgraph.nodes[u].in_edges {
                let e = &vgraph.edges[vei];
                if e.xpenalty > 0 {
                    let tail_ord = vgraph.nodes[e.tail].order as i32;
                    vals.push(MC_SCALE * tail_ord + e.tail_port_x);
                }
            }
        }

        vals.sort_unstable();
        let mval = match vals.len() {
            0 => -1e9,
            1 => f64::from(vals[0]),
            2 => f64::from(vals[0] + vals[1]) / 2.0,
            j if j % 2 == 1 => f64::from(vals[j / 2]),
            j => {
                // Weighted median (`medians` in `mincross.c`).
                let rm = j / 2;
                let lm = rm - 1;
                let rspan = vals[j - 1] - vals[rm];
                let lspan = vals[lm] - vals[0];
                if lspan == rspan {
                    f64::from(vals[lm] + vals[rm]) / 2.0
                } else {
                    (f64::from(vals[lm]) * f64::from(rspan)
                        + f64::from(vals[rm]) * f64::from(lspan))
                        / f64::from(lspan + rspan)
                }
            }
        };
        vgraph.nodes[u].mval = mval;
    }

    // Handle nodes with only flat edges (`flat_mval` in `mincross.c`).
    for &u in &rank_nodes {
        if vgraph.nodes[u].in_edges.is_empty() && vgraph.nodes[u].out_edges.is_empty() {
            if !vgraph.nodes[u].flat_in.is_empty() {
                if let Some(best_tail) = vgraph.nodes[u]
                    .flat_in
                    .iter()
                    .map(|&vei| vgraph.edges[vei].tail)
                    .max_by_key(|&t| vgraph.nodes[t].order)
                    && vgraph.nodes[best_tail].mval >= -1e8
                {
                    let step = if rankdir.is_flipped() { -1.0 } else { 1.0 };
                    vgraph.nodes[u].mval = vgraph.nodes[best_tail].mval + step;
                }
            } else if !vgraph.nodes[u].flat_out.is_empty()
                && let Some(best_head) = vgraph.nodes[u]
                    .flat_out
                    .iter()
                    .map(|&vei| vgraph.edges[vei].head)
                    .min_by_key(|&h| vgraph.nodes[h].order)
                && vgraph.nodes[best_head].mval > -1e8
            {
                let step = if rankdir.is_flipped() { 1.0 } else { -1.0 };
                vgraph.nodes[u].mval = vgraph.nodes[best_head].mval + step;
            }
        }
    }
}

/// Bubble-style median sort that skips fixed (`mval < -1e8`) nodes (`reorder` in `mincross.c`).
fn reorder_rank(vgraph: &mut VirtualGraph, r: usize, reverse: bool, rankdir: RankDir) {
    let n = vgraph.ranks[r].len();
    if n <= 1 {
        return;
    }

    for _ in 0..n {
        let mut lp = 0usize;
        while lp < n {
            while lp < n && vgraph.nodes[vgraph.ranks[r][lp]].mval < -1e8 {
                lp += 1;
            }
            if lp >= n {
                break;
            }
            let mut rp = lp + 1;
            let mut must_stay = false;
            while rp < n {
                let u = vgraph.ranks[r][lp];
                let v = vgraph.ranks[r][rp];
                if has_flat_constraint(vgraph, u, v, rankdir) {
                    must_stay = true;
                    break;
                }
                if vgraph.nodes[v].mval >= -1e8 {
                    break;
                }
                rp += 1;
            }
            if rp >= n {
                break;
            }
            if !must_stay {
                let u = vgraph.ranks[r][lp];
                let v = vgraph.ranks[r][rp];
                let p1 = vgraph.nodes[u].mval;
                let p2 = vgraph.nodes[v].mval;
                if p1 > p2 || (reverse && (p1 - p2).abs() < 1e-9) {
                    vgraph.ranks[r].swap(lp, rp);
                    vgraph.nodes[u].order = rp;
                    vgraph.nodes[v].order = lp;
                }
            }
            lp = rp;
        }
    }
}

fn has_flat_constraint(vgraph: &VirtualGraph, u: usize, v: usize, rankdir: RankDir) -> bool {
    if rankdir.is_flipped() {
        vgraph.nodes[v]
            .flat_out
            .iter()
            .any(|&vei| vgraph.edges[vei].head == u)
    } else {
        vgraph.nodes[u]
            .flat_out
            .iter()
            .any(|&vei| vgraph.edges[vei].head == v)
    }
}

/// Adjacent-exchange local search (`transpose` in `mincross.c`).
fn transpose_all(vgraph: &mut VirtualGraph, reverse: bool, rankdir: RankDir) {
    let n_ranks = vgraph.ranks.len();
    let mut improved = true;
    let mut pass_limit = 20usize;

    while improved && pass_limit > 0 {
        improved = false;
        pass_limit -= 1;
        for r in 0..n_ranks {
            let len = vgraph.ranks[r].len();
            if len <= 1 {
                continue;
            }
            for i in 0..(len - 1) {
                let v = vgraph.ranks[r][i];
                let w = vgraph.ranks[r][i + 1];
                if has_flat_constraint(vgraph, v, w, rankdir) {
                    continue;
                }
                let c0 = pair_in_cross(vgraph, v, w) + pair_out_cross(vgraph, v, w);
                let c1 = pair_in_cross(vgraph, w, v) + pair_out_cross(vgraph, w, v);
                if c1 < c0 || (c0 > 0 && reverse && c1 == c0) {
                    vgraph.ranks[r].swap(i, i + 1);
                    vgraph.nodes[v].order = i + 1;
                    vgraph.nodes[w].order = i;
                    if c1 < c0 {
                        improved = true;
                    }
                }
            }
        }
    }
}

fn group_clusters_in_ranks(vgraph: &mut VirtualGraph) {
    // Compute a single global average order for each cluster across all ranks so clusters never cross over.
    let mut global_cluster_avg: BTreeMap<SubgraphId, f64> = BTreeMap::new();
    let mut global_cluster_cnt: BTreeMap<SubgraphId, usize> = BTreeMap::new();
    for rank_nodes in &vgraph.ranks {
        for (ord, &vn) in rank_nodes.iter().enumerate() {
            if let Some(cid) = vgraph.nodes[vn].cluster {
                *global_cluster_avg.entry(cid).or_insert(0.0) += ord as f64;
                *global_cluster_cnt.entry(cid).or_insert(0) += 1;
            }
        }
    }
    for (&cid, sum) in &mut global_cluster_avg {
        *sum /= global_cluster_cnt[&cid] as f64;
    }

    for r in 0..vgraph.ranks.len() {
        if vgraph.ranks[r].len() < 2 {
            continue;
        }
        let mut cluster_avg: BTreeMap<SubgraphId, f64> = BTreeMap::new();
        let mut cluster_cnt: BTreeMap<SubgraphId, usize> = BTreeMap::new();
        for (ord, &vn) in vgraph.ranks[r].iter().enumerate() {
            if let Some(cid) = vgraph.nodes[vn].cluster {
                *cluster_avg.entry(cid).or_insert(0.0) += ord as f64;
                *cluster_cnt.entry(cid).or_insert(0) += 1;
            }
        }
        for (&cid, sum) in &mut cluster_avg {
            *sum /= cluster_cnt[&cid] as f64;
        }
        vgraph.ranks[r].sort_by(|&a, &b| {
            let ca = vgraph.nodes[a].cluster;
            let cb = vgraph.nodes[b].cluster;
            let pos_a = ca
                .and_then(|c| global_cluster_avg.get(&c).copied())
                .unwrap_or(vgraph.nodes[a].order as f64);
            let pos_b = cb
                .and_then(|c| global_cluster_avg.get(&c).copied())
                .unwrap_or(vgraph.nodes[b].order as f64);
            pos_a
                .partial_cmp(&pos_b)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| ca.cmp(&cb))
                .then_with(|| vgraph.nodes[a].order.cmp(&vgraph.nodes[b].order))
        });
        for (ord, &vn) in vgraph.ranks[r].iter().enumerate() {
            vgraph.nodes[vn].order = ord;
        }
    }
}

fn transpose_all_within_clusters(vgraph: &mut VirtualGraph, rankdir: RankDir) {
    let n_ranks = vgraph.ranks.len();
    for _ in 0..8 {
        let mut changed = false;
        for r in 0..n_ranks {
            let len = vgraph.ranks[r].len();
            if len <= 1 {
                continue;
            }
            for i in 0..(len - 1) {
                let v = vgraph.ranks[r][i];
                let w = vgraph.ranks[r][i + 1];
                if vgraph.nodes[v].cluster != vgraph.nodes[w].cluster
                    || has_flat_constraint(vgraph, v, w, rankdir)
                {
                    continue;
                }
                let c0 = pair_in_cross(vgraph, v, w) + pair_out_cross(vgraph, v, w);
                let c1 = pair_in_cross(vgraph, w, v) + pair_out_cross(vgraph, w, v);
                if c1 < c0 {
                    vgraph.ranks[r].swap(i, i + 1);
                    vgraph.nodes[v].order = i + 1;
                    vgraph.nodes[w].order = i;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
}

/// Counts crossings of in-edges between `v` (left) and `w` (right) (`in_cross` in `mincross.c`).
fn pair_in_cross(vgraph: &VirtualGraph, v: usize, w: usize) -> i64 {
    let mut cross = 0i64;
    for &e2_idx in &vgraph.nodes[w].in_edges {
        let e2 = &vgraph.edges[e2_idx];
        let cnt = i64::from(e2.xpenalty);
        let inv = vgraph.nodes[e2.tail].order as i32;
        for &e1_idx in &vgraph.nodes[v].in_edges {
            let e1 = &vgraph.edges[e1_idx];
            let t = (vgraph.nodes[e1.tail].order as i32) - inv;
            if t > 0 || (t == 0 && e1.tail_port_x > e2.tail_port_x) {
                cross += i64::from(e1.xpenalty) * cnt;
            }
        }
    }
    cross
}

/// Counts crossings of out-edges between `v` (left) and `w` (right) (`out_cross` in `mincross.c`).
fn pair_out_cross(vgraph: &VirtualGraph, v: usize, w: usize) -> i64 {
    let mut cross = 0i64;
    for &e2_idx in &vgraph.nodes[w].out_edges {
        let e2 = &vgraph.edges[e2_idx];
        let cnt = i64::from(e2.xpenalty);
        let inv = vgraph.nodes[e2.head].order as i32;
        for &e1_idx in &vgraph.nodes[v].out_edges {
            let e1 = &vgraph.edges[e1_idx];
            let t = (vgraph.nodes[e1.head].order as i32) - inv;
            if t > 0 || (t == 0 && e1.head_port_x > e2.head_port_x) {
                cross += i64::from(e1.xpenalty) * cnt;
            }
        }
    }
    cross
}

/// Counts total edge crossings across all adjacent rank pairs (`ncross` / `rcross` in `mincross.c`).
fn total_crossings(vgraph: &VirtualGraph) -> i64 {
    let mut total = 0i64;
    for r in 0..vgraph.ranks.len().saturating_sub(1) {
        let next_len = vgraph.ranks[r + 1].len();
        if next_len == 0 {
            continue;
        }
        let mut count = vec![0i64; next_len];
        let mut max_ord = 0usize;
        for &u in &vgraph.ranks[r] {
            if max_ord > 0 {
                for &vei in &vgraph.nodes[u].out_edges {
                    let e = &vgraph.edges[vei];
                    let h_ord = vgraph.nodes[e.head].order;
                    for &c in count.iter().take(max_ord + 1).skip(h_ord + 1) {
                        total += c * i64::from(e.xpenalty);
                    }
                }
            }
            for &vei in &vgraph.nodes[u].out_edges {
                let e = &vgraph.edges[vei];
                let h_ord = vgraph.nodes[e.head].order;
                if h_ord < next_len {
                    max_ord = max_ord.max(h_ord);
                    count[h_ord] += i64::from(e.xpenalty);
                }
            }
        }
    }
    total
}

// ============================================================================
// Stage 4: Coordinate Assignment (`position.c`) & Cluster Bounding Boxes
// ============================================================================

fn parse_graph_size_pt(attrs: &gv_core::AttrMap) -> Option<(f64, f64, bool)> {
    let raw = attrs.get("size")?.trim();
    if raw.is_empty() {
        return None;
    }
    let is_bang = raw.ends_with('!');
    let clean = raw.trim_end_matches('!');
    let mut parts = clean.split(',').map(str::trim);
    let w_in = parts.next()?.parse::<f64>().ok()?;
    let h_in = parts.next()?.parse::<f64>().ok()?;
    if w_in > 0.0 && h_in > 0.0 {
        Some((w_in * POINTS_PER_INCH, h_in * POINTS_PER_INCH, is_bang))
    } else {
        None
    }
}

fn assign_coordinates(
    graph: &mut Graph,
    vgraph: &mut VirtualGraph,
    rankdir: RankDir,
    nodesep_pt: f64,
    ranksep_pt: f64,
    equal_ranksep: bool,
    has_edge_labels: bool,
) {
    let n_ranks = vgraph.ranks.len();
    if n_ranks == 0 {
        return;
    }

    // 1. Compute rank positions (`set_ycoords` in `position.c:707`).
    let mut rank_pht = vec![0.0f64; n_ranks];
    for (r, rank_nodes) in vgraph.ranks.iter().enumerate() {
        let mut max_ht2 = 0.0f64;
        for &vn in rank_nodes {
            max_ht2 = max_ht2.max((vgraph.nodes[vn].ht / 2.0).round());
        }
        rank_pht[r] = max_ht2;
    }

    // Compute `ht1` (bottom extent) and `ht2` (top extent) including cluster borders & labels (`clust_ht` in `position.c:774`).
    let mut rank_ht1 = rank_pht.clone();
    let mut rank_ht2 = rank_pht.clone();
    const CL_OFFSET: f64 = 8.0;

    fn update_cluster_ht(
        graph: &Graph,
        subg_id: SubgraphId,
        n_ranks: usize,
        rank_ht1: &mut [f64],
        rank_ht2: &mut [f64],
    ) -> Option<(usize, usize, f64, f64)> {
        let subg = &graph.subgraphs[subg_id.0 as usize];
        let mut min_r = usize::MAX;
        let mut max_r = 0usize;
        for &nid in &subg.nodes {
            let r = graph.nodes[nid.0 as usize].rank as usize;
            min_r = min_r.min(r);
            max_r = max_r.max(r);
        }
        let cl_offset = if subg.is_cluster {
            parse_cluster_margin_pt(&subg.graph_attrs)
        } else {
            CL_OFFSET
        };
        let mut sub_ht1 = cl_offset;
        let mut sub_ht2 = cl_offset;
        for &cid in &subg.subgraphs {
            if let Some((c_min, c_max, c_ht1, c_ht2)) =
                update_cluster_ht(graph, cid, n_ranks, rank_ht1, rank_ht2)
            {
                if c_min < min_r {
                    min_r = c_min;
                    sub_ht2 = c_ht2 + CL_OFFSET;
                } else if c_min == min_r {
                    sub_ht2 = sub_ht2.max(c_ht2 + CL_OFFSET);
                }
                if c_max > max_r {
                    max_r = c_max;
                    sub_ht1 = c_ht1 + CL_OFFSET;
                } else if c_max == max_r {
                    sub_ht1 = sub_ht1.max(c_ht1 + CL_OFFSET);
                }
            }
        }
        if min_r > max_r || max_r >= n_ranks {
            return None;
        }
        if subg.is_cluster {
            let label_ht = subg
                .graph_attrs
                .get("label")
                .filter(|s| !s.trim().is_empty())
                .map(|lbl| {
                    let font_name = subg
                        .graph_attrs
                        .get("fontname")
                        .map(String::as_str)
                        .unwrap_or(DEFAULT_FONT_NAME);
                    let font_size =
                        parse_attr_f64(&subg.graph_attrs, "fontsize", DEFAULT_FONT_SIZE);
                    measure_multiline_label_with_context(
                        lbl,
                        &LabelSubstContext::default(),
                        font_name,
                        font_size,
                    )
                    .height
                    .floor()
                })
                .unwrap_or(0.0);
            let labelloc = subg
                .graph_attrs
                .get("labelloc")
                .map(|s| s.trim().to_ascii_lowercase());
            if labelloc.as_deref() == Some("b") {
                sub_ht1 += label_ht;
            } else {
                sub_ht2 += label_ht;
            }
            rank_ht1[max_r] = rank_ht1[max_r].max(rank_ht1[max_r] + sub_ht1 - CL_OFFSET);
            rank_ht2[min_r] = rank_ht2[min_r].max(rank_ht2[min_r] + sub_ht2 - CL_OFFSET);
            Some((min_r, max_r, rank_ht1[max_r], rank_ht2[min_r]))
        } else {
            None
        }
    }

    let child_subgs: BTreeSet<SubgraphId> = graph
        .subgraphs
        .iter()
        .flat_map(|s| s.subgraphs.iter().copied())
        .collect();
    let has_clusters = graph.subgraphs.iter().any(|s| s.is_cluster);
    for subg in &graph.subgraphs {
        if !child_subgs.contains(&subg.id) {
            update_cluster_ht(graph, subg.id, n_ranks, &mut rank_ht1, &mut rank_ht2);
        }
    }

    // Compute rank coordinates (`set_ycoords` in `position.c:707`):
    // `d0 = rank[r+1].pht2 + rank[r].pht1 + ranksep`, `d1 = rank[r+1].ht2 + rank[r].ht1 + CL_OFFSET`
    let mut rank_pos = vec![0.0f64; n_ranks];
    let mut max_delta = ranksep_pt;
    for r in 0..(n_ranks - 1) {
        let d0 = rank_pht[r] + rank_pht[r + 1] + ranksep_pt;
        let d1 = if has_clusters {
            rank_ht1[r] + rank_ht2[r + 1] + CL_OFFSET
        } else {
            0.0
        };
        let delta = d0.max(d1);
        max_delta = max_delta.max(delta);
        rank_pos[r + 1] = rank_pos[r] + delta;
    }
    if equal_ranksep && n_ranks > 1 {
        for (r, pos) in rank_pos.iter_mut().enumerate() {
            *pos = (r as f64) * max_delta;
        }
    }

    // In `TB` (default) and `LR`, rank 0 has the largest internal Y coordinate (`UR.y`) and `maxrank` has `0`,
    // and in `LR`, `ccwrotatepf(90)` maps `final_x = UR.y - y_internal`, which increases from `0` at rank 0
    // to `UR.y` at `maxrank`!
    let max_rank_coord = rank_pos.last().copied().unwrap_or(0.0);
    if matches!(rankdir, RankDir::Tb | RankDir::Rl) {
        for pos in &mut rank_pos {
            *pos = max_rank_coord - *pos;
        }
    }
    for (r, rank_nodes) in vgraph.ranks.iter().enumerate() {
        for &vn in rank_nodes {
            vgraph.nodes[vn].rank_coord = rank_pos[r];
        }
    }

    // Check `ratio="compress"` and `size="w,h"` (`compress_graph` in `position.c:488`).
    let ratio_attr = graph
        .graph_attrs
        .get("ratio")
        .map(|s| s.trim().to_ascii_lowercase());
    let size_info = parse_graph_size_pt(&graph.graph_attrs);
    let compress_limit_pt = if ratio_attr.as_deref() == Some("compress")
        && let Some((sx, sy, _)) = size_info
    {
        Some(if rankdir.is_flipped() { sy } else { sx })
    } else {
        None
    };

    // 2. Compute within-rank coordinates (`set_xcoords` / auxiliary Network Simplex + `LR_balance`).
    solve_within_rank_coordinates(
        graph,
        vgraph,
        nodesep_pt,
        has_edge_labels,
        compress_limit_pt,
    );

    // 3. Map `(rank_coord, order_coord)` to 2D `(x, y)` according to `rankdir`:
    //    - `TB` / `BT`: `x = order_coord`, `y = rank_coord`
    //    - `LR` / `RL`: `x = rank_coord`, `y = order_coord`
    for vn in &vgraph.nodes {
        if let Some(real_id) = vn.real_node {
            let pt = if rankdir.is_flipped() {
                Point::new(vn.rank_coord, vn.order_coord)
            } else {
                Point::new(vn.order_coord, vn.rank_coord)
            };
            graph.nodes[real_id.0 as usize].pos = pt;
        }
    }

    // 4. Compute cluster bounding boxes (`subgraph.bb`) bottom-up.
    compute_cluster_bounding_boxes(graph);
}

/// Solves within-rank coordinates using the auxiliary graph Network Simplex formulation from
/// `position.c` (`make_LR_constraints` + `make_edge_pairs` + `rank2`) followed by fractional
/// median-attraction refinement passes.
fn solve_within_rank_coordinates(
    graph: &Graph,
    vgraph: &mut VirtualGraph,
    nodesep_pt: f64,
    has_edge_labels: bool,
    compress_limit_pt: Option<f64>,
) {
    let n_vnodes = vgraph.nodes.len();
    if n_vnodes == 0 {
        return;
    }

    // Build auxiliary Network Simplex graph where node "rank" = integer X coordinate in points.
    // Nodes `0..n_vnodes` are the virtual/real nodes; nodes `n_vnodes..` are slack/cluster border nodes.
    let mut aux_edges = Vec::new();
    let mut total_aux_nodes = n_vnodes;

    // Left-to-right minimum separation constraints within each rank (`make_LR_constraints` in `position.c:231`).
    for (r, rank_nodes) in vgraph.ranks.iter().enumerate() {
        let base_sep = if has_edge_labels && (r % 2 == 1) {
            5.0
        } else {
            nodesep_pt
        };
        for w in rank_nodes.windows(2) {
            let u = w[0];
            let v = w[1];
            let sep = (vgraph.nodes[u].rw + vgraph.nodes[v].lw + base_sep).round() as i32;
            aux_edges.push(NsEdge {
                tail: u,
                head: v,
                minlen: sep.max(1),
                weight: 0,
                cutvalue: 0,
                tree_index: None,
            });
        }
    }

    // Cluster bounding-box constraints (`pos_clusters`: `contain_clustnodes`, `separate_subclust`, `keepout_othernodes` in `position.c:360..490`).
    let mut clust_ln_rn: BTreeMap<SubgraphId, (usize, usize, usize, usize)> = BTreeMap::new();
    for subg in &graph.subgraphs {
        if !subg.is_cluster {
            continue;
        }
        let mut c_vnodes = Vec::new();
        let mut min_r = usize::MAX;
        let mut max_r = 0usize;
        for (vi, vn) in vgraph.nodes.iter().enumerate() {
            if vn.cluster == Some(subg.id) {
                c_vnodes.push(vi);
                let r = vn.rank as usize;
                min_r = min_r.min(r);
                max_r = max_r.max(r);
            }
        }
        if c_vnodes.is_empty() || min_r > max_r {
            continue;
        }
        let ln = total_aux_nodes;
        let rn = total_aux_nodes + 1;
        total_aux_nodes += 2;
        clust_ln_rn.insert(subg.id, (ln, rn, min_r, max_r));

        let margin = parse_cluster_margin_pt(&subg.graph_attrs).round() as i32;

        // `contain_clustnodes`: cluster compaction edge `ln -> rn` with `minlen = 1, weight = 128`.
        aux_edges.push(NsEdge {
            tail: ln,
            head: rn,
            minlen: 1,
            weight: 128,
            cutvalue: 0,
            tree_index: None,
        });

        for &vi in &c_vnodes {
            let lw = vgraph.nodes[vi].lw.round() as i32;
            let rw = vgraph.nodes[vi].rw.round() as i32;
            aux_edges.push(NsEdge {
                tail: ln,
                head: vi,
                minlen: (margin + lw).max(1),
                weight: 0,
                cutvalue: 0,
                tree_index: None,
            });
            aux_edges.push(NsEdge {
                tail: vi,
                head: rn,
                minlen: (margin + rw).max(1),
                weight: 0,
                cutvalue: 0,
                tree_index: None,
            });
        }

        // `keepout_othernodes`: keep nodes outside `subg` on ranks `min_r..=max_r` outside `[ln, rn]`.
        for r in min_r..=max_r.min(vgraph.ranks.len().saturating_sub(1)) {
            let r_nodes = &vgraph.ranks[r];
            let first_pos = r_nodes
                .iter()
                .position(|&v| vgraph.nodes[v].cluster == Some(subg.id));
            let last_pos = r_nodes
                .iter()
                .rposition(|&v| vgraph.nodes[v].cluster == Some(subg.id));
            if let (Some(fp), Some(lp)) = (first_pos, last_pos) {
                if fp > 0 {
                    let u_left = r_nodes[fp - 1];
                    let rw = vgraph.nodes[u_left].rw.round() as i32;
                    aux_edges.push(NsEdge {
                        tail: u_left,
                        head: ln,
                        minlen: (margin + rw).max(1),
                        weight: 0,
                        cutvalue: 0,
                        tree_index: None,
                    });
                }
                if lp + 1 < r_nodes.len() {
                    let u_right = r_nodes[lp + 1];
                    let lw = vgraph.nodes[u_right].lw.round() as i32;
                    aux_edges.push(NsEdge {
                        tail: rn,
                        head: u_right,
                        minlen: (margin + lw).max(1),
                        weight: 0,
                        cutvalue: 0,
                        tree_index: None,
                    });
                }
            }
        }
    }

    // `separate_subclust`: separate clusters that share at least one rank.
    let clust_keys: Vec<SubgraphId> = clust_ln_rn.keys().copied().collect();
    for i in 0..clust_keys.len() {
        for j in (i + 1)..clust_keys.len() {
            let c1 = clust_keys[i];
            let c2 = clust_keys[j];
            let (ln1, rn1, min1, max1) = clust_ln_rn[&c1];
            let (ln2, rn2, min2, max2) = clust_ln_rn[&c2];
            let ov_min = min1.max(min2);
            let ov_max = max1.min(max2);
            if ov_min > ov_max {
                continue;
            }
            for r in ov_min..=ov_max.min(vgraph.ranks.len().saturating_sub(1)) {
                let p1 = vgraph.ranks[r]
                    .iter()
                    .position(|&v| vgraph.nodes[v].cluster == Some(c1));
                let p2 = vgraph.ranks[r]
                    .iter()
                    .position(|&v| vgraph.nodes[v].cluster == Some(c2));
                if let (Some(ord1), Some(ord2)) = (p1, p2) {
                    if ord1 < ord2 {
                        aux_edges.push(NsEdge {
                            tail: rn1,
                            head: ln2,
                            minlen: CLUSTER_MARGIN_PT as i32,
                            weight: 0,
                            cutvalue: 0,
                            tree_index: None,
                        });
                    } else {
                        aux_edges.push(NsEdge {
                            tail: rn2,
                            head: ln1,
                            minlen: CLUSTER_MARGIN_PT as i32,
                            weight: 0,
                            cutvalue: 0,
                            tree_index: None,
                        });
                    }
                    break;
                }
            }
        }
    }

    // Virtual slack node pairs for inter-rank edges (`make_edge_pairs` in `position.c`):
    // For each edge `e = (tail, head)` with weight `w`, create slack node `sn` and edges
    // `sn -> tail` (weight `w`, minlen `m0 + 1`) and `sn -> head` (weight `w`, minlen `m1 + 1`).
    for e in &vgraph.edges {
        let sn = total_aux_nodes;
        total_aux_nodes += 1;
        let port_diff = e.head_port_x - e.tail_port_x;
        let (m0, m1) = if port_diff > 0 {
            (port_diff, 0)
        } else {
            (0, -port_diff)
        };
        aux_edges.push(NsEdge {
            tail: sn,
            head: e.tail,
            minlen: m0 + 1,
            weight: e.weight,
            cutvalue: 0,
            tree_index: None,
        });
        aux_edges.push(NsEdge {
            tail: sn,
            head: e.head,
            minlen: m1 + 1,
            weight: e.weight,
            cutvalue: 0,
            tree_index: None,
        });
    }

    // Only connect separate weakly connected components of the auxiliary graph (`connectGraph` in `position.c:143`).
    {
        let mut comp_uf = UnionFind::new(total_aux_nodes);
        for e in &aux_edges {
            comp_uf.union(e.tail, e.head);
        }
        for r in 0..vgraph.ranks.len().saturating_sub(1) {
            if let (Some(&u), Some(&v)) = (vgraph.ranks[r].first(), vgraph.ranks[r + 1].first())
                && comp_uf.find(u) != comp_uf.find(v)
            {
                comp_uf.union(u, v);
                let sn = total_aux_nodes;
                total_aux_nodes += 1;
                aux_edges.push(NsEdge {
                    tail: sn,
                    head: u,
                    minlen: 0,
                    weight: 0,
                    cutvalue: 0,
                    tree_index: None,
                });
                aux_edges.push(NsEdge {
                    tail: sn,
                    head: v,
                    minlen: 0,
                    weight: 0,
                    cutvalue: 0,
                    tree_index: None,
                });
            }
        }
    }

    // Optional `compress_graph` constraints (`position.c:488`) when `ratio="compress"` and `size` is set.
    if let Some(limit_pt) = compress_limit_pt {
        let ln = total_aux_nodes;
        let rn = total_aux_nodes + 1;
        total_aux_nodes += 2;
        for rank_nodes in &vgraph.ranks {
            if let (Some(&first), Some(&last)) = (rank_nodes.first(), rank_nodes.last()) {
                aux_edges.push(NsEdge {
                    tail: ln,
                    head: first,
                    minlen: (vgraph.nodes[first].lw + 8.0).round() as i32,
                    weight: 0,
                    cutvalue: 0,
                    tree_index: None,
                });
                aux_edges.push(NsEdge {
                    tail: last,
                    head: rn,
                    minlen: (vgraph.nodes[last].rw + 8.0).round() as i32,
                    weight: 0,
                    cutvalue: 0,
                    tree_index: None,
                });
            }
        }
        aux_edges.push(NsEdge {
            tail: ln,
            head: rn,
            minlen: limit_pt.round().max(1.0) as i32,
            weight: 1000,
            cutvalue: 0,
            tree_index: None,
        });
    }

    let ns_coords = network_simplex_solve(total_aux_nodes, &mut aux_edges, 2);
    for (idx, vn) in vgraph.nodes.iter_mut().enumerate() {
        vn.order_coord = f64::from(ns_coords[idx]);
    }
}

#[allow(dead_code)]
fn refine_rank_positions(vgraph: &mut VirtualGraph, r: usize, use_in_edges: bool, nodesep_pt: f64) {
    let rank_nodes = vgraph.ranks[r].clone();
    if rank_nodes.is_empty() {
        return;
    }

    let mut target_x: Vec<f64> = rank_nodes
        .iter()
        .map(|&u| vgraph.nodes[u].order_coord)
        .collect();

    for (idx, &u) in rank_nodes.iter().enumerate() {
        let mut weighted_positions: Vec<(f64, i32)> = Vec::new();
        if use_in_edges {
            for &vei in &vgraph.nodes[u].in_edges {
                let e = &vgraph.edges[vei];
                let pos =
                    vgraph.nodes[e.tail].order_coord + f64::from(e.tail_port_x - e.head_port_x);
                weighted_positions.push((pos, e.weight.max(1)));
            }
        } else {
            for &vei in &vgraph.nodes[u].out_edges {
                let e = &vgraph.edges[vei];
                let pos =
                    vgraph.nodes[e.head].order_coord + f64::from(e.head_port_x - e.tail_port_x);
                weighted_positions.push((pos, e.weight.max(1)));
            }
        }
        if !weighted_positions.is_empty() {
            let total_w: f64 = weighted_positions.iter().map(|&(_, w)| f64::from(w)).sum();
            let sum_wx: f64 = weighted_positions
                .iter()
                .map(|&(x, w)| x * f64::from(w))
                .sum();
            target_x[idx] = sum_wx / total_w;
        }
    }

    // Enforce left-to-right separation constraints while staying as close as possible to `target_x`.
    let k = rank_nodes.len();
    let mut seps = Vec::with_capacity(k.saturating_sub(1));
    for i in 0..k.saturating_sub(1) {
        let u = rank_nodes[i];
        let v = rank_nodes[i + 1];
        let cluster_extra = if vgraph.nodes[u].cluster != vgraph.nodes[v].cluster {
            2.0 * CLUSTER_MARGIN_PT
        } else {
            0.0
        };
        seps.push(vgraph.nodes[u].rw + vgraph.nodes[v].lw + nodesep_pt + cluster_extra);
    }

    // Forward pass + backward pass + average to keep symmetric layout centered around targets.
    let mut fwd = target_x.clone();
    for i in 1..k {
        if fwd[i] < fwd[i - 1] + seps[i - 1] {
            fwd[i] = fwd[i - 1] + seps[i - 1];
        }
    }
    let mut bwd = target_x;
    for i in (0..k.saturating_sub(1)).rev() {
        if bwd[i] > bwd[i + 1] - seps[i] {
            bwd[i] = bwd[i + 1] - seps[i];
        }
    }
    for (i, &u) in rank_nodes.iter().enumerate() {
        vgraph.nodes[u].order_coord = 0.5 * (fwd[i] + bwd[i]);
    }
}

/// Computes cluster bounding boxes (`subgraph.bb`) recursively with cluster padding (`8.0` pt) and label height.
pub(crate) fn compute_cluster_bounding_boxes(graph: &mut Graph) {
    let n_subg = graph.subgraphs.len();
    if n_subg == 0 {
        return;
    }

    fn compute_subg_bb(
        graph: &mut Graph,
        subg_id: SubgraphId,
        visited: &mut [bool],
    ) -> BoundingBox {
        let idx = subg_id.0 as usize;
        if visited[idx] {
            return graph.subgraphs[idx].bb;
        }
        visited[idx] = true;

        let child_ids = graph.subgraphs[idx].subgraphs.clone();
        for &cid in &child_ids {
            compute_subg_bb(graph, cid, visited);
        }

        let subg = &graph.subgraphs[idx];
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;

        for &nid in &subg.nodes {
            let n = &graph.nodes[nid.0 as usize];
            let hw = (n.width * POINTS_PER_INCH) / 2.0;
            let hh = (n.height * POINTS_PER_INCH) / 2.0;
            min_x = min_x.min(n.pos.x - hw);
            max_x = max_x.max(n.pos.x + hw);
            min_y = min_y.min(n.pos.y - hh);
            max_y = max_y.max(n.pos.y + hh);
        }

        for &cid in &child_ids {
            let cbb = graph.subgraphs[cid.0 as usize].bb;
            if cbb.width() > 0.0 || cbb.height() > 0.0 {
                min_x = min_x.min(cbb.ll.x);
                min_y = min_y.min(cbb.ll.y);
                max_x = max_x.max(cbb.ur.x);
                max_y = max_y.max(cbb.ur.y);
            }
        }

        if !min_x.is_finite() {
            let bb = BoundingBox::new(0.0, 0.0, 0.0, 0.0);
            graph.subgraphs[idx].bb = bb;
            return bb;
        }

        if graph.subgraphs[idx].is_cluster {
            let margin = parse_cluster_margin_pt(&graph.subgraphs[idx].graph_attrs);
            min_x -= margin;
            max_x += margin;
            min_y -= margin;
            max_y += margin;

            if let Some(lbl) = graph.subgraphs[idx]
                .graph_attrs
                .get("label")
                .filter(|s| !s.trim().is_empty())
            {
                let font_name = graph.subgraphs[idx]
                    .graph_attrs
                    .get("fontname")
                    .map(String::as_str)
                    .unwrap_or(DEFAULT_FONT_NAME);
                let font_size = parse_attr_f64(
                    &graph.subgraphs[idx].graph_attrs,
                    "fontsize",
                    DEFAULT_FONT_SIZE,
                );
                let layout = measure_multiline_label_with_context(
                    lbl,
                    &LabelSubstContext::default(),
                    font_name,
                    font_size,
                );
                let label_h = layout.height + 4.0;
                let labelloc = graph.subgraphs[idx]
                    .graph_attrs
                    .get("labelloc")
                    .map(|s| s.trim().to_ascii_lowercase());
                if labelloc.as_deref() == Some("b") {
                    min_y -= label_h;
                } else {
                    max_y += label_h;
                }
                let cur_w = max_x - min_x;
                let req_w = layout.width + 2.0 * margin;
                if req_w > cur_w {
                    let expand = (req_w - cur_w) / 2.0;
                    min_x -= expand;
                    max_x += expand;
                }
            }
        }

        let bb = BoundingBox::new(min_x, min_y, max_x, max_y);
        graph.subgraphs[idx].bb = bb;
        bb
    }

    let mut visited = vec![false; n_subg];
    for i in 0..n_subg {
        if !visited[i] {
            compute_subg_bb(graph, SubgraphId(i as u32), &mut visited);
        }
    }
}

// ============================================================================
// Stage 5: Edge Spline Routing (`dotsplines.c`) & Coordinate Normalization
// ============================================================================

fn route_all_dot_splines(graph: &mut Graph, vgraph: &VirtualGraph, rankdir: RankDir) {
    let rankdir_str = rankdir.as_str();

    // Collect bounding boxes of all real nodes for obstacle avoidance.
    let node_bboxes: Vec<BoundingBox> = graph.nodes.iter().map(node_bounding_box).collect();

    // Group parallel edges between the same unordered node pair so multi-edges receive clean lateral offsets.
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
        let first_edge = &graph.edges[indices[0]];
        if first_edge.tail == first_edge.head {
            for (k, &ei) in indices.iter().enumerate() {
                self_loop_index[ei] = k;
            }
        } else {
            let count = indices.len();
            let step = 14.0;
            for (k, &ei) in indices.iter().enumerate() {
                let centered = (k as f64) - ((count - 1) as f64) / 2.0;
                multi_offset[ei] = centered * step;
            }
        }
    }

    for ei in 0..graph.edges.len() {
        let tail_id = graph.edges[ei].tail;
        let head_id = graph.edges[ei].head;
        let tail_node = graph.nodes[tail_id.0 as usize].clone();
        let head_node = graph.nodes[head_id.0 as usize].clone();

        // Collect obstacle boxes excluding the edge's own endpoints.
        let mut obstacles = Vec::new();
        for (ni, bb) in node_bboxes.iter().enumerate() {
            if ni != tail_id.0 as usize && ni != head_id.0 as usize {
                obstacles.push(*bb);
            }
        }

        let (mut spline, mut label_pos) = route_edge_spline(
            &tail_node,
            &head_node,
            &graph.edges[ei],
            &obstacles,
            rankdir_str,
            graph.kind == gv_core::GraphKind::Directed,
        );

        // If this edge is a multi-rank edge with virtual nodes AND `route_edge_spline` produced a single
        // segment that doesn't already bend around virtual waypoints, incorporate virtual node waypoints
        // or multi-edge lateral offset cleanly.
        let offset = multi_offset[ei];
        if tail_id == head_id && self_loop_index[ei] > 0 {
            let extra = (self_loop_index[ei] as f64) * 14.0;
            for seg in &mut spline.segments {
                let n_pts = seg.points.len();
                for (pi, pt) in seg.points.iter_mut().enumerate() {
                    if pi > 0 && pi + 1 < n_pts {
                        pt.x += extra;
                    }
                }
            }
            if let Some(lp) = &mut label_pos {
                lp.x += extra;
            }
        } else if offset.abs() > 1e-6 {
            apply_lateral_offset_to_spline(
                &mut spline,
                &mut label_pos,
                tail_node.pos,
                head_node.pos,
                offset,
            );
        } else if let Some(chain) = vgraph.edge_chains.get(&graph.edges[ei].id)
            && chain.len() > 2
            && spline.segments.len() == 1
            && spline.segments[0].points.len() == 4
        {
            // Check if intermediate virtual nodes deviate laterally from the straight line between tail and head.
            let p_start = spline.segments[0].points[0];
            let p_end = spline.segments[0].points[3];
            let mut waypoints = Vec::with_capacity(chain.len());
            waypoints.push(p_start);
            let mut max_dev = 0.0f64;
            for &vn_idx in &chain[1..chain.len() - 1] {
                let vn = &vgraph.nodes[vn_idx];
                let wp = if rankdir.is_flipped() {
                    Point::new(vn.rank_coord, vn.order_coord)
                } else {
                    Point::new(vn.order_coord, vn.rank_coord)
                };
                max_dev = max_dev.max(point_to_line_distance(wp, p_start, p_end));
                waypoints.push(wp);
            }
            waypoints.push(p_end);

            if max_dev > 4.0 {
                spline = catmull_rom_to_bezier_spline(
                    &waypoints,
                    spline.segments[0].start_arrow,
                    spline.segments[0].end_arrow,
                );
            }
            if label_pos.is_some() {
                let mid_vn = &vgraph.nodes[chain[chain.len() / 2]];
                let label_offset = (mid_vn.rw - mid_vn.lw) / 2.0;
                label_pos = Some(if rankdir.is_flipped() {
                    Point::new(mid_vn.rank_coord, mid_vn.order_coord + label_offset)
                } else {
                    Point::new(mid_vn.order_coord + label_offset, mid_vn.rank_coord)
                });
            }
        }

        graph.edges[ei].spline = spline;
        graph.edges[ei].label_pos = label_pos;
    }
}

fn apply_lateral_offset_to_spline(
    spline: &mut Spline,
    label_pos: &mut Option<Point>,
    from: Point,
    to: Point,
    offset: f64,
) {
    let dir = Point::new(to.x - from.x, to.y - from.y).normalized_or_zero();
    let perp = if dir.x.abs() < 1e-9 && dir.y.abs() < 1e-9 {
        Point::new(1.0, 0.0)
    } else {
        Point::new(-dir.y, dir.x)
    };
    for seg in &mut spline.segments {
        let len = seg.points.len();
        for (i, pt) in seg.points.iter_mut().enumerate() {
            if i > 0 && i + 1 < len {
                pt.x += perp.x * offset;
                pt.y += perp.y * offset;
            }
        }
    }
    if let Some(lp) = label_pos {
        lp.x += perp.x * offset;
        lp.y += perp.y * offset;
    }
}

fn point_to_line_distance(p: Point, a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = dx.hypot(dy);
    if len <= 1e-9 {
        return p.dist(a);
    }
    ((dy * p.x - dx * p.y + b.x * a.y - b.y * a.x).abs()) / len
}

fn catmull_rom_to_bezier_spline(
    waypoints: &[Point],
    start_arrow: Option<Point>,
    end_arrow: Option<Point>,
) -> Spline {
    let n = waypoints.len();
    if n < 2 {
        return Spline::default();
    }
    let mut points = Vec::with_capacity(3 * (n - 1) + 1);
    points.push(waypoints[0]);

    for i in 0..(n - 1) {
        let p0 = if i == 0 {
            waypoints[0]
        } else {
            waypoints[i - 1]
        };
        let p1 = waypoints[i];
        let p2 = waypoints[i + 1];
        let p3 = if i + 2 < n {
            waypoints[i + 2]
        } else {
            waypoints[n - 1]
        };

        let c1 = Point::new(p1.x + (p2.x - p0.x) / 6.0, p1.y + (p2.y - p0.y) / 6.0);
        let c2 = Point::new(p2.x - (p3.x - p1.x) / 6.0, p2.y - (p3.y - p1.y) / 6.0);
        points.push(c1);
        points.push(c2);
        points.push(p2);
    }

    Spline {
        segments: vec![BezierSegment {
            points,
            start_arrow,
            end_arrow,
        }],
    }
}

/// Computes the axis-aligned bounding box of a node in points (`72.0` pt/inch).
pub(crate) fn node_bounding_box(node: &gv_core::NodeData) -> BoundingBox {
    let hw = (node.width * POINTS_PER_INCH) / 2.0;
    let hh = (node.height * POINTS_PER_INCH) / 2.0;
    BoundingBox::new(
        node.pos.x - hw,
        node.pos.y - hh,
        node.pos.x + hw,
        node.pos.y + hh,
    )
}

/// Shifts all node positions, cluster bounding boxes, edge spline control points, and edge labels
/// so that `graph.bb.ll == (0.0, 0.0)` and `graph.bb.ur == (width, height)` (`gv_postprocess` in C Graphviz).
pub(crate) fn normalize_graph_coordinates(graph: &mut Graph) {
    if graph.nodes.is_empty() {
        graph.bb = BoundingBox::new(0.0, 0.0, 0.0, 0.0);
        return;
    }

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for node in &graph.nodes {
        let bb = node_bounding_box(node);
        min_x = min_x.min(bb.ll.x);
        min_y = min_y.min(bb.ll.y);
        max_x = max_x.max(bb.ur.x);
        max_y = max_y.max(bb.ur.y);
    }

    for subg in &graph.subgraphs {
        if subg.is_cluster && (subg.bb.width() > 0.0 || subg.bb.height() > 0.0) {
            min_x = min_x.min(subg.bb.ll.x - CLUSTER_MARGIN_PT);
            min_y = min_y.min(subg.bb.ll.y - CLUSTER_MARGIN_PT);
            max_x = max_x.max(subg.bb.ur.x + CLUSTER_MARGIN_PT);
            max_y = max_y.max(subg.bb.ur.y + CLUSTER_MARGIN_PT);
        }
    }

    for edge in &graph.edges {
        for seg in &edge.spline.segments {
            for &pt in &seg.points {
                min_x = min_x.min(pt.x);
                min_y = min_y.min(pt.y);
                max_x = max_x.max(pt.x);
                max_y = max_y.max(pt.y);
            }
            if let Some(sa) = seg.start_arrow {
                min_x = min_x.min(sa.x);
                min_y = min_y.min(sa.y);
                max_x = max_x.max(sa.x);
                max_y = max_y.max(sa.y);
            }
            if let Some(ea) = seg.end_arrow {
                min_x = min_x.min(ea.x);
                min_y = min_y.min(ea.y);
                max_x = max_x.max(ea.x);
                max_y = max_y.max(ea.y);
            }
        }
        if let Some(lp) = edge.label_pos {
            min_x = min_x.min(lp.x - 8.0);
            min_y = min_y.min(lp.y - 6.0);
            max_x = max_x.max(lp.x + 8.0);
            max_y = max_y.max(lp.y + 6.0);
        }
    }

    // Account for root graph `margin` and label if present (`pad` is applied by the renderer in `gvrender_core_svg.c`).
    let margin_pt = graph
        .graph_attrs
        .get("margin")
        .and_then(|s| s.split(',').next())
        .and_then(gv_geom::shapes::parse_f64_strip_units)
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| v * POINTS_PER_INCH)
        .unwrap_or(0.0);

    let border = margin_pt;
    min_x -= border;
    min_y -= border;
    max_x += border;
    max_y += border;

    if let Some(lbl) = graph
        .graph_attrs
        .get("label")
        .filter(|s| !s.trim().is_empty())
    {
        let font_name = graph
            .graph_attrs
            .get("fontname")
            .map(String::as_str)
            .unwrap_or(DEFAULT_FONT_NAME);
        let font_size = parse_attr_f64(&graph.graph_attrs, "fontsize", DEFAULT_FONT_SIZE);
        let layout = measure_multiline_label_with_context(
            lbl,
            &LabelSubstContext::default(),
            font_name,
            font_size,
        );
        let label_h = layout.height + 6.0;
        let labelloc = graph
            .graph_attrs
            .get("labelloc")
            .map(|s| s.trim().to_ascii_lowercase());
        if labelloc.as_deref() == Some("t") {
            max_y += label_h;
        } else {
            min_y -= label_h;
        }
        let cur_w = max_x - min_x;
        if layout.width + 2.0 * border > cur_w {
            let extra = (layout.width + 2.0 * border - cur_w) / 2.0;
            min_x -= extra;
            max_x += extra;
        }
    }

    let dx = -min_x;
    let dy = -min_y;

    for node in &mut graph.nodes {
        node.pos.x += dx;
        node.pos.y += dy;
    }

    for subg in &mut graph.subgraphs {
        if subg.bb.width() > 0.0 || subg.bb.height() > 0.0 {
            subg.bb.ll.x += dx;
            subg.bb.ll.y += dy;
            subg.bb.ur.x += dx;
            subg.bb.ur.y += dy;
        }
    }

    for edge in &mut graph.edges {
        for seg in &mut edge.spline.segments {
            for pt in &mut seg.points {
                pt.x += dx;
                pt.y += dy;
            }
            if let Some(sa) = &mut seg.start_arrow {
                sa.x += dx;
                sa.y += dy;
            }
            if let Some(ea) = &mut seg.end_arrow {
                ea.x += dx;
                ea.y += dy;
            }
        }
        if let Some(lp) = &mut edge.label_pos {
            lp.x += dx;
            lp.y += dy;
        }
    }

    graph.bb = BoundingBox::new(0.0, 0.0, (max_x - min_x).max(0.0), (max_y - min_y).max(0.0));
}

pub(crate) fn parse_attr_f64(attrs: &AttrMap, key: &str, default: f64) -> f64 {
    attrs
        .get(key)
        .and_then(|s| s.split([',', ' ']).next())
        .and_then(gv_geom::shapes::parse_f64_strip_units)
        .filter(|v| v.is_finite())
        .unwrap_or(default)
}

/// Parses cluster `margin` in points (`late_int(g, G_margin, CL_OFFSET, 0)` in `position.c:412`).
pub(crate) fn parse_cluster_margin_pt(attrs: &AttrMap) -> f64 {
    let Some(raw) = attrs.get("margin") else {
        return CLUSTER_MARGIN_PT;
    };
    let Some(first) = raw.split([',', ' ']).find(|s| !s.is_empty()) else {
        return CLUSTER_MARGIN_PT;
    };
    let Some(val) = gv_geom::shapes::parse_f64_strip_units(first) else {
        return CLUSTER_MARGIN_PT;
    };
    if val <= 0.0 {
        0.0
    } else if val < 1.0 && first.contains('.') {
        val * POINTS_PER_INCH
    } else {
        val
    }
}
