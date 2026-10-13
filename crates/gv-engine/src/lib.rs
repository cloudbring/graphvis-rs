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

    #[test]
    fn smoke_render_svg() {
        let svg = render_dot("digraph G { a -> b; }", "dot", "svg").unwrap();
        assert!(svg.contains("<svg"));
    }
}
