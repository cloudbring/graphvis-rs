//! Port of Graphviz `lib/fdpgen` (the `fdp` layout engine).
//!
//! Module ↔ C file map:
//! - [`layout`] — `layout.c` (`fdp_layout`, derived graphs, ports, `finalCC`, `setBB`)
//! - [`tlayout`] — `tlayout.c` (spring embedder with ports and the grid)
//! - [`grid`] — `grid.c`
//! - [`xlayout`] — `xlayout.c` (overlap removal by expansion)
//! - [`comp`] — `comp.c` (`findCComp`)
//! - [`fdpinit`] — `fdpinit.c` (node/edge initialisation, `initialPositions`)
//! - [`clusteredges`] — `clusteredges.c` (`compoundEdges`)
//!
//! The cluster hierarchy built by `mkClusters` is shared with `osage` and
//! `patchwork`, which carry copies of the same C function.

// Faithful ports of numeric C loops index several arrays in lockstep.
#![allow(
    clippy::needless_range_loop,
    clippy::too_many_arguments,
    // `!(dist > 0.0)` also catches NaN, as in C.
    clippy::neg_cmp_op_on_partial_ord
)]

pub mod clusteredges;
pub mod comp;
pub mod fdpinit;
pub mod grid;
pub mod layout;
pub mod tlayout;
pub mod xlayout;

use crate::common::utils::mapbool;
use graphviz_pure_core::{Graph, SubgraphData};

/// Whether `GVDBG` (1: derived positions), `GVDBG2` (2: forces) or `GVDBG3`
/// (3: derived node addresses) `GVDBG4` (4: cluster ports) `GVDBG5` (5: spring
/// embedder iterations) or `GVDBG6` (6: spring forces) tracing is on; traces are line-compatible
/// with an instrumented C build.
pub(crate) fn trace(level: u8) -> bool {
    static ON: std::sync::OnceLock<[bool; 6]> = std::sync::OnceLock::new();
    let on = ON.get_or_init(|| {
        ["GVDBG", "GVDBG2", "GVDBG3", "GVDBG4", "GVDBG5", "GVDBG6"]
            .map(|k| std::env::var_os(k).is_some())
    });
    on[usize::from(level.clamp(1, 6)) - 1]
}

/// `is_a_cluster(g)` for a subgraph.
pub fn is_a_cluster(sg: &SubgraphData) -> bool {
    graphviz_pure_core::has_cluster_prefix(&sg.name)
        || mapbool(sg.graph_attrs.get("cluster").map(String::as_str))
}

/// The cluster hierarchy recorded by `mkClusters` (`GD_clust`, `GD_n_cluster`,
/// `LEVEL`, `GPARENT`). Clusters are numbered in discovery order.
#[derive(Debug, Clone, Default)]
pub struct ClusterTree {
    /// Index into `Graph::subgraphs` of each cluster.
    pub subgraph: Vec<usize>,
    /// Immediate child clusters of each cluster.
    pub children: Vec<Vec<usize>>,
    /// Smallest containing cluster (`None` for the root).
    pub parent: Vec<Option<usize>>,
    /// Depth: top-level clusters have level 1 (the root has level 0).
    pub level: Vec<i32>,
    /// Immediate child clusters of the root.
    pub root: Vec<usize>,
}

impl ClusterTree {
    pub fn len(&self) -> usize {
        self.subgraph.len()
    }

    pub fn is_empty(&self) -> bool {
        self.subgraph.is_empty()
    }

    /// Child clusters of `c` (`None` = root).
    pub fn kids(&self, c: Option<usize>) -> &[usize] {
        match c {
            None => &self.root,
            Some(c) => &self.children[c],
        }
    }

    /// `LEVEL(g)`.
    pub fn level_of(&self, c: Option<usize>) -> i32 {
        c.map_or(0, |c| self.level[c])
    }
}

/// `mkClusters(g, NULL, g)`: attach the immediate child clusters of every
/// cluster, looking through non-cluster subgraphs.
pub fn mk_clusters(graph: &Graph) -> ClusterTree {
    fn scan(
        graph: &Graph,
        subs: &[usize],
        parent: Option<usize>,
        out: &mut Vec<usize>,
        t: &mut ClusterTree,
    ) {
        for &si in subs {
            let sg = &graph.subgraphs[si];
            if is_a_cluster(sg) {
                let c = t.subgraph.len();
                t.subgraph.push(si);
                t.children.push(Vec::new());
                t.parent.push(parent);
                t.level.push(t.level_of(parent) + 1);
                out.push(c);
                let kids: Vec<usize> = sg.subgraphs.iter().map(|s| s.0 as usize).collect();
                let mut list = Vec::new();
                scan(graph, &kids, Some(c), &mut list, t);
                t.children[c] = list;
            } else {
                let kids: Vec<usize> = sg.subgraphs.iter().map(|s| s.0 as usize).collect();
                scan(graph, &kids, parent, out, t);
            }
        }
    }
    let mut t = ClusterTree::default();
    let top = crate::neatogen::neatoinit::top_level_subgraphs(graph);
    let mut root = Vec::new();
    scan(graph, &top, None, &mut root, &mut t);
    t.root = root;
    t
}

/// Nodes of subgraph `si` including nested subgraphs, in sequence order
/// (`agfstnode(subg)`..`agnxtnode`).
pub fn subgraph_member_nodes(graph: &Graph, si: usize) -> Vec<usize> {
    crate::neatogen::neatoinit::subgraph_nodes(graph, si)
}
