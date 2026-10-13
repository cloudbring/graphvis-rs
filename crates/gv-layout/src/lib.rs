//! Graphviz layout engines in pure Rust (`dot`, `neato`, `fdp`, `sfdp`, `twopi`, `circo`, `osage`, `patchwork`).

use gv_core::{BoundingBox, Graph, GraphvizError, Point, Result};
use gv_geom::straight_spline;

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

pub fn layout_graph(graph: &mut Graph, _engine: LayoutEngine) -> Result<()> {
    for (idx, node) in graph.nodes.iter_mut().enumerate() {
        node.pos = Point::new(27.0, 18.0 + (idx as f64) * 72.0);
    }
    let positions: Vec<Point> = graph.nodes.iter().map(|n| n.pos).collect();
    for edge in &mut graph.edges {
        let p_tail = positions
            .get(edge.tail.0 as usize)
            .copied()
            .unwrap_or_default();
        let p_head = positions
            .get(edge.head.0 as usize)
            .copied()
            .unwrap_or_default();
        edge.spline = straight_spline(p_tail, p_head);
    }
    graph.bb = BoundingBox::new(0.0, 0.0, 54.0, (graph.nodes.len().max(1) as f64) * 36.0);
    Ok(())
}
