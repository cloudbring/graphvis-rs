//! Graph, node and edge initialisation shared by the non-dot engines:
//! the parts of `graph_init` (`lib/common/input.c`), `common_init_node` and
//! `common_init_edge` (`lib/common/utils.c`) that layouts consult.

use super::shapes::NodeShape;
use super::types::*;
use super::utils::{c_round, late_double, late_nnstring, map_bool, mapbool, parse_f64_rest};
use crate::cutil::{CRand, Drand48};
use gv_core::font::{LabelSubstContext, measure_multiline_label_with_context};
use gv_core::{AttrMap, BoundingBox, Graph, GraphKind, Point};

pub const DEFAULT_FONTSIZE: f64 = 14.0;
pub const MIN_FONTSIZE: f64 = 1.0;
pub const DEFAULT_FONTNAME: &str = "Times-Roman";
pub const DEFAULT_NODESEP: f64 = 0.25;
pub const MIN_NODESEP: f64 = 0.02;

/// `POINTS(a)`: inches to integral points.
pub fn points(inches: f64) -> f64 {
    c_round(inches * 72.0) as f64
}

/// `sscanf(p, "%lf,%lf%c")`: number of conversions and the values.
pub fn scan_pair(p: &str) -> (usize, f64, f64, Option<char>) {
    let Some((x, rest)) = parse_f64_rest(p) else {
        return (0, 0.0, 0.0, None);
    };
    let Some(rest) = rest.strip_prefix(',') else {
        return (1, x, 0.0, rest.chars().next());
    };
    let Some((y, rest)) = parse_f64_rest(rest) else {
        return (1, x, 0.0, None);
    };
    let c = rest.chars().next();
    (if c.is_some() { 3 } else { 2 }, x, y, c)
}

/// `getdoubles2ptf`: a `size`/`page` attribute in points and its `!` flag.
fn getdoubles2ptf(p: Option<&str>) -> (Point, bool) {
    let mut res = Point::default();
    let Some(p) = p else { return (res, false) };
    let (i, x, y, c) = scan_pair(p);
    if i > 1 && x > 0.0 && y > 0.0 {
        res = Point::new(points(x), points(y));
        return (res, c == Some('!'));
    }
    if let Some((x, rest)) = parse_f64_rest(p)
        && x > 0.0
    {
        res = Point::new(points(x), points(x));
        return (res, rest.starts_with('!'));
    }
    (res, false)
}

/// Builds a [`TextLabel`] for `text` measured with the port's font metrics.
pub fn make_label(
    text: &str,
    ctx: &LabelSubstContext<'_>,
    fontname: &str,
    fontsize: f64,
) -> TextLabel {
    let m = measure_multiline_label_with_context(text, ctx, fontname, fontsize);
    TextLabel {
        text: text.to_string(),
        dimen: Point::new(m.width, m.height),
        pos: Point::default(),
        set: false,
        fontsize,
        fontname: fontname.to_string(),
    }
}

fn label_font(attrs: &AttrMap) -> (String, f64) {
    (
        late_nnstring(attrs, "fontname", DEFAULT_FONTNAME).to_string(),
        late_double(attrs, "fontsize", DEFAULT_FONTSIZE, MIN_FONTSIZE),
    )
}

/// `do_graph_label` for a graph or cluster: the label and `GD_label_pos`.
pub fn graph_label(
    attrs: &AttrMap,
    name: &str,
    is_root: bool,
    directed: bool,
) -> (Option<TextLabel>, u8) {
    let Some(text) = attrs.get("label").filter(|s| !s.is_empty()) else {
        return (None, 0);
    };
    let (fontname, fontsize) = label_font(attrs);
    let ctx = LabelSubstContext {
        graph_name: name,
        label_text: text,
        is_directed: directed,
        ..Default::default()
    };
    let lbl = make_label(text, &ctx, &fontname, fontsize);
    let pos = attrs.get("labelloc").map(String::as_str);
    let mut flag = if is_root {
        if pos.is_some_and(|p| p.starts_with('t')) {
            LABEL_AT_TOP
        } else {
            LABEL_AT_BOTTOM
        }
    } else if pos.is_some_and(|p| p.starts_with('b')) {
        LABEL_AT_BOTTOM
    } else {
        LABEL_AT_TOP
    };
    if let Some(j) = attrs.get("labeljust") {
        if j.starts_with('l') {
            flag |= LABEL_AT_LEFT;
        } else if j.starts_with('r') {
            flag |= LABEL_AT_RIGHT;
        }
    }
    (Some(lbl), flag)
}

/// `PAD(d)`: label padding of `4*GAP` by `2*GAP`.
pub fn pad(d: Point) -> Point {
    Point::new(d.x + 16.0, d.y + 8.0)
}

impl<'a> LGraph<'a> {
    /// `graph_init` + `common_init_node` + `common_init_edge` for `graph`,
    /// with `ndim` layout dimensions and `default_edge_type` for `setEdgeType`.
    pub fn new(graph: &'a Graph, ndim: usize, default_edge_type: EdgeType) -> Self {
        let directed = graph.kind == GraphKind::Directed;
        let gattrs = &graph.graph_attrs;
        let nodesep = c_round(late_double(gattrs, "nodesep", DEFAULT_NODESEP, MIN_NODESEP) * 72.0);
        let mut drawing = Drawing::default();
        if let Some(p) = gattrs.get("ratio") {
            match p.as_str() {
                "auto" => drawing.ratio_kind = RatioKind::Auto,
                "compress" => drawing.ratio_kind = RatioKind::Compress,
                "expand" => drawing.ratio_kind = RatioKind::Expand,
                "fill" => drawing.ratio_kind = RatioKind::Fill,
                _ => {
                    let r = super::utils::atof(p);
                    if r > 0.0 {
                        drawing.ratio_kind = RatioKind::Value;
                        drawing.ratio = r;
                    }
                }
            }
        }
        let (size, filled) = getdoubles2ptf(gattrs.get("size").map(String::as_str));
        drawing.size = size;
        drawing.filled = filled;
        drawing.landscape = if let Some(p) = gattrs.get("rotate") {
            super::utils::atoi(p) == 90
        } else if let Some(p) = gattrs.get("orientation") {
            p.starts_with('l') || p.starts_with('L')
        } else {
            mapbool(gattrs.get("landscape").map(String::as_str))
        };
        let concentrate = mapbool(gattrs.get("concentrate").map(String::as_str));
        let (label, label_pos) = graph_label(gattrs, &graph.name, true, directed);

        let edge_type = match gattrs.get("splines") {
            None => default_edge_type,
            Some(s) if s.is_empty() => EdgeType::None,
            Some(s) => edge_type_of(s, default_edge_type),
        };

        let mut nodes = Vec::with_capacity(graph.nodes.len());
        for nd in &graph.nodes {
            let mut n = nd.clone();
            gv_geom::compute_node_dimensions(&mut n, &graph.name);
            let shape = NodeShape::new(&n);
            let w = n.width * 72.0;
            let xlabel = nd.attrs.get("xlabel").filter(|s| !s.is_empty()).map(|t| {
                let (fname, fsize) = label_font(&nd.attrs);
                let ctx = LabelSubstContext {
                    node_name: &nd.name,
                    graph_name: &graph.name,
                    is_directed: directed,
                    label_text: t,
                    ..Default::default()
                };
                make_label(t, &ctx, &fname, fsize)
            });
            nodes.push(LNode {
                pos: vec![0.0; ndim],
                coord: Point::default(),
                width: n.width,
                height: n.height,
                lw: w / 2.0,
                rw: w / 2.0,
                ht: n.height * 72.0,
                pinned: 0,
                id: 0,
                heapindex: -1,
                hops: 0,
                dist: 0.0,
                mark: false,
                shape,
                xlabel,
                clust: None,
                lim: -1,
            });
        }

        let mut out = vec![Vec::new(); graph.nodes.len()];
        let mut inn = vec![Vec::new(); graph.nodes.len()];
        let mut edges = Vec::with_capacity(graph.edges.len());
        for (ei, ed) in graph.edges.iter().enumerate() {
            let t = ed.tail.0 as usize;
            let h = ed.head.0 as usize;
            out[t].push(ei);
            inn[h].push(ei);
            let a = &ed.attrs;
            let (fname, fsize) = label_font(a);
            let lfname = late_nnstring(a, "labelfontname", &fname).to_string();
            let lfsize = late_double(a, "labelfontsize", fsize, MIN_FONTSIZE);
            let tname = &graph.nodes[t].name;
            let hname = &graph.nodes[h].name;
            let mk = |key: &str, fname: &str, fsize: f64| {
                a.get(key).filter(|s| !s.is_empty()).map(|text| {
                    let ctx = LabelSubstContext {
                        graph_name: &graph.name,
                        tail_name: tname,
                        head_name: hname,
                        is_edge: true,
                        is_directed: directed,
                        tail_port: ed.tail_port.name.as_deref(),
                        head_port: ed.head_port.name.as_deref(),
                        label_text: text,
                        ..Default::default()
                    };
                    make_label(text, &ctx, fname, fsize)
                })
            };
            let clip = |key: &str| map_bool(a.get(key).map(String::as_str), true);
            edges.push(LEdge {
                tail: t,
                head: h,
                dist: 1.0,
                factor: late_double(a, "weight", 1.0, 1.0),
                count: 0,
                to_virt: None,
                spl: None,
                label: mk("label", &fname, fsize),
                xlabel: mk("xlabel", &fname, fsize),
                head_label: mk("headlabel", &lfname, lfsize),
                tail_label: mk("taillabel", &lfname, lfsize),
                tail_port: PortInfo {
                    clip: clip("tailclip"),
                    ..PortInfo::default()
                },
                head_port: PortInfo {
                    clip: clip("headclip"),
                    ..PortInfo::default()
                },
                path: Vec::new(),
            });
        }
        let has_labelangle = graph.edge_defaults.contains_key("labelangle")
            || graph
                .edges
                .iter()
                .any(|e| e.attrs.contains_key("labelangle"));
        let has_labeldistance = graph.edge_defaults.contains_key("labeldistance")
            || graph
                .edges
                .iter()
                .any(|e| e.attrs.contains_key("labeldistance"));

        let mut lg = LGraph {
            src: graph,
            nodes,
            edges,
            out,
            inn,
            ndim,
            nodesep,
            bb: BoundingBox::default(),
            label,
            label_pos,
            drawing,
            clusters: Vec::new(),
            root_clusters: Vec::new(),
            edge_type,
            concentrate,
            drand: Drand48::default(),
            crand: CRand::default(),
            has_labelangle,
            has_labeldistance,
        };
        super::ports::resolve_all_ports(&mut lg);
        lg
    }

    /// Writes positions, splines, label positions and bounding boxes back
    /// into `graph` (points, with the final translation already applied).
    pub fn write_back(&self, graph: &mut Graph) {
        for (i, n) in self.nodes.iter().enumerate() {
            let nd = &mut graph.nodes[i];
            nd.pos = n.coord;
            nd.width = n.width;
            nd.height = n.height;
        }
        for (i, e) in self.edges.iter().enumerate() {
            let ed = &mut graph.edges[i];
            ed.spline = gv_core::Spline {
                segments: e
                    .spl
                    .as_ref()
                    .map(|v| {
                        v.iter()
                            .map(|b| gv_core::BezierSegment {
                                points: b.list.clone(),
                                start_arrow: (b.sflag != 0).then_some(b.sp),
                                end_arrow: (b.eflag != 0).then_some(b.ep),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            };
            ed.label_pos = e.label.as_ref().filter(|l| l.set).map(|l| l.pos);
            for (key, lbl) in [
                ("head_lp", &e.head_label),
                ("tail_lp", &e.tail_label),
                ("xlp", &e.xlabel),
            ] {
                if let Some(l) = lbl.as_ref().filter(|l| l.set) {
                    ed.attrs.insert(
                        key.to_string(),
                        format!("{},{}", fmt_g5(l.pos.x), fmt_g5(l.pos.y)),
                    );
                }
            }
        }
        for (i, n) in self.nodes.iter().enumerate() {
            if let Some(l) = n.xlabel.as_ref().filter(|l| l.set) {
                graph.nodes[i].attrs.insert(
                    "xlp".to_string(),
                    format!("{},{}", fmt_g5(l.pos.x), fmt_g5(l.pos.y)),
                );
            }
        }
        graph.bb = self.bb;
        for c in &self.clusters {
            graph.subgraphs[c.subgraph].bb = c.bb;
        }
    }
}

/// `%.5g`-style formatting used for extra label attributes.
pub fn fmt_g5(v: f64) -> String {
    if !v.is_finite() || v.abs() < 1e-9 {
        return "0".to_string();
    }
    let log10 = v.abs().log10().floor() as i32;
    let decimals = (4 - log10).clamp(0, 6) as usize;
    let s = format!("{v:.decimals$}");
    if s.contains('.') {
        let t = s.trim_end_matches('0').trim_end_matches('.');
        if t == "-0" {
            "0".to_string()
        } else {
            t.to_string()
        }
    } else {
        s
    }
}

/// `edgeType` in `lib/common/utils.c`.
pub fn edge_type_of(s: &str, dflt: EdgeType) -> EdgeType {
    let b = s.as_bytes();
    if b.is_empty() {
        return dflt;
    }
    if b[0] == b'0' {
        return EdgeType::Line;
    }
    if (b'1'..=b'9').contains(&b[0]) {
        return EdgeType::Spline;
    }
    let l = s.to_ascii_lowercase();
    match l.as_str() {
        "curved" => EdgeType::Curved,
        "compound" => EdgeType::CompoundEdge,
        "false" | "line" | "no" => EdgeType::Line,
        "none" => EdgeType::None,
        "ortho" => EdgeType::Ortho,
        "polyline" => EdgeType::Pline,
        "spline" | "true" | "yes" => EdgeType::Spline,
        _ => dflt,
    }
}
