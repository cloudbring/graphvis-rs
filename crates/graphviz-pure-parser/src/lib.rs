//! Re-entrant DOT and HTML-label parser and canonical serializer (`graphviz-pure-parser`).
//! Fixes GV-BUG-001 (global mutable lexer state) and GV-BUG-005 (memory leaks on syntax errors).

pub mod canon;
pub mod lexer;
pub mod parser;

pub use canon::{
    canon_str, canon_str_with, canon_value, is_html_string, write_canon, write_canon_forced,
};
pub use lexer::{Lexer, Token, TokenKind};
pub use parser::{GlobalAttrs, Parser};

use graphviz_pure_core::{Graph, Result};

/// Parses a DOT graph specification into an arena-backed [`Graph`].
pub fn parse_dot(input: &str) -> Result<Graph> {
    Parser::parse_str(input)
}

/// Parses every graph in a DOT stream (`digraph G {..} digraph H {..}`), applying `globals`
/// (`-G`/`-N`/`-E` command line attributes) as if declared at the top of each graph.
pub fn parse_dot_all(input: &str, globals: &GlobalAttrs) -> Result<Vec<Graph>> {
    Parser::parse_all(input, globals)
}

/// [`parse_dot_all`] keeping the graphs read before a syntax error, with the error.
pub fn parse_dot_all_partial(
    input: &str,
    globals: &GlobalAttrs,
) -> (Vec<Graph>, Option<graphviz_pure_core::GraphvizError>) {
    Parser::parse_all_partial(input, globals)
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphviz_pure_core::GraphKind;
    use std::path::PathBuf;

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(name)
    }

    fn read_fixture(name: &str) -> String {
        let path = fixture_path(name);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read fixture {}: {}", path.display(), e))
    }

    #[test]
    fn parse_all_20_fixtures_and_roundtrip_canon() {
        let fixtures = [
            "ER.gv",
            "Heawood.gv",
            "NaN.gv",
            "Petersen.gv",
            "abstract.gv",
            "alf.gv",
            "arrows.gv",
            "clust.gv",
            "clust1.gv",
            "clust2.gv",
            "clust3.gv",
            "clust4.gv",
            "fsm.gv",
            "html.gv",
            "html2.gv",
            "pack.gv",
            "record.gv",
            "shapes.gv",
            "unix.gv",
            "world.gv",
        ];

        for name in fixtures {
            let src = read_fixture(name);
            let graph =
                parse_dot(&src).unwrap_or_else(|e| panic!("failed to parse fixture {name}: {e}"));
            assert!(
                !graph.nodes.is_empty(),
                "fixture {name} should have at least 1 node"
            );

            let canon = write_canon(&graph);
            let reparsed = parse_dot(&canon).unwrap_or_else(|e| {
                panic!("failed to re-parse canonical output for {name}: {e}\nCanon:\n{canon}")
            });
            assert_eq!(
                graph.kind, reparsed.kind,
                "kind mismatch after canon roundtrip for {name}"
            );
            assert_eq!(
                graph.nodes.len(),
                reparsed.nodes.len(),
                "node count mismatch after canon roundtrip for {name}"
            );
            assert_eq!(
                graph.edges.len(),
                reparsed.edges.len(),
                "edge count mismatch after canon roundtrip for {name}"
            );
        }
    }

    #[test]
    fn verify_fsm_fixture() {
        let g = parse_dot(&read_fixture("fsm.gv")).unwrap();
        assert_eq!(g.name, "finite_state_machine");
        assert_eq!(g.kind, GraphKind::Directed);
        assert!(!g.strict);
        assert_eq!(g.nodes.len(), 9);
        assert_eq!(g.edges.len(), 14);
        assert_eq!(g.graph_attrs.get("rankdir").map(String::as_str), Some("LR"));

        let lr0 = g.node_lookup["LR_0"];
        let lr1 = g.node_lookup["LR_1"];
        assert_eq!(
            g.nodes[lr0.0 as usize]
                .attrs
                .get("shape")
                .map(String::as_str),
            Some("doublecircle")
        );
        assert_eq!(
            g.nodes[lr1.0 as usize]
                .attrs
                .get("shape")
                .map(String::as_str),
            Some("circle")
        );
    }

    #[test]
    fn verify_clusters_and_attribute_scoping() {
        let clust = parse_dot(&read_fixture("clust.gv")).unwrap();
        assert_eq!(clust.nodes.len(), 8);
        assert_eq!(clust.edges.len(), 9);
        assert_eq!(clust.subgraphs.len(), 2);
        assert!(clust.subgraphs[0].is_cluster);
        assert_eq!(clust.subgraphs[0].name, "cluster_0");
        assert_eq!(
            clust.subgraphs[0]
                .graph_attrs
                .get("label")
                .map(String::as_str),
            Some("hello world")
        );
        assert_eq!(clust.subgraphs[0].nodes.len(), 3);
        assert_eq!(clust.subgraphs[0].edges.len(), 2);

        let clust4 = parse_dot(&read_fixture("clust4.gv")).unwrap();
        assert_eq!(clust4.nodes.len(), 10);
        assert_eq!(clust4.edges.len(), 13);
        assert_eq!(clust4.subgraphs.len(), 2);
        let a0 = clust4.node_lookup["a0"];
        let b0 = clust4.node_lookup["b0"];
        let start = clust4.node_lookup["start"];
        assert_eq!(
            clust4.nodes[a0.0 as usize]
                .attrs
                .get("color")
                .map(String::as_str),
            Some("white")
        );
        assert_eq!(
            clust4.nodes[b0.0 as usize].attrs.get("color"),
            None,
            "color=white in cluster_0 must not leak into cluster_1"
        );
        assert_eq!(
            clust4.nodes[start.0 as usize]
                .attrs
                .get("shape")
                .map(String::as_str),
            Some("Mdiamond")
        );
    }

    #[test]
    fn canon_distinguishes_quoted_and_html_labels() {
        let g = parse_dot(
            "digraph { a [label=\"<x>\"]; b [label=<<b>x</b>>]; a -> b [label=\"<e>\"]; }",
        )
        .unwrap();
        let canon = write_canon(&g);
        assert!(canon.contains("a\t[label=\"<x>\"];"), "{canon}");
        assert!(canon.contains("b\t[label=<<b>x</b>>];"), "{canon}");
        assert!(canon.contains("a -> b\t[label=\"<e>\"];"), "{canon}");
        assert!(canon.contains("node [label=\"\\N\"];"), "{canon}");
    }

    #[test]
    fn command_line_attrs_are_fixed_at_root() {
        // `global_def` marks the symbols `fixed`: root attribute statements keep
        // the command-line value, subgraphs and individual objects may change it.
        let mut globals = GlobalAttrs::default();
        globals.graph.insert("rankdir".into(), "LR".into());
        globals.node.insert("shape".into(), "box".into());
        let src = "digraph G { rankdir=TB; graph [rankdir=BT]; node [shape=circle]; a; \
                   b [shape=point]; subgraph s { rankdir=RL; node [shape=egg]; c } }";
        let g = &parse_dot_all(src, &globals).unwrap()[0];
        assert_eq!(g.graph_attrs.get("rankdir").map(String::as_str), Some("LR"));
        let shape = |n: usize| g.nodes[n].attrs.get("shape").map(String::as_str);
        assert_eq!(shape(0), Some("box"));
        assert_eq!(shape(1), Some("point"));
        assert_eq!(shape(2), Some("egg"));
        assert_eq!(
            g.subgraphs[0]
                .graph_attrs
                .get("rankdir")
                .map(String::as_str),
            Some("RL")
        );
    }

    #[test]
    fn parse_dot_all_drops_the_rest_of_a_graphs_last_line() {
        // cgraph reads line by line and starts a new scanner per graph.
        let graphs = parse_dot_all(
            "digraph G { a -> b } graph H { c -- d }",
            &GlobalAttrs::default(),
        )
        .unwrap();
        assert_eq!(graphs.len(), 1);
    }

    #[test]
    fn parse_dot_all_returns_every_graph_with_globals() {
        let mut globals = GlobalAttrs::default();
        globals.graph.insert("rankdir".into(), "LR".into());
        globals.node.insert("shape".into(), "box".into());
        globals.edge.insert("color".into(), "red".into());
        let graphs = parse_dot_all("digraph G { a -> b }\ngraph H { c -- d }", &globals).unwrap();
        assert_eq!(graphs.len(), 2);
        assert_eq!(graphs[0].name, "G");
        assert_eq!(graphs[1].name, "H");
        assert_eq!(graphs[1].kind, GraphKind::Undirected);
        for g in &graphs {
            assert_eq!(g.graph_attrs.get("rankdir").map(String::as_str), Some("LR"));
            assert_eq!(
                g.nodes[0].attrs.get("shape").map(String::as_str),
                Some("box")
            );
            assert_eq!(
                g.edges[0].attrs.get("color").map(String::as_str),
                Some("red")
            );
        }
        assert_eq!(
            parse_dot("digraph G { a } digraph H { b }").unwrap().name,
            "G"
        );
        assert!(
            parse_dot_all("", &GlobalAttrs::default())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn canon_orders_out_edges_by_head_sequence() {
        let g = parse_dot("digraph { b; c; a -> c; a -> b; }").unwrap();
        let canon = write_canon(&g);
        assert!(canon.find("a -> b").unwrap() < canon.find("a -> c").unwrap());
    }

    #[test]
    fn verify_html_labels_and_ports() {
        let html = parse_dot(&read_fixture("html.gv")).unwrap();
        assert_eq!(html.nodes.len(), 3);
        assert_eq!(html.edges.len(), 2);
        for node in &html.nodes {
            let label = node.attrs.get("label").expect("missing label");
            assert!(is_html_string(label), "expected HTML label, got {label}");
            assert!(label.contains("<TABLE"));
        }
        assert_eq!(html.edges[0].tail_port.name.as_deref(), Some("f1"));
        assert_eq!(html.edges[0].head_port.name.as_deref(), Some("f0"));
        assert_eq!(html.edges[1].tail_port.name.as_deref(), Some("f2"));
        assert_eq!(html.edges[1].head_port.name.as_deref(), Some("here"));

        let html2 = parse_dot(&read_fixture("html2.gv")).unwrap();
        assert_eq!(html2.nodes.len(), 4);
        assert_eq!(html2.edges.len(), 3);
        assert_eq!(html2.subgraphs.len(), 1);
        assert_eq!(html2.edges[0].tail_port.name.as_deref(), Some("here"));
        assert_eq!(html2.edges[0].head_port.name.as_deref(), Some("there"));
        let edge_label = html2.edges[2]
            .attrs
            .get("label")
            .expect("missing edge label");
        assert!(is_html_string(edge_label));
    }

    #[test]
    fn verify_er_heawood_petersen_pack_world() {
        let er = parse_dot(&read_fixture("ER.gv")).unwrap();
        assert_eq!(er.kind, GraphKind::Undirected);
        assert_eq!(er.nodes.len(), 12);
        assert_eq!(er.edges.len(), 12);
        let name0 = er.node_lookup["name0"];
        let code = er.node_lookup["code"];
        assert_eq!(
            er.nodes[name0.0 as usize]
                .attrs
                .get("label")
                .map(String::as_str),
            Some("name")
        );
        assert_eq!(
            er.nodes[code.0 as usize]
                .attrs
                .get("label")
                .map(String::as_str),
            Some("\\N")
        );

        let heawood = parse_dot(&read_fixture("Heawood.gv")).unwrap();
        assert_eq!(heawood.name, "Heawood");
        assert_eq!(heawood.nodes.len(), 14);
        assert_eq!(heawood.edges.len(), 21);

        let petersen = parse_dot(&read_fixture("Petersen.gv")).unwrap();
        assert_eq!(petersen.name, "Petersen");
        assert_eq!(petersen.nodes.len(), 10);
        assert_eq!(petersen.edges.len(), 15);
        assert_eq!(
            petersen.edges[0].attrs.get("color").map(String::as_str),
            Some("blue")
        );

        let pack = parse_dot(&read_fixture("pack.gv")).unwrap();
        assert_eq!(pack.nodes.len(), 24);
        assert_eq!(pack.edges.len(), 20);

        let world = parse_dot(&read_fixture("world.gv")).unwrap();
        assert_eq!(world.subgraphs.len(), 14);
        assert!(world.edges.len() > 50);
    }

    #[test]
    fn verify_strict_concat_compass_and_syntax_errors() {
        let dot = r#"
            # Preprocessor comment
            strict digraph "My" + "Graph" {
                a:p1:ne -> b:p2:sw [label="hello " + "world"][color=red];
                a:p1:ne -> b:p2:sw [weight=2];
                { x, y } -> { z };
            }
        "#;
        let g = parse_dot(dot).unwrap();
        assert!(g.strict);
        assert_eq!(g.name, "MyGraph");
        assert_eq!(g.edges.len(), 3); // strict merges duplicate a->b, plus x->z and y->z
        assert_eq!(g.edges[0].tail_port.name.as_deref(), Some("p1"));
        assert_eq!(g.edges[0].tail_port.compass.as_deref(), Some("ne"));
        assert_eq!(g.edges[0].head_port.name.as_deref(), Some("p2"));
        assert_eq!(g.edges[0].head_port.compass.as_deref(), Some("sw"));
        assert_eq!(
            g.edges[0].attrs.get("label").map(String::as_str),
            Some("hello world")
        );
        assert_eq!(
            g.edges[0].attrs.get("weight").map(String::as_str),
            Some("2")
        );

        // Directed edge in undirected graph must fail
        assert!(parse_dot("graph G { a -> b }").is_err());
        // Undirected edge in directed graph must fail
        assert!(parse_dot("digraph G { a -- b }").is_err());
        // Unclosed HTML string must fail
        assert!(parse_dot("digraph G { a [label=<<table>] }").is_err());
    }
}
