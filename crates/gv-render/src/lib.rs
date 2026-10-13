//! Output renderers for `graphvis-rs` (`svg`, `json`, `json0`, `dot`, `xdot`, `canon`, `plain`).

#![forbid(unsafe_code)]

pub mod dot_out;
pub mod json;
pub mod svg;

pub use dot_out::{render_dot_positioned, render_plain};
pub use json::render_json;
pub use svg::render_svg;

use gv_core::{Graph, GraphvizError, Result};
use gv_parser::write_canon;

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
            "xdot" | "xdot1.2" | "xdot1.4" | "xdot_json" => Ok(Self::Xdot),
            "canon" => Ok(Self::Canon),
            "plain" | "plain-ext" => Ok(Self::Plain),
            other => Err(GraphvizError::UnsupportedFormat(other.to_string())),
        }
    }
}

pub fn render_graph(graph: &Graph, format: OutputFormat) -> Result<String> {
    match format {
        OutputFormat::Svg => Ok(render_svg(graph)),
        OutputFormat::Json => render_json(graph, false),
        OutputFormat::Json0 => render_json(graph, true),
        OutputFormat::Dot => Ok(render_dot_positioned(graph, false)),
        OutputFormat::Xdot => Ok(render_dot_positioned(graph, true)),
        OutputFormat::Canon => Ok(write_canon(graph)),
        OutputFormat::Plain => Ok(render_plain(graph)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gv_core::{BezierSegment, BoundingBox, Point, Spline};
    use gv_parser::parse_dot;

    fn sample_laid_out_graph() -> Graph {
        let mut g = parse_dot(
            r##"
            digraph G {
                label="Sample Graph";
                subgraph cluster_0 {
                    label="Cluster A";
                    style=filled;
                    fillcolor="#eeeeee";
                    a [label="Node A", shape=box, style="filled,rounded", fillcolor=lightblue];
                    b [label="{f0|f1|f2}", shape=record];
                }
                c [label=<<b>HTML</b><br/>Node>, shape=diamond];
                a -> b [label="ab"];
                b -> c [arrowhead=vee, style=dashed];
            }
            "##,
        )
        .expect("parse sample graph");

        g.bb = BoundingBox::new(0.0, 0.0, 220.0, 180.0);

        if let Some(sub) = g.subgraphs.first_mut() {
            sub.bb = BoundingBox::new(16.0, 64.0, 204.0, 168.0);
        }

        if let Some(a) = g.nodes.get_mut(0) {
            a.pos = Point::new(60.0, 110.0);
            a.width = 1.0;
            a.height = 0.5;
        }
        if let Some(b) = g.nodes.get_mut(1) {
            b.pos = Point::new(150.0, 110.0);
            b.width = 1.2;
            b.height = 0.5;
        }
        if let Some(c) = g.nodes.get_mut(2) {
            c.pos = Point::new(150.0, 36.0);
            c.width = 1.1;
            c.height = 0.6;
        }

        if let Some(e0) = g.edges.get_mut(0) {
            e0.spline = Spline {
                segments: vec![BezierSegment {
                    points: vec![
                        Point::new(96.0, 110.0),
                        Point::new(102.0, 110.0),
                        Point::new(105.0, 110.0),
                        Point::new(108.0, 110.0),
                    ],
                    start_arrow: None,
                    end_arrow: Some(Point::new(114.0, 110.0)),
                }],
            };
            e0.label_pos = Some(Point::new(105.0, 122.0));
        }

        if let Some(e1) = g.edges.get_mut(1) {
            e1.spline = Spline {
                segments: vec![BezierSegment {
                    points: vec![
                        Point::new(150.0, 92.0),
                        Point::new(150.0, 80.0),
                        Point::new(150.0, 70.0),
                        Point::new(150.0, 62.0),
                    ],
                    start_arrow: None,
                    end_arrow: Some(Point::new(150.0, 58.0)),
                }],
            };
        }

        g
    }

    #[test]
    fn svg_renderer_emits_graphviz_structure_shapes_and_edges() {
        let g = sample_laid_out_graph();
        let svg = render_graph(&g, OutputFormat::Svg).expect("render svg");

        assert!(svg.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>"));
        assert!(svg.contains("<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\""));
        assert!(svg.contains("<svg width=\"228pt\" height=\"188pt\""));
        assert!(svg.contains("viewBox=\"0.00 0.00 228.00 188.00\""));
        assert!(svg.contains(
            "<g id=\"graph0\" class=\"graph\" transform=\"scale(1 1) rotate(0) translate(4 184)\">"
        ));
        assert!(svg.contains("<g id=\"clust1\" class=\"cluster\">"));
        assert!(svg.contains("<g id=\"node1\" class=\"node\">"));
        assert!(svg.contains("<g id=\"node2\" class=\"node\">"));
        assert!(svg.contains("<g id=\"node3\" class=\"node\">"));
        assert!(svg.contains("<g id=\"edge1\" class=\"edge\">"));
        assert!(svg.contains("<g id=\"edge2\" class=\"edge\">"));
        assert!(svg.contains("<title>a&#45;&gt;b</title>"));
        assert!(svg.contains("<path fill=\"none\""));
        assert!(svg.contains("font-weight=\"bold\""));
    }

    #[test]
    fn json_and_json0_render_graphviz_schema() {
        let g = sample_laid_out_graph();
        let json0_str = render_graph(&g, OutputFormat::Json0).expect("render json0");
        let v0: serde_json::Value = serde_json::from_str(&json0_str).expect("valid json0");
        assert_eq!(v0["name"], "G");
        assert_eq!(v0["directed"], true);
        assert_eq!(v0["strict"], false);
        assert_eq!(v0["bb"], "0,0,220,180");
        assert_eq!(v0["_subgraph_cnt"], 1);
        assert!(v0["objects"].is_array());
        assert_eq!(v0["objects"].as_array().unwrap().len(), 4);
        assert!(v0["objects"][0].get("_draw_").is_none());
        assert_eq!(v0["edges"].as_array().unwrap().len(), 2);
        assert!(
            v0["edges"][0]["pos"]
                .as_str()
                .unwrap()
                .starts_with("e,114,110 ")
        );

        let json_str = render_graph(&g, OutputFormat::Json).expect("render json");
        let v: serde_json::Value = serde_json::from_str(&json_str).expect("valid json");
        assert!(v["_draw_"].is_array());
        assert!(v["objects"][0]["_draw_"].is_array());
        assert!(v["objects"][1]["_draw_"].is_array());
        assert!(v["edges"][0]["_draw_"].is_array());
        assert!(v["edges"][0]["_hdraw_"].is_array());
    }

    #[test]
    fn dot_xdot_and_plain_outputs_include_layout_positions() {
        let g = sample_laid_out_graph();

        let dot = render_graph(&g, OutputFormat::Dot).expect("render dot");
        assert!(dot.contains("bb=\"0,0,220,180\""));
        assert!(dot.contains("pos=\"60,110\""));
        assert!(dot.contains("e,114,110 96,110 102,110 105,110 108,110"));
        assert!(!dot.contains("_draw_"));

        let xdot = render_graph(&g, OutputFormat::Xdot).expect("render xdot");
        assert!(xdot.contains("xdotversion=1.7"));
        assert!(xdot.contains("_draw_="));
        assert!(xdot.contains("_ldraw_="));
        assert!(xdot.contains("_hdraw_="));

        let plain = render_graph(&g, OutputFormat::Plain).expect("render plain");
        assert!(plain.starts_with("graph 1 "));
        assert!(plain.contains("\nnode a "));
        assert!(plain.contains("\nnode b "));
        assert!(plain.contains("\nedge a b 4 "));
        assert!(plain.ends_with("stop\n"));
    }
}
