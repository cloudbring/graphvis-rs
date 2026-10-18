//! WebAssembly bindings (`wasm-bindgen`) for running `graphvis-rs` directly in the browser,
//! Node.js, Deno, Bun, and Cloudflare Workers.

use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct GraphStats {
    name: String,
    directed: bool,
    strict: bool,
    nodes: usize,
    edges: usize,
    subgraphs: usize,
    width_pt: f64,
    height_pt: f64,
}

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

/// Renders every graph in a DOT string (a file may hold several), one output string per graph,
/// like the `dot` command line does for a multi-graph input file.
#[wasm_bindgen(js_name = renderAll)]
pub fn render_all(
    dot_source: &str,
    engine: &str,
    format: &str,
) -> std::result::Result<Vec<String>, JsValue> {
    gv_engine::render_dot_all(
        dot_source,
        engine,
        format,
        &gv_engine::GlobalAttrs::default(),
    )
    .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// [`render_all`] for a file's raw bytes, as the `dot` command line reads them: bytes that are
/// not UTF-8 (a `charset=latin1` file, say) are kept rather than replaced, and the result is
/// the bytes the command line writes, every graph's output concatenated. A JS string cannot
/// carry such bytes, so `renderAll` cannot reproduce C on these inputs.
#[wasm_bindgen(js_name = renderAllBytes)]
pub fn render_all_bytes(
    dot_source: &[u8],
    engine: &str,
    format: &str,
) -> std::result::Result<Vec<u8>, JsValue> {
    let src = gv_core::charset::decode_slice(dot_source);
    let parts = gv_engine::render_dot_all(&src, engine, format, &gv_engine::GlobalAttrs::default())
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(gv_core::charset::encode(&parts.concat()).into_owned())
}

/// Convenience helper to render a DOT graph directly to an SVG string using the specified layout engine.
#[wasm_bindgen(js_name = renderSvg)]
pub fn render_svg(dot_source: &str, engine: &str) -> std::result::Result<String, JsValue> {
    gv_engine::render_dot(dot_source, engine, "svg").map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Convenience helper to render a DOT graph to Graphviz `json` format.
#[wasm_bindgen(js_name = renderJson)]
pub fn render_json(dot_source: &str, engine: &str) -> std::result::Result<String, JsValue> {
    gv_engine::render_dot(dot_source, engine, "json").map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Parses and lays out a DOT graph and returns JSON statistics (`nodes`, `edges`, `subgraphs`, `width_pt`, `height_pt`).
#[wasm_bindgen(js_name = inspectGraph)]
pub fn inspect_graph(dot_source: &str, engine: &str) -> std::result::Result<String, JsValue> {
    let layout_engine =
        gv_engine::LayoutEngine::parse(engine).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let mut graph =
        gv_engine::parse_dot(dot_source).map_err(|e| JsValue::from_str(&e.to_string()))?;
    gv_engine::layout_graph(&mut graph, layout_engine)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let stats = GraphStats {
        name: graph.name,
        directed: graph.kind == gv_core::GraphKind::Directed,
        strict: graph.strict,
        nodes: graph.nodes.len(),
        edges: graph.edges.len(),
        subgraphs: graph.subgraphs.len(),
        width_pt: graph.bb.width(),
        height_pt: graph.bb.height(),
    };
    serde_json::to_string(&stats).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Returns the list of supported layout engines as a JSON array.
#[wasm_bindgen(js_name = supportedEngines)]
pub fn supported_engines() -> String {
    r#"["dot","neato","fdp","sfdp","twopi","circo","osage","patchwork"]"#.to_string()
}

/// Returns the list of supported output formats as a JSON array.
#[wasm_bindgen(js_name = supportedFormats)]
pub fn supported_formats() -> String {
    r#"["svg","json","json0","dot","xdot","canon","plain"]"#.to_string()
}

/// Returns the version of `graphvis-rs` (compatible with Graphviz 16.1.0).
#[wasm_bindgen]
pub fn version() -> String {
    "graphvis-rs 0.1.0 (partial port of Graphviz 16.1.0)".to_string()
}
