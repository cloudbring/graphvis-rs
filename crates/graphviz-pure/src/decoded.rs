//! The entry points of the crate root for text already in the port's decoded form
//! ([`charset`]): file bytes read with [`charset::decode`], or text converted with
//! [`charset::from_text`]. What they return is decoded too: [`charset::encode`] gives the bytes
//! C writes, [`charset::to_text`] ordinary text. The command line and the wasm bindings use
//! these, so nothing is converted twice.
//!
//! [`charset`]: crate::charset
//! [`charset::decode`]: crate::charset::decode
//! [`charset::from_text`]: crate::charset::from_text
//! [`charset::encode`]: crate::charset::encode
//! [`charset::to_text`]: crate::charset::to_text

use crate::{
    GlobalAttrs, Graph, LayoutEngine, LayoutOptions, OutputFormat, Result, in_rand_run,
    layout_graph_in_run,
};

pub use graphviz_pure_parser::{GraphStream, parse_dot, parse_dot_all};
pub use graphviz_pure_render::{GraphJobs, render_graph, render_graph_named};

/// [`crate::render_dot`] on decoded text: renders the first graph in `dot_source`.
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
        &mut LayoutOptions::default(),
    )
}

/// [`crate::render_dot_all`] on decoded text: renders every graph in `dot_source`, applying
/// `globals` (`-G`/`-N`/`-E`, decoded too) before each graph body.
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
    let graphs = parse_dot_all(dot_source, globals)?;
    // One call is one C process: its graphs share the C library's generator states (the
    // caller's, inside a `with_rand_state`) and `Nop`.
    let mut options = options;
    in_rand_run(|| {
        graphs
            .into_iter()
            .map(|graph| render_parsed(graph, engine, output_format, format, &mut options))
            .collect()
    })
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
    // One call is one C process: its graphs share the C library's generator states (the
    // caller's, inside a `with_rand_state`) and `Nop`.
    let mut options = options;
    let err = in_rand_run(|| {
        for graph in graphs {
            match render_parsed(graph, engine, output_format, format, &mut options) {
                Ok(s) => out.push(s),
                Err(e) => return Some(e),
            }
        }
        None
    });
    (out, err.or(parse_err))
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
    // One call is one C process: its graphs share the C library's generator states (the
    // caller's, inside a `with_rand_state`) and `Nop`.
    let mut options = options;
    let err = in_rand_run(|| {
        for graph in graphs {
            let explicit_dpi = graphviz_pure_core::cnum::drawing_dpi(&graph.graph_attrs) > 0.0;
            match render_parsed(graph, engine, OutputFormat::Svg, "svg", &mut options) {
                Ok(s) => out.push((s, explicit_dpi)),
                Err(e) => return Some(e),
            }
        }
        None
    });
    (out, err.or(parse_err))
}

fn render_parsed(
    mut graph: Graph,
    engine: &str,
    output_format: OutputFormat,
    format_name: &str,
    options: &mut LayoutOptions,
) -> Result<String> {
    let engine = LayoutEngine::parse(engine)?;
    // As in C (`gvLayoutJobs`), a graph's `layout` attribute overrides the requested engine.
    let layout_engine = match graph.graph_attrs.get("layout") {
        Some(name) if !name.trim().is_empty() => LayoutEngine::parse(name.trim())?,
        _ => engine,
    };
    // `dot` lays the graph out for every format, `canon` and the json formats without
    // layout attributes included: they write the graph as the layout left it.
    layout_graph_in_run(&mut graph, layout_engine, options)?;
    render_graph_named(&graph, output_format, format_name)
}
