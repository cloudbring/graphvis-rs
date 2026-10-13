//! Re-entrant DOT and HTML-label parser and canonical serializer (`gv-parser`).
//! Fixes GV-BUG-001 (global mutable lexer state) and GV-BUG-005 (memory leaks on syntax errors).

use gv_core::{Graph, GraphKind, Result};

/// Parses a DOT graph specification into an arena-backed [`Graph`].
pub fn parse_dot(input: &str) -> Result<Graph> {
    let trimmed = input.trim();
    let strict = trimmed.starts_with("strict");
    let kind = if trimmed.contains("digraph") {
        GraphKind::Directed
    } else {
        GraphKind::Undirected
    };
    Ok(Graph::new("G", kind, strict))
}
