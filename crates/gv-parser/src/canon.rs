//! Canonical DOT writer (`dot -Tcanon`) matching Graphviz `lib/cgraph/write.c`.

use gv_core::{AttrMap, EdgeData, EdgeId, Graph, GraphKind, NodeData, NodeId, Port, SubgraphId};
use std::collections::BTreeSet;

/// Formats a [`Graph`] into canonical DOT representation (`dot -Tcanon`).
pub fn write_canon(graph: &Graph) -> String {
    let html = &graph.html_strings;
    let mut out = String::new();
    let mut state = WriteState {
        written_nodes: BTreeSet::new(),
        written_edges: BTreeSet::new(),
        attrs_written_nodes: BTreeSet::new(),
        subgraph_edges: subgraph_edge_membership(graph),
    };

    if graph.strict {
        out.push_str("strict ");
    }
    match graph.kind {
        GraphKind::Directed => out.push_str("digraph "),
        GraphKind::Undirected => out.push_str("graph "),
    }
    if !graph.name.is_empty() {
        out.push_str(&canon_str(&graph.name));
        out.push(' ');
    }
    out.push_str("{\n");

    let all_nodes: Vec<NodeId> = graph.nodes.iter().map(|n| n.id).collect();
    let all_edges: Vec<EdgeId> = graph.edges.iter().map(|e| e.id).collect();

    let mut root_node_defaults = effective_node_defaults(graph, &all_nodes, &graph.node_defaults);
    // dot declares `label="\N"` as the node default when the input does not.
    root_node_defaults
        .entry("label".to_string())
        .or_insert_with(|| "\\N".to_string());
    let root_edge_defaults = effective_edge_defaults(graph, &all_edges, &graph.edge_defaults);

    write_dict(&mut out, 1, "graph", &graph.graph_attrs, None, html);
    write_dict(&mut out, 1, "node", &root_node_defaults, None, html);
    write_dict(&mut out, 1, "edge", &root_edge_defaults, None, html);

    // Determine top-level subgraphs (those not nested inside another subgraph)
    let mut child_subgraphs = BTreeSet::new();
    for subg in &graph.subgraphs {
        for &child_id in &subg.subgraphs {
            child_subgraphs.insert(child_id);
        }
    }

    for subg in &graph.subgraphs {
        if !child_subgraphs.contains(&subg.id) {
            write_subgraph(
                graph,
                subg.id,
                &graph.graph_attrs,
                &root_node_defaults,
                &root_edge_defaults,
                1,
                &mut state,
                &mut out,
            );
        }
    }

    write_nodes_and_edges(
        graph,
        &all_nodes,
        &all_edges,
        &root_node_defaults,
        &root_edge_defaults,
        1,
        &mut state,
        &mut out,
    );

    out.push_str("}\n");
    out
}

struct WriteState {
    written_nodes: BTreeSet<NodeId>,
    written_edges: BTreeSet<EdgeId>,
    attrs_written_nodes: BTreeSet<NodeId>,
    subgraph_edges: Vec<Vec<EdgeId>>,
}

/// Edge lists per subgraph. After ranking, dot adds every edge whose endpoints both lie in a
/// cluster to that cluster and its ancestors (`mark_lowclusters`), so `-Tcanon` prints them there.
fn subgraph_edge_membership(graph: &Graph) -> Vec<Vec<EdgeId>> {
    let mut parent: Vec<Option<usize>> = vec![None; graph.subgraphs.len()];
    for (i, sg) in graph.subgraphs.iter().enumerate() {
        for child in &sg.subgraphs {
            parent[child.0 as usize] = Some(i);
        }
    }
    let mut sets: Vec<BTreeSet<EdgeId>> = graph
        .subgraphs
        .iter()
        .map(|sg| sg.edges.iter().copied().collect())
        .collect();
    for (i, sg) in graph.subgraphs.iter().enumerate() {
        if !sg.is_cluster {
            continue;
        }
        let members: BTreeSet<NodeId> = sg.nodes.iter().copied().collect();
        for e in &graph.edges {
            if members.contains(&e.tail) && members.contains(&e.head) {
                let mut cur = Some(i);
                while let Some(c) = cur {
                    sets[c].insert(e.id);
                    cur = parent[c];
                }
            }
        }
    }
    sets.into_iter().map(|s| s.into_iter().collect()).collect()
}

fn effective_node_defaults(graph: &Graph, nodes: &[NodeId], defaults: &AttrMap) -> AttrMap {
    defaults
        .iter()
        .filter(|(k, _)| {
            nodes
                .iter()
                .all(|nid| graph.nodes[nid.0 as usize].attrs.contains_key(*k))
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

fn effective_edge_defaults(graph: &Graph, edges: &[EdgeId], defaults: &AttrMap) -> AttrMap {
    defaults
        .iter()
        .filter(|(k, _)| {
            edges
                .iter()
                .all(|eid| graph.edges[eid.0 as usize].attrs.contains_key(*k))
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn write_subgraph(
    graph: &Graph,
    subg_id: SubgraphId,
    parent_graph_attrs: &AttrMap,
    parent_node_defaults: &AttrMap,
    parent_edge_defaults: &AttrMap,
    level: usize,
    state: &mut WriteState,
    out: &mut String,
) {
    let subg = &graph.subgraphs[subg_id.0 as usize];
    let subg_node_defaults = effective_node_defaults(graph, &subg.nodes, &subg.node_defaults);
    let subg_edge_defaults = effective_edge_defaults(graph, &subg.edges, &subg.edge_defaults);

    if is_irrelevant_subgraph(
        subg.name.as_str(),
        &subg.graph_attrs,
        &subg_node_defaults,
        &subg_edge_defaults,
        parent_graph_attrs,
        parent_node_defaults,
        parent_edge_defaults,
    ) {
        for &child_id in &subg.subgraphs {
            write_subgraph(
                graph,
                child_id,
                parent_graph_attrs,
                parent_node_defaults,
                parent_edge_defaults,
                level,
                state,
                out,
            );
        }
        return;
    }

    push_indent(out, level);
    if !subg.name.is_empty() {
        out.push_str("subgraph ");
        out.push_str(&canon_str(&subg.name));
        out.push_str(" {\n");
    } else {
        out.push_str("{\n");
    }

    let inner_level = level + 1;
    write_dict(
        out,
        inner_level,
        "graph",
        &subg.graph_attrs,
        Some(parent_graph_attrs),
        &graph.html_strings,
    );
    write_dict(
        out,
        inner_level,
        "node",
        &subg_node_defaults,
        Some(parent_node_defaults),
        &graph.html_strings,
    );
    write_dict(
        out,
        inner_level,
        "edge",
        &subg_edge_defaults,
        Some(parent_edge_defaults),
        &graph.html_strings,
    );

    for &child_id in &subg.subgraphs {
        write_subgraph(
            graph,
            child_id,
            &subg.graph_attrs,
            &subg_node_defaults,
            &subg_edge_defaults,
            inner_level,
            state,
            out,
        );
    }

    let scope_edges = state.subgraph_edges[subg_id.0 as usize].clone();
    write_nodes_and_edges(
        graph,
        &subg.nodes,
        &scope_edges,
        &subg_node_defaults,
        &subg_edge_defaults,
        inner_level,
        state,
        out,
    );

    push_indent(out, level);
    out.push_str("}\n");
}

#[allow(clippy::too_many_arguments)]
fn write_nodes_and_edges(
    graph: &Graph,
    scope_nodes: &[NodeId],
    scope_edges: &[EdgeId],
    node_defaults: &AttrMap,
    edge_defaults: &AttrMap,
    level: usize,
    state: &mut WriteState,
    out: &mut String,
) {
    let scope_edge_set: BTreeSet<EdgeId> = scope_edges.iter().copied().collect();

    for &node_id in scope_nodes {
        let node = &graph.nodes[node_id.0 as usize];
        let has_edges_in_scope = graph
            .edges
            .iter()
            .any(|e| scope_edge_set.contains(&e.id) && (e.tail == node_id || e.head == node_id));
        let non_default = collect_non_default_node_attrs(node, node_defaults);

        if !state.written_nodes.contains(&node_id)
            && (!has_edges_in_scope || !non_default.is_empty())
        {
            write_node_stmt(out, level, node, &non_default, &graph.html_strings, state);
        }

        // Out-edges iterate by head node sequence, then edge sequence (`agedgeseqcmpf`).
        let mut out_edges: Vec<&EdgeData> = graph
            .edges
            .iter()
            .filter(|e| e.tail == node_id && scope_edge_set.contains(&e.id))
            .collect();
        out_edges.sort_by_key(|e| (e.head, e.id));
        for edge in out_edges {
            if !state.written_edges.contains(&edge.id) {
                let head_node = &graph.nodes[edge.head.0 as usize];
                let head_non_default = collect_non_default_node_attrs(head_node, node_defaults);
                if edge.head != node_id
                    && !state.written_nodes.contains(&edge.head)
                    && !head_non_default.is_empty()
                {
                    write_node_stmt(
                        out,
                        level,
                        head_node,
                        &head_non_default,
                        &graph.html_strings,
                        state,
                    );
                }
                write_edge_stmt(out, level, graph, edge, edge_defaults, state);
            }
        }
    }
}

fn write_node_stmt(
    out: &mut String,
    level: usize,
    node: &NodeData,
    non_default_attrs: &[(&String, &String)],
    html: &BTreeSet<String>,
    state: &mut WriteState,
) {
    push_indent(out, level);
    out.push_str(&canon_str(&node.name));
    if !state.attrs_written_nodes.contains(&node.id) && !non_default_attrs.is_empty() {
        write_inline_attrs(out, level, non_default_attrs, html);
        state.attrs_written_nodes.insert(node.id);
    }
    out.push_str(";\n");
    state.written_nodes.insert(node.id);
}

fn write_edge_stmt(
    out: &mut String,
    level: usize,
    graph: &Graph,
    edge: &EdgeData,
    edge_defaults: &AttrMap,
    state: &mut WriteState,
) {
    let tail_node = &graph.nodes[edge.tail.0 as usize];
    let head_node = &graph.nodes[edge.head.0 as usize];

    push_indent(out, level);
    out.push_str(&canon_str(&tail_node.name));
    write_port(out, &edge.tail_port);

    match graph.kind {
        GraphKind::Directed => out.push_str(" -> "),
        GraphKind::Undirected => out.push_str(" -- "),
    }

    out.push_str(&canon_str(&head_node.name));
    write_port(out, &edge.head_port);

    let non_default = collect_non_default_edge_attrs(edge, edge_defaults);
    if !non_default.is_empty() {
        write_inline_attrs(out, level, &non_default, &graph.html_strings);
    }

    out.push_str(";\n");
    state.written_edges.insert(edge.id);
    state.written_nodes.insert(edge.tail);
    state.written_nodes.insert(edge.head);
}

fn write_port(out: &mut String, port: &Port) {
    if let Some(name) = &port.name {
        out.push(':');
        out.push_str(&canon_str(name));
    }
    if let Some(compass) = &port.compass {
        out.push(':');
        out.push_str(&canon_str(compass));
    }
}

fn write_dict(
    out: &mut String,
    level: usize,
    dict_name: &str,
    attrs: &AttrMap,
    parent_attrs: Option<&AttrMap>,
    html: &BTreeSet<String>,
) {
    let entries: Vec<(&String, &String)> = attrs
        .iter()
        .filter(|(k, v)| match parent_attrs {
            Some(parent) => parent.get(*k) != Some(*v),
            None => !v.is_empty(),
        })
        .collect();

    if entries.is_empty() {
        return;
    }

    push_indent(out, level);
    out.push_str(dict_name);
    out.push_str(" [");
    for (idx, (k, v)) in entries.iter().enumerate() {
        if idx > 0 {
            out.push_str(",\n");
            push_indent(out, level + 1);
        }
        out.push_str(&canon_str(k));
        out.push('=');
        out.push_str(&canon_value(v, html));
    }
    if entries.len() > 1 {
        out.push('\n');
        push_indent(out, level);
    }
    out.push_str("];\n");
}

fn write_inline_attrs(
    out: &mut String,
    level: usize,
    attrs: &[(&String, &String)],
    html: &BTreeSet<String>,
) {
    out.push_str("\t[");
    for (idx, (k, v)) in attrs.iter().enumerate() {
        if idx > 0 {
            out.push_str(",\n");
            push_indent(out, level + 1);
        }
        out.push_str(&canon_str(k));
        out.push('=');
        out.push_str(&canon_value(v, html));
    }
    out.push(']');
}

fn collect_non_default_node_attrs<'a>(
    node: &'a NodeData,
    node_defaults: &AttrMap,
) -> Vec<(&'a String, &'a String)> {
    node.attrs
        .iter()
        .filter(|(k, v)| match node_defaults.get(*k) {
            Some(def_val) => *v != def_val,
            None => !(*k == "label" && *v == "\\N"),
        })
        .collect()
}

fn collect_non_default_edge_attrs<'a>(
    edge: &'a EdgeData,
    edge_defaults: &AttrMap,
) -> Vec<(&'a String, &'a String)> {
    edge.attrs
        .iter()
        .filter(|(k, v)| {
            if (*k == "tailport" && edge.tail_port != Port::default())
                || (*k == "headport" && edge.head_port != Port::default())
            {
                return false;
            }
            edge_defaults.get(*k) != Some(*v)
        })
        .collect()
}

fn is_irrelevant_subgraph(
    name: &str,
    graph_attrs: &AttrMap,
    node_defaults: &AttrMap,
    edge_defaults: &AttrMap,
    parent_graph_attrs: &AttrMap,
    parent_node_defaults: &AttrMap,
    parent_edge_defaults: &AttrMap,
) -> bool {
    name.is_empty()
        && graph_attrs
            .iter()
            .all(|(k, v)| parent_graph_attrs.get(k) == Some(v))
        && node_defaults
            .iter()
            .all(|(k, v)| parent_node_defaults.get(k) == Some(v))
        && edge_defaults
            .iter()
            .all(|(k, v)| parent_edge_defaults.get(k) == Some(v))
}

fn push_indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push('\t');
    }
}

/// Returns `true` if `s` is an HTML-like label string (`<...>`).
pub fn is_html_string(s: &str) -> bool {
    if !(s.starts_with('<') && s.ends_with('>') && s.len() >= 2) {
        return false;
    }
    // Check that `<` and `>` are balanced and that the outer `<` closes only at the last byte
    let mut depth = 0usize;
    for (idx, ch) in s.char_indices() {
        if ch == '<' {
            depth += 1;
        } else if ch == '>' {
            if depth == 0 {
                return false;
            }
            depth -= 1;
            if depth == 0 && idx + ch.len_utf8() < s.len() {
                return false;
            }
        }
    }
    depth == 0
}

/// Canonicalizes an attribute value: a string lexed as an HTML string (`<...>`, recorded in
/// `html`) is emitted verbatim, anything else (including a quoted `"<x>"`) is quoted as needed.
pub fn canon_value(s: &str, html: &BTreeSet<String>) -> String {
    if html.contains(s) {
        s.to_string()
    } else {
        canon_str(s)
    }
}

/// Canonicalizes a single identifier or string value following `_agstrcanon` in `write.c`.
pub fn canon_str(s: &str) -> String {
    if s.is_empty() {
        return "\"\"".to_string();
    }
    if can_be_unquoted(s) {
        return s.to_string();
    }

    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() && is_graphviz_escape_char(bytes[i + 1]) {
            out.push('\\');
            out.push(bytes[i + 1] as char);
            i += 2;
        } else if bytes[i] == b'"' {
            out.push_str("\\\"");
            i += 1;
        } else {
            let ch = s[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out.push('"');
    out
}

fn is_graphviz_escape_char(b: u8) -> bool {
    matches!(
        b,
        b'E' | b'G' | b'H' | b'L' | b'N' | b'T' | b'l' | b'n' | b'r' | b'\\' | b'"'
    )
}

fn can_be_unquoted(s: &str) -> bool {
    if matches!(
        s.to_ascii_lowercase().as_str(),
        "node" | "edge" | "strict" | "graph" | "digraph" | "subgraph"
    ) {
        return false;
    }

    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    // Valid unquoted identifier
    if first.is_ascii_alphabetic() || first == '_' || !first.is_ascii() {
        return chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || !c.is_ascii());
    }

    // Valid unquoted numeric literal
    is_valid_dot_numeral(s)
}

fn is_valid_dot_numeral(s: &str) -> bool {
    let s = s.strip_prefix('-').unwrap_or(s);
    if s.is_empty() || s == "." {
        return false;
    }
    let mut dot_count = 0usize;
    let mut digit_count = 0usize;
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            digit_count += 1;
        } else if ch == '.' {
            dot_count += 1;
            if dot_count > 1 {
                return false;
            }
        } else {
            return false;
        }
    }
    digit_count > 0
}
