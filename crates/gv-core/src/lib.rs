//! Core data structures, error types, geometry primitives, and tables for `graphvis-rs`.

pub mod color;
mod color_table;
pub mod diag;
pub mod error;
pub mod font;
pub mod geom;
pub mod graph;

pub use color::{
    RgbaColor, format_svg_color, resolve_color, resolve_color_with_scheme, warn_unknown_color,
};
pub use error::{GraphvizError, Result};
pub use font::{
    DEFAULT_FONT_NAME, DEFAULT_FONT_SIZE, DEFAULT_LABEL_FONT_SIZE, LINESPACING, LabelSubstContext,
    TextJustify, TextLabelLayout, TextLineLayout, estimate_text_size, estimate_text_width_1pt,
    measure_multiline_label, measure_multiline_label_with_context,
};
pub use geom::{BezierSegment, BoundingBox, Point, Spline};
pub use graph::{
    AttrDicts, AttrMap, AttrSym, CLUST_NODE_KEY, EdgeData, EdgeId, Graph, GraphKind,
    NO_EDGE_INIT_KEY, NO_GRAPH_LABEL_KEY, NodeData, NodeId, Port, REGISTERED_CLUSTER_KEY, SEQ_KEY,
    SubgraphData, SubgraphId, UNPLACED_PORT_LABELS_KEY, UNREGISTERED_CLUSTER_KEY,
};
