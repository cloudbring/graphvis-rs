//! Graphviz `json` and `json0` output renderer ported from `plugin/core/gvrender_core_json.c`.

use crate::dot_out::build_positioned_graph;
use gv_core::{AttrMap, Graph, GraphKind, GraphvizError, Result};
use serde_json::{Map, Value, json};

/// Renders a [`Graph`] into Graphviz `json` (`json0 = false`, includes structured xdot `_draw_` arrays)
/// or `json0` (`json0 = true`, positioned attributes only) format.
pub fn render_json(graph: &Graph, json0: bool) -> Result<String> {
    let do_xdot = !json0;
    let positioned = build_positioned_graph(graph, do_xdot);
    let sg_cnt = positioned.subgraphs.len();

    let mut root = Map::new();
    root.insert("name".to_string(), Value::String(positioned.name.clone()));
    root.insert(
        "directed".to_string(),
        Value::Bool(positioned.kind == GraphKind::Directed),
    );
    root.insert("strict".to_string(), Value::Bool(positioned.strict));

    insert_attrs(&mut root, &positioned.graph_attrs, do_xdot);
    root.insert("_subgraph_cnt".to_string(), json!(sg_cnt));

    // Build `objects` array: subgraphs first (`_gvid` = `0..sg_cnt`), then nodes (`_gvid` = `sg_cnt..`)
    let mut objects = Vec::with_capacity(sg_cnt + positioned.nodes.len());

    for (sg_idx, subg) in positioned.subgraphs.iter().enumerate() {
        let mut sg_obj = Map::new();
        sg_obj.insert("name".to_string(), Value::String(subg.name.clone()));
        sg_obj.insert("_gvid".to_string(), json!(sg_idx));
        insert_attrs(&mut sg_obj, &subg.graph_attrs, do_xdot);

        if !subg.subgraphs.is_empty() {
            let child_ids: Vec<usize> = subg.subgraphs.iter().map(|id| id.0 as usize).collect();
            sg_obj.insert("subgraphs".to_string(), json!(child_ids));
        }
        if !subg.nodes.is_empty() {
            let node_gvids: Vec<usize> = subg
                .nodes
                .iter()
                .map(|nid| sg_cnt + (nid.0 as usize))
                .collect();
            sg_obj.insert("nodes".to_string(), json!(node_gvids));
        }
        if !subg.edges.is_empty() {
            let edge_gvids: Vec<usize> = subg.edges.iter().map(|eid| eid.0 as usize).collect();
            sg_obj.insert("edges".to_string(), json!(edge_gvids));
        }

        objects.push(Value::Object(sg_obj));
    }

    for (node_idx, node) in positioned.nodes.iter().enumerate() {
        let mut n_obj = Map::new();
        n_obj.insert("_gvid".to_string(), json!(sg_cnt + node_idx));
        n_obj.insert("name".to_string(), Value::String(node.name.clone()));
        insert_attrs(&mut n_obj, &node.attrs, do_xdot);
        objects.push(Value::Object(n_obj));
    }

    root.insert("objects".to_string(), Value::Array(objects));

    // Build `edges` array
    let mut edges = Vec::with_capacity(positioned.edges.len());
    for (edge_idx, edge) in positioned.edges.iter().enumerate() {
        let mut e_obj = Map::new();
        e_obj.insert("_gvid".to_string(), json!(edge_idx));
        e_obj.insert("tail".to_string(), json!(sg_cnt + (edge.tail.0 as usize)));
        e_obj.insert("head".to_string(), json!(sg_cnt + (edge.head.0 as usize)));
        if let Some(tp) = &edge.tail_port.name
            && !tp.is_empty()
        {
            e_obj.insert("tailport".to_string(), Value::String(tp.clone()));
        }
        if let Some(hp) = &edge.head_port.name
            && !hp.is_empty()
        {
            e_obj.insert("headport".to_string(), Value::String(hp.clone()));
        }
        insert_attrs(&mut e_obj, &edge.attrs, do_xdot);
        edges.push(Value::Object(e_obj));
    }
    root.insert("edges".to_string(), Value::Array(edges));

    let mut rendered = serde_json::to_string_pretty(&Value::Object(root)).map_err(|e| {
        GraphvizError::LayoutError {
            stage: "json_serialize",
            message: e.to_string(),
        }
    })?;
    rendered.push('\n');
    Ok(rendered)
}

fn is_xdot_attr(name: &str) -> bool {
    matches!(
        name,
        "_draw_" | "_ldraw_" | "_hdraw_" | "_tdraw_" | "_hldraw_" | "_tldraw_"
    )
}

fn insert_attrs(map: &mut Map<String, Value>, attrs: &AttrMap, do_xdot: bool) {
    for (k, v) in attrs {
        if v.is_empty() && k != "label" {
            continue;
        }
        if do_xdot && is_xdot_attr(k) {
            map.insert(k.clone(), Value::Array(parse_xdot_ops(v)));
        } else {
            map.insert(k.clone(), Value::String(v.clone()));
        }
    }
}

/// Parses an xdot command string into structured JSON objects matching `write_xdot` in
/// `gvrender_core_json.c`.
fn parse_xdot_ops(xdot_str: &str) -> Vec<Value> {
    let mut ops = Vec::new();
    let bytes = xdot_str.as_bytes();
    let mut i = 0;

    let skip_ws = |idx: &mut usize| {
        while *idx < bytes.len() && bytes[*idx].is_ascii_whitespace() {
            *idx += 1;
        }
    };

    let read_token = |idx: &mut usize| -> Option<&str> {
        skip_ws(idx);
        if *idx >= bytes.len() {
            return None;
        }
        let start = *idx;
        while *idx < bytes.len() && !bytes[*idx].is_ascii_whitespace() {
            *idx += 1;
        }
        std::str::from_utf8(&bytes[start..*idx]).ok()
    };

    let read_f64 = |idx: &mut usize| -> Option<f64> { read_token(idx)?.parse::<f64>().ok() };

    let read_usize = |idx: &mut usize| -> Option<usize> { read_token(idx)?.parse::<usize>().ok() };

    let read_len_prefixed_str = |idx: &mut usize| -> Option<String> {
        let len = read_usize(idx)?;
        skip_ws(idx);
        if *idx < bytes.len() && bytes[*idx] == b'-' {
            *idx += 1;
        }
        let end = (*idx + len).min(bytes.len());
        let s = String::from_utf8_lossy(&bytes[*idx..end]).into_owned();
        *idx = end;
        Some(s)
    };

    while let Some(op_tok) = read_token(&mut i) {
        match op_tok {
            "E" | "e" => {
                let x = read_f64(&mut i).unwrap_or(0.0);
                let y = read_f64(&mut i).unwrap_or(0.0);
                let w = read_f64(&mut i).unwrap_or(0.0);
                let h = read_f64(&mut i).unwrap_or(0.0);
                ops.push(json!({
                    "op": op_tok,
                    "rect": [x, y, w, h]
                }));
            }
            "P" | "p" | "L" | "B" | "b" => {
                let n = read_usize(&mut i).unwrap_or(0);
                let mut pts = Vec::with_capacity(n);
                for _ in 0..n {
                    let px = read_f64(&mut i).unwrap_or(0.0);
                    let py = read_f64(&mut i).unwrap_or(0.0);
                    pts.push(json!([px, py]));
                }
                let json_op = match op_tok {
                    // In xdot, 'B' is unfilled bezier and 'b' is filled bezier, whereas in
                    // gvrender_core_json.c `xd_filled_bezier` is 'B' and `xd_unfilled_bezier` is 'b'
                    "B" => "b",
                    "b" => "B",
                    other => other,
                };
                ops.push(json!({
                    "op": json_op,
                    "points": pts
                }));
            }
            "T" => {
                let x = read_f64(&mut i).unwrap_or(0.0);
                let y = read_f64(&mut i).unwrap_or(0.0);
                let j = read_token(&mut i)
                    .and_then(|s| s.parse::<i32>().ok())
                    .unwrap_or(0);
                let w = read_f64(&mut i).unwrap_or(0.0);
                let txt = read_len_prefixed_str(&mut i).unwrap_or_default();
                let align = match j {
                    -1 => "l",
                    1 => "r",
                    _ => "c",
                };
                ops.push(json!({
                    "op": "T",
                    "pt": [x, y],
                    "align": align,
                    "width": w,
                    "text": txt
                }));
            }
            "F" => {
                let size = read_f64(&mut i).unwrap_or(14.0);
                let face = read_len_prefixed_str(&mut i).unwrap_or_default();
                ops.push(json!({
                    "op": "F",
                    "size": size,
                    "face": face
                }));
            }
            "c" | "C" => {
                let color = read_len_prefixed_str(&mut i).unwrap_or_default();
                ops.push(json!({
                    "op": op_tok,
                    "grad": "none",
                    "color": color
                }));
            }
            "S" => {
                let style = read_len_prefixed_str(&mut i).unwrap_or_default();
                ops.push(json!({
                    "op": "S",
                    "style": style
                }));
            }
            "t" => {
                let fontchar = read_usize(&mut i).unwrap_or(0);
                ops.push(json!({
                    "op": "t",
                    "fontchar": fontchar
                }));
            }
            _ => break,
        }
    }

    ops
}
