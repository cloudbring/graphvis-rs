//! Graphviz layout engines in pure Rust (`dot`, `neato`, `fdp`, `sfdp`, `twopi`, `circo`, `osage`, `patchwork`).

pub mod cdt;
pub mod circogen;
pub mod common;
pub mod cutil;
pub mod dotgen;
pub mod fdpgen;
pub mod neatogen;
pub mod nop;
pub mod ortho;
pub mod osage;
pub mod pack;
pub mod patchwork;
pub mod sfdpgen;
pub mod sparse;
pub mod twopigen;
pub mod vpsc;

use graphviz_pure_core::{Graph, GraphvizError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutEngine {
    Dot,
    Neato,
    Fdp,
    Sfdp,
    Twopi,
    Circo,
    Osage,
    Patchwork,
    /// `nop`/`nop1`: neato with `-n1`.
    Nop1,
    /// `nop2`: neato with `-n2`.
    Nop2,
}

/// Engine options set on the command line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LayoutOptions {
    /// `-n[N]`: neato keeps the input positions (`Nop` in C). 0 runs the full layout.
    pub nop: u8,
}

impl LayoutEngine {
    pub fn parse(name: &str) -> Result<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "dot" => Ok(Self::Dot),
            "neato" => Ok(Self::Neato),
            "fdp" => Ok(Self::Fdp),
            "sfdp" => Ok(Self::Sfdp),
            "twopi" => Ok(Self::Twopi),
            "circo" => Ok(Self::Circo),
            "osage" => Ok(Self::Osage),
            "patchwork" => Ok(Self::Patchwork),
            "nop" | "nop1" => Ok(Self::Nop1),
            "nop2" => Ok(Self::Nop2),
            other => Err(GraphvizError::UnsupportedEngine(other.to_string())),
        }
    }
}

pub fn layout_graph(graph: &mut Graph, engine: LayoutEngine) -> Result<()> {
    layout_graph_with_options(graph, engine, LayoutOptions::default())
}

pub fn layout_graph_with_options(
    graph: &mut Graph,
    engine: LayoutEngine,
    options: LayoutOptions,
) -> Result<()> {
    common::splines::reset_bz_split_budget();
    graphviz_pure_core::charset::begin_layout(
        &graph.name,
        graph.graph_attrs.get("charset").map(String::as_str),
    );
    // Only dot exports its `GD_clust` lists; to C the key is an ordinary attribute, so a
    // value from the input must not reorder the clusters of another engine's drawing.
    let key = graphviz_pure_core::CLUSTER_LIST_KEY;
    graph.graph_attrs.remove(key);
    for s in &mut graph.subgraphs {
        s.graph_attrs.remove(key);
    }
    // graph_init's do_graph_label(g) (lib/common/input.c:711) makes the root label before
    // any engine's node and edge init, so its diagnostics come first; the engines' own
    // calls do not repeat them.
    let _ = graphviz_pure_geom::graph_label_geometry_in(
        &graph.name,
        &graph.graph_attrs,
        true,
        false,
        &graphviz_pure_geom::NodeSizeEnv::from_graph(graph),
    );
    let effective_engine = if engine == LayoutEngine::Dot
        && let Some(attr_layout) = graph.graph_attrs.get("layout")
        && let Ok(parsed) = LayoutEngine::parse(attr_layout)
    {
        parsed
    } else {
        engine
    };
    // A layout of the same graph must not keep an earlier dot layout's lw/rw split.
    for n in &mut graph.nodes {
        n.attrs.remove(graphviz_pure_core::NODE_LW_RW_KEY);
    }

    match effective_engine {
        LayoutEngine::Dot => dotgen::layout_dot(graph),
        LayoutEngine::Neato if options.nop > 0 => nop::layout_nop(graph, options.nop),
        LayoutEngine::Nop1 => nop::layout_nop(graph, 1),
        LayoutEngine::Nop2 => nop::layout_nop(graph, 2),
        LayoutEngine::Neato => neatogen::neatoinit::neato_layout(graph),
        LayoutEngine::Fdp => fdpgen::layout::fdp_layout(graph),
        LayoutEngine::Sfdp => sfdpgen::sfdpinit::sfdp_layout(graph),
        LayoutEngine::Twopi => twopigen::twopiinit::twopi_layout(graph),
        LayoutEngine::Circo => circogen::circularinit::circo_layout(graph),
        LayoutEngine::Osage => osage::osage_layout(graph),
        LayoutEngine::Patchwork => patchwork::patchwork_layout(graph),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphviz_pure_core::{AttrMap, Graph, GraphKind, Point, SubgraphData, SubgraphId};

    fn make_chain_graph() -> Graph {
        let mut g = Graph::new("G", GraphKind::Directed, false);
        let a = g.ensure_node("a");
        let b = g.ensure_node("b");
        let c = g.ensure_node("c");
        g.add_edge(a, b, AttrMap::new());
        g.add_edge(b, c, AttrMap::new());
        g
    }

    #[test]
    fn relayout_drops_the_previous_lw_rw_split() {
        let mut g = Graph::new("G", GraphKind::Directed, false);
        let a = g.ensure_node("a");
        g.add_edge(a, a, AttrMap::new());
        let key = graphviz_pure_core::NODE_LW_RW_KEY;
        g.graph_attrs.insert("splines".into(), String::new());
        layout_graph(&mut g, LayoutEngine::Dot).unwrap();
        assert!(g.nodes[a.0 as usize].attrs.contains_key(key));
        g.graph_attrs.insert("splines".into(), "true".into());
        layout_graph(&mut g, LayoutEngine::Dot).unwrap();
        assert!(!g.nodes[a.0 as usize].attrs.contains_key(key));
    }

    #[test]
    fn test_engine_parse() {
        assert_eq!(LayoutEngine::parse("dot").unwrap(), LayoutEngine::Dot);
        assert_eq!(LayoutEngine::parse("NEATO").unwrap(), LayoutEngine::Neato);
        assert_eq!(LayoutEngine::parse("fdp").unwrap(), LayoutEngine::Fdp);
        assert_eq!(LayoutEngine::parse("sfdp").unwrap(), LayoutEngine::Sfdp);
        assert_eq!(LayoutEngine::parse("twopi").unwrap(), LayoutEngine::Twopi);
        assert_eq!(LayoutEngine::parse("circo").unwrap(), LayoutEngine::Circo);
        assert_eq!(LayoutEngine::parse("osage").unwrap(), LayoutEngine::Osage);
        assert_eq!(
            LayoutEngine::parse("patchwork").unwrap(),
            LayoutEngine::Patchwork
        );
        assert!(LayoutEngine::parse("unknown").is_err());
    }

    #[test]
    fn test_dot_rank_assignment_and_tb_coordinates() {
        let mut g = make_chain_graph();
        layout_graph(&mut g, LayoutEngine::Dot).unwrap();

        assert_eq!(g.nodes[0].rank, 0);
        assert_eq!(g.nodes[1].rank, 1);
        assert_eq!(g.nodes[2].rank, 2);

        // In default TB rankdir, rank 0 is at top (highest Y) and rank 2 is at bottom (lowest Y).
        assert!(g.nodes[0].pos.y > g.nodes[1].pos.y);
        assert!(g.nodes[1].pos.y > g.nodes[2].pos.y);
        // Single-chain nodes should be vertically aligned on X.
        assert!((g.nodes[0].pos.x - g.nodes[1].pos.x).abs() < 1e-4);
        assert!((g.nodes[1].pos.x - g.nodes[2].pos.x).abs() < 1e-4);

        // Graph bounding box lower-left must be normalized to (0, 0).
        assert_eq!(g.bb.ll, Point::new(0.0, 0.0));
        assert!(g.bb.width() > 0.0 && g.bb.height() > 0.0);
    }

    #[test]
    fn test_dot_rankdir_variants() {
        // BT: rank 0 at bottom (smaller Y), rank 2 at top (larger Y)
        let mut g_bt = make_chain_graph();
        g_bt.graph_attrs
            .insert("rankdir".to_string(), "BT".to_string());
        layout_graph(&mut g_bt, LayoutEngine::Dot).unwrap();
        assert!(g_bt.nodes[0].pos.y < g_bt.nodes[1].pos.y);
        assert!(g_bt.nodes[1].pos.y < g_bt.nodes[2].pos.y);

        // LR: rank 0 on left (smaller X), rank 2 on right (larger X)
        let mut g_lr = make_chain_graph();
        g_lr.graph_attrs
            .insert("rankdir".to_string(), "LR".to_string());
        layout_graph(&mut g_lr, LayoutEngine::Dot).unwrap();
        assert!(g_lr.nodes[0].pos.x < g_lr.nodes[1].pos.x);
        assert!(g_lr.nodes[1].pos.x < g_lr.nodes[2].pos.x);

        // RL: rank 0 on right (larger X), rank 2 on left (smaller X)
        let mut g_rl = make_chain_graph();
        g_rl.graph_attrs
            .insert("rankdir".to_string(), "RL".to_string());
        layout_graph(&mut g_rl, LayoutEngine::Dot).unwrap();
        assert!(g_rl.nodes[0].pos.x > g_rl.nodes[1].pos.x);
        assert!(g_rl.nodes[1].pos.x > g_rl.nodes[2].pos.x);
    }

    #[test]
    fn test_dot_cycle_breaking_and_rank_same() {
        let mut g = Graph::new("Cycle", GraphKind::Directed, false);
        let a = g.ensure_node("a");
        let b = g.ensure_node("b");
        let c = g.ensure_node("c");
        let d = g.ensure_node("d");

        let mut heavy = AttrMap::new();
        heavy.insert("weight".to_string(), "10".to_string());
        g.add_edge(a, b, heavy.clone());
        g.add_edge(b, c, heavy);
        // Light back-edge c -> a should be reversed during cycle breaking.
        g.add_edge(c, a, AttrMap::new());
        g.add_edge(b, d, AttrMap::new());

        // Constrain c and d to rank=same.
        let mut sub_attrs = AttrMap::new();
        sub_attrs.insert("rank".to_string(), "same".to_string());
        g.subgraphs.push(SubgraphData {
            id: SubgraphId(0),
            name: "same_cd".to_string(),
            is_cluster: false,
            graph_attrs: sub_attrs,
            node_defaults: AttrMap::new(),
            edge_defaults: AttrMap::new(),
            nodes: vec![c, d],
            edges: Vec::new(),
            subgraphs: Vec::new(),
            bb: Default::default(),
        });

        layout_graph(&mut g, LayoutEngine::Dot).unwrap();
        assert!(g.nodes[a.0 as usize].rank < g.nodes[b.0 as usize].rank);
        assert!(g.nodes[b.0 as usize].rank < g.nodes[c.0 as usize].rank);
        assert_eq!(g.nodes[c.0 as usize].rank, g.nodes[d.0 as usize].rank);
    }

    #[test]
    fn test_dot_crossing_minimization() {
        // Construct a 2-layer bipartite graph where initial order (a1, a2) -> (b1, b2)
        // has edges a1->b2 and a2->b1 that would cross unless mincross reorders one rank.
        let mut g = Graph::new("Cross", GraphKind::Directed, false);
        let a1 = g.ensure_node("a1");
        let a2 = g.ensure_node("a2");
        let b1 = g.ensure_node("b1");
        let b2 = g.ensure_node("b2");
        g.add_edge(a1, b2, AttrMap::new());
        g.add_edge(a2, b1, AttrMap::new());

        layout_graph(&mut g, LayoutEngine::Dot).unwrap();
        // Either a1 is to the left of a2 AND b2 is to the left of b1, or vice versa (0 crossings).
        let a_diff = g.nodes[a1.0 as usize].pos.x - g.nodes[a2.0 as usize].pos.x;
        let b_diff = g.nodes[b2.0 as usize].pos.x - g.nodes[b1.0 as usize].pos.x;
        assert!(a_diff * b_diff > 0.0);
    }

    #[test]
    fn test_all_eight_layout_engines_succeed() {
        let engines = [
            LayoutEngine::Dot,
            LayoutEngine::Neato,
            LayoutEngine::Fdp,
            LayoutEngine::Sfdp,
            LayoutEngine::Twopi,
            LayoutEngine::Circo,
            LayoutEngine::Osage,
            LayoutEngine::Patchwork,
        ];

        for engine in engines {
            let mut g = Graph::new("TestAll", GraphKind::Directed, false);
            let a = g.ensure_node("a");
            let b = g.ensure_node("b");
            let c = g.ensure_node("c");
            let d = g.ensure_node("d");
            g.add_edge(a, b, AttrMap::new());
            g.add_edge(b, c, AttrMap::new());
            g.add_edge(c, d, AttrMap::new());
            g.add_edge(d, a, AttrMap::new());

            layout_graph(&mut g, engine).unwrap();
            assert_eq!(g.bb.ll, Point::new(0.0, 0.0));
            assert!(g.bb.width() > 0.0 && g.bb.height() > 0.0);
            for node in &g.nodes {
                assert!(node.pos.x.is_finite() && node.pos.y.is_finite());
                assert!(node.pos.x >= 0.0 && node.pos.y >= 0.0);
            }
            // patchwork draws no edges, as in C.
            if engine != LayoutEngine::Patchwork {
                for edge in &g.edges {
                    assert!(!edge.spline.segments.is_empty());
                }
            }
        }
    }
}
