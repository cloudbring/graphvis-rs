//! High-level Graphviz orchestration engine (`graphviz-pure`).

pub use graphviz_pure_core::{Graph, GraphvizError, Result};
pub use graphviz_pure_layout::{
    LayoutEngine, LayoutOptions, layout_graph, layout_graph_with_options,
};
pub use graphviz_pure_parser::{GlobalAttrs, parse_dot, parse_dot_all};
pub use graphviz_pure_render::{OutputFormat, render_graph, render_graph_named};

/// End-to-end DOT rendering pipeline for any supported engine and output format. Renders the
/// first graph in `dot_source`.
pub fn render_dot(dot_source: &str, engine: &str, format: &str) -> Result<String> {
    let output_format = OutputFormat::parse(format)?;
    // Each call is one C process: per-process counters (gradient and anchor ids) restart.
    graphviz_pure_render::reset_render_state();
    let graph = parse_dot(dot_source)?;
    render_parsed(
        graph,
        engine,
        output_format,
        format,
        LayoutOptions::default(),
    )
}

/// Renders every graph in `dot_source`, applying `globals` (`-G`/`-N`/`-E`) before each graph body.
pub fn render_dot_all(
    dot_source: &str,
    engine: &str,
    format: &str,
    globals: &GlobalAttrs,
) -> Result<Vec<String>> {
    render_dot_all_with_options(
        dot_source,
        engine,
        format,
        globals,
        LayoutOptions::default(),
    )
}

/// [`render_dot_all`] with engine options such as `-n` (keep input positions).
pub fn render_dot_all_with_options(
    dot_source: &str,
    engine: &str,
    format: &str,
    globals: &GlobalAttrs,
    options: LayoutOptions,
) -> Result<Vec<String>> {
    let output_format = OutputFormat::parse(format)?;
    graphviz_pure_render::reset_render_state();
    parse_dot_all(dot_source, globals)?
        .into_iter()
        .map(|graph| render_parsed(graph, engine, output_format, format, options))
        .collect()
}

/// [`render_dot_all_with_options`] that renders the graphs read before a syntax error
/// and returns that error alongside, as `dot` does.
pub fn render_dot_all_partial(
    dot_source: &str,
    engine: &str,
    format: &str,
    globals: &GlobalAttrs,
    options: LayoutOptions,
) -> (Vec<String>, Option<graphviz_pure_core::GraphvizError>) {
    let output_format = match OutputFormat::parse(format) {
        Ok(f) => f,
        Err(e) => return (Vec::new(), Some(e)),
    };
    graphviz_pure_render::reset_render_state();
    let (graphs, parse_err) = graphviz_pure_parser::parse_dot_all_partial(dot_source, globals);
    let mut out = Vec::with_capacity(graphs.len());
    for graph in graphs {
        match render_parsed(graph, engine, output_format, format, options) {
            Ok(s) => out.push(s),
            Err(e) => return (out, Some(e)),
        }
    }
    (out, parse_err)
}

/// Lays out and renders every graph in `dot_source` as SVG, for raster output: each SVG comes
/// with whether its graph sets `dpi` (or `resolution`), since Graphviz then scales the drawing
/// to that resolution instead of drawing raster formats at 96 dpi. Like
/// [`render_dot_all_partial`], the graphs read before a syntax error are returned with it.
pub fn render_svg_all_partial(
    dot_source: &str,
    engine: &str,
    globals: &GlobalAttrs,
    options: LayoutOptions,
) -> (
    Vec<(String, bool)>,
    Option<graphviz_pure_core::GraphvizError>,
) {
    graphviz_pure_render::reset_render_state();
    let (graphs, parse_err) = graphviz_pure_parser::parse_dot_all_partial(dot_source, globals);
    let mut out = Vec::with_capacity(graphs.len());
    for graph in graphs {
        // GD_drawing(g)->dpi = late_double(dpi, late_double(resolution, 0, 0), 0) > 0.
        let late_double = |k: &str, dflt: f64| match graph
            .graph_attrs
            .get(k)
            .filter(|v| !v.is_empty())
            .and_then(|v| graphviz_pure_core::strtod::strtod_rest(v))
        {
            Some((v, _)) if v < 0.0 => 0.0,
            Some((v, _)) => v,
            None => dflt,
        };
        let explicit_dpi = late_double("dpi", late_double("resolution", 0.0)) > 0.0;
        match render_parsed(graph, engine, OutputFormat::Svg, "svg", options) {
            Ok(s) => out.push((s, explicit_dpi)),
            Err(e) => return (out, Some(e)),
        }
    }
    (out, parse_err)
}

fn render_parsed(
    mut graph: Graph,
    engine: &str,
    output_format: OutputFormat,
    format_name: &str,
    options: LayoutOptions,
) -> Result<String> {
    // As in C (`gvLayoutJobs`), a graph's `layout` attribute overrides the requested engine.
    let layout_engine = match graph.graph_attrs.get("layout") {
        Some(name) if !name.trim().is_empty() => LayoutEngine::parse(name.trim())?,
        _ => LayoutEngine::parse(engine)?,
    };
    let lname = format_name.to_ascii_lowercase();
    if output_format != OutputFormat::Canon && lname != "dot_json" && lname != "xdot_json" {
        layout_graph_with_options(&mut graph, layout_engine, options)?;
    }
    render_graph_named(&graph, output_format, format_name)
}

#[cfg(test)]
mod tests {

    #[test]
    fn repeated_renders_are_identical() {
        // Gradient and HTML anchor ids are per-process statics in C; a library call must
        // not see the counts left by an earlier call.
        let src = r#"digraph { a [style=filled, fillcolor="red:blue"];
            b [label=<<TABLE><TR><TD HREF="x">c</TD></TR></TABLE>>]; a -> b }"#;
        let first = render_dot(src, "dot", "svg").unwrap();
        assert_eq!(first, render_dot(src, "dot", "svg").unwrap());
        assert!(first.contains("_l_0"));
    }

    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn smoke_render_svg() {
        let svg = render_dot("digraph G { a -> b; }", "dot", "svg").unwrap();
        assert!(svg.contains("<svg"));
    }

    #[test]
    fn render_dot_all_renders_every_graph_with_globals() {
        let mut globals = GlobalAttrs::default();
        globals.node.insert("shape".into(), "box".into());
        let outs = render_dot_all(
            "digraph G { a -> b }\ndigraph H { c -> d }",
            "dot",
            "svg",
            &globals,
        )
        .unwrap();
        assert_eq!(outs.len(), 2);
        assert!(outs[0].contains("<title>a</title>") && !outs[0].contains("<ellipse"));
        assert!(outs[1].contains("<title>c</title>"));
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
            let src = graphviz_pure_core::charset::decode(fs::read(&path).unwrap());
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
