//! Output renderers for `graphvis-rs` (`svg`, `json`, `json0`, `dot`, `xdot`, `canon`, `plain`).

#![forbid(unsafe_code)]

mod agwrite;
pub mod emit;
pub mod json;
mod svg_engine;
mod xdot_engine;

/// Renders `-Tdot` (`xdot == false`) or `-Txdot`.
pub fn render_dot_positioned(graph: &Graph, xdot: bool) -> String {
    render_graph(
        graph,
        if xdot {
            OutputFormat::Xdot
        } else {
            OutputFormat::Dot
        },
    )
    .unwrap_or_default()
}

/// Renders `-Tplain`.
pub fn render_plain(graph: &Graph) -> String {
    render_graph(graph, OutputFormat::Plain).unwrap_or_default()
}

/// Renders `-Tjson` (`json0 == false`) or `-Tjson0`.
pub fn render_json(graph: &Graph, json0: bool) -> Result<String> {
    render_graph(
        graph,
        if json0 {
            OutputFormat::Json0
        } else {
            OutputFormat::Json
        },
    )
}
pub use emit::reset_render_state;

/// Renders `graph` as SVG (`-Tsvg`).
pub fn render_svg(graph: &Graph) -> String {
    let m = emit::model::Model::build(graph);
    emit::run(&m, svg_engine::SvgEngine::default()).out
}

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
    render_graph_named(graph, format, "")
}

/// Edge iteration order of `agfstnode`/`agfstout` (numbering of `_gvid` in JSON).
fn edge_order(m: &emit::model::Model<'_>) -> Vec<usize> {
    m.out_edges.iter().flatten().copied().collect()
}

/// The graph with layout attributes and, for xdot, draw operations attached.
fn attached(
    m: &emit::model::Model<'_>,
    xdot: bool,
) -> (Graph, Vec<std::collections::BTreeSet<String>>) {
    let (mut g, _, _, mut forced) = emit::output::attach_attrs(m);
    if xdot {
        let eng = xdot_engine::XdotEngine::new(m.root_attrs().get("xdotversion"), None);
        let eng = emit::run(m, eng);
        for (kind, name, value) in eng.sets {
            let attrs = match kind {
                emit::job::ObjKind::Root => &mut g.graph_attrs,
                emit::job::ObjKind::Cluster(c) => &mut g.subgraphs[m.clusters[c].sub].graph_attrs,
                emit::job::ObjKind::Node(n) => &mut g.nodes[n].attrs,
                emit::job::ObjKind::Edge(e) => &mut g.edges[e].attrs,
            };
            if let emit::job::ObjKind::Cluster(c) = kind {
                forced[m.clusters[c].sub].insert(name.to_string());
            }
            attrs.insert(name.to_string(), value);
        }
    }
    (g, forced)
}

/// Private graph attribute the no-layout engine sets (`gv_layout::nop`).
const NOP_KEY: &str = "\u{1}nop";

/// Records the attributes the layout and renderers set in cgraph's dictionaries:
/// `safe_dcl` for node and edge attributes, and graph values as local symbols (`agxset`).
fn sync_dicts(g: &mut Graph, orig: &Graph) {
    use gv_core::AttrSym;
    let set_graph = |d: &mut std::collections::BTreeMap<String, AttrSym>,
                     now: &gv_core::AttrMap,
                     before: &gv_core::AttrMap| {
        for (k, v) in now {
            if before.get(k) != Some(v) {
                d.entry(k.clone()).or_default().defval = v.clone();
            }
        }
    };
    let (root_now, root_before) = (g.graph_attrs.clone(), &orig.graph_attrs);
    // safe_dcl declares graph attributes on the root before setting them.
    for k in root_now.keys() {
        if !root_before.contains_key(k) {
            g.dicts[0].graph.entry(k.clone()).or_default();
        }
    }
    set_graph(&mut g.dicts[0].graph, &root_now, root_before);
    for i in 0..g.subgraphs.len() {
        let now = g.subgraphs[i].graph_attrs.clone();
        for k in now.keys() {
            if !orig.subgraphs[i].graph_attrs.contains_key(k) {
                g.dicts[0].graph.entry(k.clone()).or_default();
            }
        }
        set_graph(
            &mut g.dicts[i + 1].graph,
            &now,
            &orig.subgraphs[i].graph_attrs,
        );
    }
    let mut node_keys = std::collections::BTreeSet::new();
    for n in &g.nodes {
        node_keys.extend(n.attrs.keys().cloned());
    }
    for k in node_keys {
        if !g.dicts.iter().any(|d| d.node.contains_key(&k)) {
            g.dicts[0].node.insert(k, AttrSym::default());
        }
    }
    let mut edge_keys = std::collections::BTreeSet::new();
    for e in &g.edges {
        edge_keys.extend(e.attrs.keys().cloned());
    }
    for k in edge_keys {
        if !g.dicts.iter().any(|d| d.edge.contains_key(&k)) {
            g.dicts[0].edge.insert(k, AttrSym::default());
        }
    }
}

/// dot's `mark_lowclusters`: an edge whose ends both lie in a cluster belongs to it and its
/// ancestors.
fn low_cluster_edges(graph: &Graph) -> Vec<std::collections::BTreeSet<usize>> {
    let n = graph.subgraphs.len();
    let mut parent: Vec<Option<usize>> = vec![None; n];
    for (i, sg) in graph.subgraphs.iter().enumerate() {
        for c in &sg.subgraphs {
            parent[c.0 as usize] = Some(i);
        }
    }
    let mut sets = vec![std::collections::BTreeSet::new(); n];
    for (i, sg) in graph.subgraphs.iter().enumerate() {
        if !sg.is_cluster {
            continue;
        }
        let members: std::collections::BTreeSet<u32> = sg.nodes.iter().map(|x| x.0).collect();
        for (ei, e) in graph.edges.iter().enumerate() {
            if members.contains(&e.tail.0) && members.contains(&e.head.0) {
                let mut cur = Some(i);
                while let Some(c) = cur {
                    sets[c].insert(ei);
                    cur = parent[c];
                }
            }
        }
    }
    sets
}

/// Renders `graph`; `name` is the requested format name (distinguishes `plain-ext`).
pub fn render_graph_named(graph: &Graph, format: OutputFormat, name: &str) -> Result<String> {
    if format == OutputFormat::Canon {
        if graph.dicts.len() == graph.subgraphs.len() + 1 {
            return Ok(agwrite::agwrite(graph, &[]));
        }
        return Ok(write_canon(graph));
    }
    let m = emit::model::Model::build(graph);
    Ok(match format {
        OutputFormat::Svg => emit::run(&m, svg_engine::SvgEngine::default()).out,
        OutputFormat::Json | OutputFormat::Json0 => {
            let (g, _) = attached(&m, format == OutputFormat::Json);
            json::write_json(&g, format == OutputFormat::Json, &edge_order(&m))
        }
        OutputFormat::Dot | OutputFormat::Xdot => {
            let (mut g, forced) = attached(&m, format == OutputFormat::Xdot);
            if g.dicts.len() == g.subgraphs.len() + 1 {
                sync_dicts(&mut g, graph);
                let extra = if graph.graph_attrs.contains_key(NOP_KEY) {
                    Vec::new()
                } else {
                    low_cluster_edges(graph)
                };
                agwrite::agwrite(&g, &extra)
            } else {
                emit::output::propagate_inherited(&mut g, graph, &forced);
                gv_parser::write_canon_forced(&g, &forced)
            }
        }
        OutputFormat::Plain => {
            let zoom = emit::job::Job::new(
                &m,
                emit::job::Features {
                    flags: 0,
                    default_pad: 0.0,
                    knowncolors: &[],
                    rgba: false,
                },
                0,
            )
            .zoom;
            emit::output::write_plain(&m, zoom, name.eq_ignore_ascii_case("plain-ext"))
        }
        OutputFormat::Canon => unreachable!(),
    })
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
