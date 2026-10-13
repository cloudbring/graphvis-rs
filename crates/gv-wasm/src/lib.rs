//! WebAssembly bindings (`wasm-bindgen`) for running `graphvis-rs` directly in the browser.

use wasm_bindgen::prelude::*;

/// Renders a DOT string using the specified layout engine (`dot`, `neato`, `fdp`, `sfdp`, `twopi`, `circo`, `osage`, `patchwork`)
/// and output format (`svg`, `json`, `json0`, `dot`, `xdot`, `canon`, `plain`).
#[wasm_bindgen]
pub fn render(
    dot_source: &str,
    engine: &str,
    format: &str,
) -> std::result::Result<String, JsValue> {
    gv_engine::render_dot(dot_source, engine, format).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Returns the version of `graphvis-rs` (compatible with Graphviz 16.1.0).
#[wasm_bindgen]
pub fn version() -> String {
    "graphvis-rs 0.1.0 (Graphviz 16.1.0 compatible)".to_string()
}
