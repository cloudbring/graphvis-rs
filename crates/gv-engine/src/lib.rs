//! High-level Graphviz orchestration engine (`gv-engine`).

pub use gv_core::{Graph, GraphvizError, Result};
pub use gv_layout::{LayoutEngine, layout_graph};
pub use gv_parser::parse_dot;
pub use gv_render::{OutputFormat, render_graph};

/// End-to-end DOT rendering pipeline for any supported engine and output format.
pub fn render_dot(dot_source: &str, engine: &str, format: &str) -> Result<String> {
    let layout_engine = LayoutEngine::parse(engine)?;
    let output_format = OutputFormat::parse(format)?;
    let mut graph = parse_dot(dot_source)?;
    if output_format != OutputFormat::Canon {
        layout_graph(&mut graph, layout_engine)?;
    }
    render_graph(&graph, output_format)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn smoke_render_svg() {
        let svg = render_dot("digraph G { a -> b; }", "dot", "svg").unwrap();
        assert!(svg.contains("<svg"));
    }

    #[test]
    fn test_multigraph_and_duplicate_cluster_scopes() {
        // 1. Multi-graph DOT stream (`multi.gv`)
        let multi = "digraph G { a -> b; }\ndigraph H { c -> d; }\n";
        let svg = render_dot(multi, "dot", "svg").unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("<title>a</title>"));

        // 2. Same cluster name in different parent scopes (`b7.gv`)
        let b7 = r#"
            digraph G {
                subgraph cluster_0 {
                    subgraph cluster_1 {}
                    a -> c;
                }
                subgraph cluster_1 {
                    y -> z;
                    y -> q;
                }
                y -> b;
            }
        "#;
        let g = parse_dot(b7).unwrap();
        assert_eq!(g.subgraphs.len(), 3);
        let j0 = render_dot(b7, "dot", "json0").unwrap();
        assert!(j0.contains("\"_subgraph_cnt\": 3"));
    }

    #[test]
    fn test_wild_corpus_renders_without_error() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let wild_dir = manifest_dir.join("../../tests/wild");
        let mut entries: Vec<PathBuf> = fs::read_dir(&wild_dir)
            .expect("tests/wild directory must exist")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|ext| ext == "gv"))
            .collect();
        entries.sort();
        assert!(
            entries.len() >= 30,
            "expected at least 30 wild corpus fixtures"
        );

        for path in entries {
            let src = fs::read_to_string(&path).unwrap();
            let svg = render_dot(&src, "dot", "svg")
                .unwrap_or_else(|e| panic!("Failed rendering SVG for {}: {e}", path.display()));
            assert!(svg.contains("<svg"), "Missing <svg in {}", path.display());
            assert!(
                !svg.contains("NaN") && !svg.contains("Infinity"),
                "Non-finite coordinate in {}",
                path.display()
            );

            let j0 = render_dot(&src, "dot", "json0")
                .unwrap_or_else(|e| panic!("Failed rendering json0 for {}: {e}", path.display()));
            assert!(j0.contains("\"bb\":"), "Missing bb in {}", path.display());
        }
    }
}
