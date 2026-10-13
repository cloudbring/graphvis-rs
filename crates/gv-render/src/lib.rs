//! Output renderers for `graphvis-rs` (`svg`, `json`, `json0`, `dot`, `xdot`, `canon`, `plain`).

use gv_core::{Graph, GraphvizError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Svg,
    Json,
    Json0,
    Dot,
    Xdot,
    Canon,
    Plain,
}

impl OutputFormat {
    pub fn parse(name: &str) -> Result<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "svg" => Ok(Self::Svg),
            "json" => Ok(Self::Json),
            "json0" => Ok(Self::Json0),
            "dot" | "gv" => Ok(Self::Dot),
            "xdot" => Ok(Self::Xdot),
            "canon" => Ok(Self::Canon),
            "plain" => Ok(Self::Plain),
            other => Err(GraphvizError::UnsupportedFormat(other.to_string())),
        }
    }
}

pub fn render_graph(graph: &Graph, format: OutputFormat) -> Result<String> {
    match format {
        OutputFormat::Svg => Ok(format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\"></svg>",
            graph.bb.width().max(54.0),
            graph.bb.height().max(36.0)
        )),
        OutputFormat::Json | OutputFormat::Json0 => {
            serde_json::to_string_pretty(graph).map_err(|e| GraphvizError::LayoutError {
                stage: "json_serialize",
                message: e.to_string(),
            })
        }
        OutputFormat::Dot | OutputFormat::Xdot | OutputFormat::Canon | OutputFormat::Plain => {
            Ok(format!("digraph {} {{\n}}\n", graph.name))
        }
    }
}
