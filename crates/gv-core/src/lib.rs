//! Core data structures, error types, geometry primitives, and tables for `graphvis-rs`.

pub mod error;
pub mod geom;
pub mod graph;

pub use error::{GraphvizError, Result};
pub use geom::{BezierSegment, BoundingBox, Point, Spline};
pub use graph::{
    AttrMap, EdgeData, EdgeId, Graph, GraphKind, NodeData, NodeId, Port, SubgraphData, SubgraphId,
};
