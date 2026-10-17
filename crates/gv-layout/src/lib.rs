//! Graphviz layout engines in pure Rust (`dot`, `neato`, `fdp`, `sfdp`, `twopi`, `circo`, `osage`, `patchwork`).

pub mod circogen;
pub mod common;
pub mod cutil;
pub mod dot;
pub mod dotgen;
pub mod force_circular;
pub mod neatogen;
pub mod pack;
pub mod sfdpgen;
pub mod sparse;
pub mod twopigen;

use gv_core::{Graph, GraphvizError, Result};

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
            other => Err(GraphvizError::UnsupportedEngine(other.to_string())),
        }
    }
}

pub fn layout_graph(graph: &mut Graph, engine: LayoutEngine) -> Result<()> {
    let effective_engine = if engine == LayoutEngine::Dot
        && let Some(attr_layout) = graph.graph_attrs.get("layout")
        && let Ok(parsed) = LayoutEngine::parse(attr_layout)
    {
        parsed
    } else {
        engine
    };

    match effective_engine {
        LayoutEngine::Dot => {
            // `GV_DOT_LEGACY=1` selects the previous approximate engine for comparison.
            if std::env::var_os("GV_DOT_LEGACY").is_some() {
                dot::layout_dot(graph)
            } else {
                dotgen::layout_dot(graph)
            }
        }
        LayoutEngine::Neato => neatogen::neatoinit::neato_layout(graph),
        LayoutEngine::Fdp => force_circular::layout_fdp(graph, false),
        LayoutEngine::Sfdp => force_circular::layout_fdp(graph, true),
        LayoutEngine::Twopi => twopigen::twopiinit::twopi_layout(graph),
        LayoutEngine::Circo => circogen::circularinit::circo_layout(graph),
        LayoutEngine::Osage => force_circular::layout_osage(graph),
        LayoutEngine::Patchwork => force_circular::layout_patchwork(graph),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gv_core::{AttrMap, Graph, GraphKind, Point, SubgraphData, SubgraphId};

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
            for edge in &g.edges {
                assert!(!edge.spline.segments.is_empty());
            }
        }
    }
}
