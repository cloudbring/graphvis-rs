//! No-layout mode: `neato -n[N]` and the `nop`, `nop1` and `nop2` engines.
//!
//! Port of the `Nop` path through `lib/neatogen/neatoinit.c` (`neato_layout`, `init_nop`,
//! `user_pos`, `user_spline`, `nop_init_graphs`, `nop_init_edges`, `chkBB`), the bounding box
//! computation of `lib/common/utils.c` (`compute_bb`, `addLabelBB`), the translation and aspect
//! handling of `lib/neatogen/neatosplines.c` (`neato_translate`, `_neato_set_aspect`) and the
//! cluster/root label placement of `lib/common/postproc.c` (`gv_postprocess` with no
//! translation, `place_graph_label`, `place_root_label`).
//!
//! Node positions come from the `pos` attribute, in points (`PSinputscale` is 72 under `-n`).
//! With `-n2` (and above) edge splines come from the edge `pos` attribute, and label positions
//! from `lp`, `xlp`, `head_lp` and `tail_lp`; cluster boxes come from `bb`. Edges without a
//! usable `pos` are routed as straight lines.
//!
//! Results are stored on the graph: `NodeData::pos`, `EdgeData::spline`, `EdgeData::label_pos`,
//! `SubgraphData::bb` and `Graph::bb`. Positions of node `xlp` and edge `xlp`, `head_lp`,
//! `tail_lp` labels are written back to those attributes in Rust's shortest round-trip
//! notation, so renderers read the exact values; graph and cluster label positions go to
//! [`LABEL_POS_KEY`].

use graphviz_pure_core::font::{DEFAULT_FONT_NAME, DEFAULT_FONT_SIZE, LabelSubstContext};
use graphviz_pure_core::strtod::{atof, sscanf, strtod_rest};
use graphviz_pure_core::{
    AttrMap, BezierSegment, BoundingBox, Graph, GraphKind, GraphvizError, Point, Result, Spline,
    SubgraphId,
};

const POINTS_PER_INCH: f64 = 72.0;

/// Graph attribute holding a graph or cluster label position computed by the layout, in
/// exact notation. Renderers prefer it to `lp`; output writers never print it.
pub const LABEL_POS_KEY: &str = "\u{1}lp";

/// For engines that return before `dotneato_postprocess` on a graph without nodes
/// (`twopi_layout`, `circo_layout`, `patchwork_layout`): the graph and cluster labels
/// keep their zeroed position, which C draws and prints at the origin.
pub(crate) fn labels_at_origin(graph: &mut graphviz_pure_core::Graph) {
    let origin = exact_point(graphviz_pure_core::Point::new(0.0, 0.0));
    graph
        .graph_attrs
        .insert(LABEL_POS_KEY.to_string(), origin.clone());
    for sg in &mut graph.subgraphs {
        sg.graph_attrs
            .insert(LABEL_POS_KEY.to_string(), origin.clone());
    }
}
/// `PAD` in `const.h`: horizontal and vertical label padding.
const LABEL_PAD_X: f64 = 16.0;
const LABEL_PAD_Y: f64 = 8.0;

/// `sscanf(s, "%lf,%lf%n", ...) == 2`: the point and the rest after it.
pub(crate) fn scan_point(s: &str) -> Option<(Point, &str)> {
    let r = sscanf(s, "%lf,%lf");
    (r.count == 2).then(|| (Point::new(r.values[0], r.values[1]), r.rest))
}

/// `sscanf(s, "%lf,%lf,%lf,%lf", ...) == 4`.
fn scan_box(s: &str) -> Option<BoundingBox> {
    let r = sscanf(s, "%lf,%lf,%lf,%lf");
    let [a, b, c, d, _] = r.values;
    (r.count == 4).then(|| BoundingBox::new(a, b, c, d))
}

/// `addZ`, which the Nop path runs after `neato_init_graph`: with `dim > 2` and a declared
/// `z`, `user_pos` also gives each node a third coordinate (from `pos`, `z` or `drand48`),
/// which only shows in the `z` attribute written here. `user_pos`'s messages were printed
/// above.
fn add_z(graph: &mut Graph) {
    use crate::common::utils::late_int;
    let z_declared = graph.node_defaults.contains_key("z")
        || graph.nodes.iter().any(|n| n.attrs.contains_key("z"));
    let outdim = late_int(&graph.graph_attrs, "dimen", 2, 2);
    let ndim = late_int(&graph.graph_attrs, "dim", outdim, 2)
        .min(crate::common::types::MAXDIM as i64) as usize;
    if ndim < 3 || !z_declared {
        return;
    }
    let ng = graph.nodes.len();
    let mut drand = crate::cutil::Drand48::default();
    for node in &mut graph.nodes {
        let mut pvec = vec![0.0; ndim];
        if let Some(p) = node.attrs.get("pos") {
            graphviz_pure_core::diag::quietly(|| {
                crate::neatogen::neatoinit::user_pos_values(
                    p,
                    node.attrs.get("z").map(String::as_str),
                    false,
                    &node.name,
                    POINTS_PER_INCH,
                    ng,
                    &mut drand,
                    &mut pvec,
                )
            });
        }
        let z = pvec.get(2).copied().unwrap_or(0.0);
        crate::neatogen::neatoinit::set_z(node, z);
    }
}

/// Formats a position for storage in an attribute without losing precision.
fn exact_point(p: Point) -> String {
    format!("{:?},{:?}", p.x, p.y)
}

fn mapbool(v: Option<&str>) -> bool {
    match v.map(str::trim) {
        Some(s) if s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("yes") => true,
        Some(s) => s.parse::<i64>().is_ok_and(|n| n != 0),
        None => false,
    }
}

fn is_a_cluster(name: &str, attrs: &AttrMap) -> bool {
    graphviz_pure_core::has_cluster_prefix(name)
        || mapbool(attrs.get("cluster").map(String::as_str))
}

/// `arrow_flags`: whether the tail and head of an edge carry arrows.
fn arrow_flags(graph: &Graph, attrs: &AttrMap) -> (bool, bool) {
    let directed = graph.kind == GraphKind::Directed;
    let (mut tail, mut head) = (false, directed);
    if let Some(dir) = attrs.get("dir").filter(|s| !s.is_empty()) {
        match dir.as_str() {
            "forward" => (tail, head) = (false, true),
            "back" => (tail, head) = (true, false),
            "both" => (tail, head) = (true, true),
            "none" => (tail, head) = (false, false),
            _ => {}
        }
    }
    let none = |k: &str| attrs.get(k).is_some_and(|s| s == "none");
    if head && none("arrowhead") {
        head = false;
    }
    if tail && none("arrowtail") {
        tail = false;
    }
    (tail, head)
}

/// `gv_isspace`: C's `isspace` in the C locale.
fn is_c_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r')
}

fn count_fields(b: &[u8]) -> usize {
    // numFields: whitespace-separated tokens up to the next ';'. A ';' after white space is
    // a token of its own, as C counts any byte there before it stops.
    let mut i = 0;
    let mut cnt = 0;
    loop {
        while i < b.len() && is_c_space(b[i]) {
            i += 1;
        }
        if i < b.len() {
            cnt += 1;
            while i < b.len() && !is_c_space(b[i]) && b[i] != b';' {
                i += 1;
            }
        }
        if i >= b.len() || !is_c_space(b[i]) {
            return cnt;
        }
    }
}

/// Why [`user_spline`] rejected an edge `pos`.
#[derive(Debug, PartialEq)]
enum SplineError {
    /// An empty `pos`: no warning.
    Empty,
    /// `doesn't have 3n+1 points`.
    Count,
    /// `syntax error in pos attribute`.
    Syntax,
}

/// `user_spline`: parses an edge `pos` attribute.
///
/// C walks `pos` a byte at a time and skips one byte after a segment, so the next segment can
/// start inside a multi-byte character. The port keeps a byte offset for the same reason:
/// text that does not start on a character boundary matches no `sscanf` pattern, as the
/// continuation bytes it starts with match none in C.
fn user_spline(
    pos: &str,
    tail_arrow: bool,
    head_arrow: bool,
) -> std::result::Result<Spline, SplineError> {
    if pos.is_empty() {
        return Err(SplineError::Empty);
    }
    let bytes = pos.as_bytes();
    let offset = |rest: &str| pos.len() - rest.len();
    let mut segments = Vec::new();
    let mut at = 0;
    // As in C, the arrow points persist across `;`-separated segments: a later segment
    // without its own `s,`/`e,` reuses the previous one.
    let mut sp = None;
    let mut ep = None;
    loop {
        if let Some(r) = pos.get(at..).and_then(|r| r.strip_prefix("s,"))
            && let Some((p, r)) = scan_point(r)
        {
            sp = Some(p);
            at = offset(r);
        }
        let mut e_at = at;
        while e_at < bytes.len() && is_c_space(bytes[e_at]) {
            e_at += 1;
        }
        if let Some(r) = pos.get(e_at..).and_then(|r| r.strip_prefix("e,"))
            && let Some((p, r)) = scan_point(r)
        {
            ep = Some(p);
            at = offset(r);
        }
        let npts = count_fields(&bytes[at..]);
        if npts < 4 || npts % 3 != 1 {
            return Err(SplineError::Count);
        }
        let mut points = Vec::with_capacity(npts);
        for _ in 0..npts {
            let (p, r) = pos
                .get(at..)
                .and_then(scan_point)
                .ok_or(SplineError::Syntax)?;
            points.push(p);
            at = offset(r);
        }
        while at < bytes.len() && is_c_space(bytes[at]) {
            at += 1;
        }
        let more = at < bytes.len();
        if more {
            at += 1;
        }
        segments.push(BezierSegment {
            points,
            start_arrow: sp.filter(|_| tail_arrow),
            end_arrow: ep.filter(|_| head_arrow),
        });
        if !more {
            break;
        }
    }
    Ok(Spline { segments })
}

/// Size of a label in points (`make_label`), made again after `layout_nop`'s first
/// `LGraph` reported its diagnostics.
fn label_dimen(
    text: &str,
    html: bool,
    fontname: &str,
    fontsize: f64,
    ctx: &LabelSubstContext<'_>,
) -> Point {
    graphviz_pure_core::diag::quietly(|| make_label_dimen(text, html, fontname, fontsize, ctx))
}

/// Size of a label in points (`make_label`).
fn make_label_dimen(
    text: &str,
    html: bool,
    fontname: &str,
    fontsize: f64,
    ctx: &LabelSubstContext<'_>,
) -> Point {
    if text.is_empty() {
        return Point::default();
    }
    graphviz_pure_geom::node_size::make_label(
        text,
        html,
        ctx,
        fontname,
        fontsize,
        graphviz_pure_core::charset::current(),
    )
    .dimen
}

fn attr_f64(attrs: &AttrMap, key: &str, default: f64, min: f64) -> f64 {
    match attrs.get(key).and_then(|s| strtod_rest(s)) {
        Some((v, _)) if v >= min => v,
        Some(_) => min,
        None => default,
    }
}

fn attr_str<'a>(attrs: &'a AttrMap, key: &str, default: &'a str) -> &'a str {
    attrs
        .get(key)
        .map(String::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or(default)
}

/// A label whose position may be known.
#[derive(Clone, Copy)]
struct Lbl {
    dimen: Point,
    pos: Point,
    set: bool,
}

struct Clust {
    id: SubgraphId,
    label: Option<Lbl>,
    /// `GD_label_pos` bits: 1 = top, 2 = left, 4 = right (as `LABEL_AT_*` in `const.h`).
    label_loc: u8,
    border: Point,
    children: Vec<usize>,
}

fn add_label_bb(bb: &mut BoundingBox, l: &Lbl) {
    let (w, h) = (l.dimen.x, l.dimen.y);
    let (min, max) = (l.pos.x - w / 2.0, l.pos.x + w / 2.0);
    if min < bb.ll.x {
        bb.ll.x = min;
    }
    if max > bb.ur.x {
        bb.ur.x = max;
    }
    let (min, max) = (l.pos.y - h / 2.0, l.pos.y + h / 2.0);
    if min < bb.ll.y {
        bb.ll.y = min;
    }
    if max > bb.ur.y {
        bb.ur.y = max;
    }
}

/// Label of a graph or cluster (`do_graph_label`), with its `GD_label_pos` and border.
fn graph_label(
    attrs: &AttrMap,
    name: &str,
    is_root: bool,
    html: &std::collections::BTreeSet<String>,
) -> Option<(Lbl, u8, Point)> {
    let text = attrs.get("label").filter(|s| !s.is_empty())?;
    let fontsize = attr_f64(attrs, "fontsize", DEFAULT_FONT_SIZE, 1.0);
    let fontname = attr_str(attrs, "fontname", DEFAULT_FONT_NAME);
    let ctx = LabelSubstContext {
        graph_name: name,
        ..Default::default()
    };
    // nop_init_graphs' do_graph_label makes the cluster labels, after the node and edge init;
    // the root label was made before the engine ran.
    let dimen = if is_root {
        label_dimen(text, html.contains(text), fontname, fontsize, &ctx)
    } else {
        make_label_dimen(text, html.contains(text), fontname, fontsize, &ctx)
    };
    let loc = attrs.get("labelloc").map(String::as_str).unwrap_or("");
    let mut pos_flag = 0u8;
    if is_root {
        if loc.starts_with('t') {
            pos_flag |= 1;
        }
    } else if !loc.starts_with('b') {
        pos_flag |= 1;
    }
    let just = attrs.get("labeljust").map(String::as_str).unwrap_or("");
    if just.starts_with('l') {
        pos_flag |= 2;
    } else if just.starts_with('r') {
        pos_flag |= 4;
    }
    let border = Point::new(dimen.x + LABEL_PAD_X, dimen.y + LABEL_PAD_Y);
    Some((
        Lbl {
            dimen,
            pos: Point::default(),
            set: false,
        },
        pos_flag,
        border,
    ))
}

struct EdgeLabels {
    label: Option<Lbl>,
    xlabel: Option<Lbl>,
    head: Option<Lbl>,
    tail: Option<Lbl>,
}

/// Runs the no-layout engine. `nop` is the `-n` level (1 or 2; higher acts like 2).
#[allow(clippy::needless_range_loop)]
pub fn layout_nop(graph: &mut Graph, nop: u8) -> Result<()> {
    let rankdir = graph
        .graph_attrs
        .get("rankdir")
        .cloned()
        .unwrap_or_else(|| "TB".to_string());
    let _ = rankdir;
    // neato_init_graph's neato_init_node_edge comes first: common_init_node for every node,
    // then common_init_edge (labels, then ports) per edge. Their diagnostics are reported
    // here, in that order. The sizings below repeat it quietly, apart from the cluster
    // labels, which nop_init_graphs makes next.
    let _ = crate::common::types::LGraph::new(graph, 2, crate::common::types::EdgeType::Line);
    graphviz_pure_core::diag::quietly(|| graphviz_pure_geom::node_size::size_graph_nodes(graph));

    // user_pos: positions in points, stored in inches as ND_pos.
    let mut npos: Vec<(f64, f64)> = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        let parsed = node.attrs.get("pos").and_then(|p| scan_point(p));
        match parsed {
            Some((p, _)) => npos.push((p.x / POINTS_PER_INCH, p.y / POINTS_PER_INCH)),
            None if node.name.starts_with("cluster") => npos.push((0.0, 0.0)),
            None => {
                // init_nop, then neato_layout continues its message (neatoinit.c).
                graphviz_pure_core::diag::agerrorf(&format!(
                    "node {} in graph {} has no position\n",
                    node.name, graph.name
                ));
                graphviz_pure_core::diag::agerr_prev("as required by the -n flag\n");
                return Err(GraphvizError::LayoutError {
                    stage: "nop",
                    message: format!("init_nop: node {} has no position", node.name),
                });
            }
        }
    }
    add_z(graph);

    // Node xlabels.
    let mut node_xl: Vec<Option<Lbl>> = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        let xl = node.attrs.get("xlabel").filter(|s| !s.is_empty()).map(|t| {
            let fs = attr_f64(&node.attrs, "fontsize", DEFAULT_FONT_SIZE, 1.0);
            let fname = attr_str(&node.attrs, "fontname", DEFAULT_FONT_NAME);
            let ctx = LabelSubstContext {
                graph_name: &graph.name,
                node_name: &node.name,
                is_node: true,
                ..Default::default()
            };
            let mut l = Lbl {
                dimen: label_dimen(t, graph.html_strings.contains(t), fname, fs, &ctx),
                pos: Point::default(),
                set: false,
            };
            if let Some((p, _)) = node.attrs.get("xlp").and_then(|s| scan_point(s)) {
                l.pos = p;
                l.set = true;
            }
            l
        });
        node_xl.push(xl);
    }

    // nop_init_graphs: root label and the cluster tree.
    let mut root_label = graph_label(&graph.graph_attrs, &graph.name, true, &graph.html_strings);
    if let Some((l, _, _)) = root_label.as_mut()
        && let Some((p, _)) = graph.graph_attrs.get("lp").and_then(|s| scan_point(s))
    {
        l.pos = p;
        l.set = true;
    }
    let mut clusters: Vec<Clust> = Vec::new();
    let mut root_clusters: Vec<usize> = Vec::new();
    let is_child: Vec<bool> = {
        let mut v = vec![false; graph.subgraphs.len()];
        for s in &graph.subgraphs {
            for c in &s.subgraphs {
                v[c.0 as usize] = true;
            }
        }
        v
    };
    let top: Vec<SubgraphId> = graph
        .subgraphs
        .iter()
        .filter(|s| !is_child[s.id.0 as usize])
        .map(|s| s.id)
        .collect();
    fn dfs(graph: &mut Graph, sid: SubgraphId, out: &mut Vec<usize>, clusters: &mut Vec<Clust>) {
        let sub = &graph.subgraphs[sid.0 as usize];
        let bb = sub.graph_attrs.get("bb").and_then(|s| scan_box(s));
        if is_a_cluster(&sub.name, &sub.graph_attrs)
            && let Some(mut bb) = bb
        {
            if bb.ll.y > bb.ur.y {
                std::mem::swap(&mut bb.ll.y, &mut bb.ur.y);
            }
            let mut label = graph_label(&sub.graph_attrs, &sub.name, false, &graph.html_strings);
            if let Some((l, _, _)) = label.as_mut()
                && let Some((p, _)) = sub.graph_attrs.get("lp").and_then(|s| scan_point(s))
            {
                l.pos = p;
                l.set = true;
            }
            let children_ids = sub.subgraphs.clone();
            graph.subgraphs[sid.0 as usize].bb = bb;
            let idx = clusters.len();
            clusters.push(Clust {
                id: sid,
                label: label.map(|x| x.0),
                label_loc: label.map_or(0, |x| x.1),
                border: label.map_or(Point::default(), |x| x.2),
                children: Vec::new(),
            });
            out.push(idx);
            let mut kids = Vec::new();
            for c in children_ids {
                dfs(graph, c, &mut kids, clusters);
            }
            clusters[idx].children = kids;
        } else {
            for c in sub.subgraphs.clone() {
                dfs(graph, c, out, clusters);
            }
        }
    }
    for sid in top {
        dfs(graph, sid, &mut root_clusters, &mut clusters);
    }

    // nop_init_edges: splines and edge labels.
    let mut edge_labels: Vec<EdgeLabels> = Vec::with_capacity(graph.edges.len());
    let mut with_spline = 0usize;
    // The first edge, in C's `agfstnode`/`agfstout` order, whose `pos` user_spline rejects.
    let mut rejected: Option<(u32, usize, SplineError)> = None;
    for ei in 0..graph.edges.len() {
        let (tail_arrow, head_arrow) = arrow_flags(graph, &graph.edges[ei].attrs);
        let e = &graph.edges[ei];
        let spline = if nop >= 2 {
            e.attrs
                .get("pos")
                .and_then(|p| match user_spline(p, tail_arrow, head_arrow) {
                    Ok(s) => Some(s),
                    Err(SplineError::Empty) => None,
                    Err(err) => {
                        if rejected.as_ref().is_none_or(|r| e.tail.0 < r.0) {
                            rejected = Some((e.tail.0, ei, err));
                        }
                        None
                    }
                })
        } else {
            None
        };
        let tname = graph.nodes[e.tail.0 as usize].name.clone();
        let hname = graph.nodes[e.head.0 as usize].name.clone();
        let ctx = LabelSubstContext {
            graph_name: &graph.name,
            tail_name: &tname,
            head_name: &hname,
            is_edge: true,
            is_directed: graph.kind == GraphKind::Directed,
            tail_port: e.tail_port.name.as_deref(),
            head_port: e.head_port.name.as_deref(),
            ..Default::default()
        };
        let fs = attr_f64(&e.attrs, "fontsize", DEFAULT_FONT_SIZE, 1.0);
        let fname = attr_str(&e.attrs, "fontname", DEFAULT_FONT_NAME).to_string();
        let lfs = attr_f64(&e.attrs, "labelfontsize", fs, 1.0);
        let lfname = attr_str(&e.attrs, "labelfontname", &fname).to_string();
        let has_spline = spline.is_some();
        let mk = |text_key: &str, pos_key: &str, fname: &str, fs: f64| {
            let t = e.attrs.get(text_key).filter(|s| !s.is_empty())?;
            let mut l = Lbl {
                dimen: label_dimen(t, graph.html_strings.contains(t), fname, fs, &ctx),
                pos: Point::default(),
                set: false,
            };
            if has_spline && let Some((p, _)) = e.attrs.get(pos_key).and_then(|s| scan_point(s)) {
                l.pos = p;
                l.set = true;
            }
            Some(l)
        };
        let labels = EdgeLabels {
            label: mk("label", "lp", &fname, fs),
            xlabel: mk("xlabel", "xlp", &fname, fs),
            head: mk("headlabel", "head_lp", &lfname, lfs),
            tail: mk("taillabel", "tail_lp", &lfname, lfs),
        };
        edge_labels.push(labels);
        if let Some(s) = spline {
            with_spline += 1;
            graph.edges[ei].spline = s;
        } else {
            graph.edges[ei].spline = Spline::default();
        }
    }
    if let Some((_, ei, err)) = rejected {
        // neatoinit.c:user_spline: a `static atomic_flag`, so once per process.
        static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            let e = &graph.edges[ei];
            let (t, h) = (
                &graph.nodes[e.tail.0 as usize].name,
                &graph.nodes[e.head.0 as usize].name,
            );
            graphviz_pure_core::diag::agwarningf(&match err {
                SplineError::Count => {
                    format!("pos attribute for edge ({t},{h}) doesn't have 3n+1 points\n")
                }
                _ => format!("syntax error in pos attribute for edge ({t},{h})\n"),
            });
        }
    }
    let all_edges = with_spline == graph.edges.len();
    let no_edges = with_spline == 0 && !graph.edges.is_empty();

    // compute_bb
    let has_spline = |graph: &Graph, ei: usize| !graph.edges[ei].spline.segments.is_empty();
    let compute_bb = |graph: &Graph,
                      npos: &[(f64, f64)],
                      node_xl: &[Option<Lbl>],
                      edge_labels: &[EdgeLabels],
                      root_label: &Option<(Lbl, u8, Point)>| {
        if graph.nodes.is_empty() && root_clusters.is_empty() {
            return BoundingBox::default();
        }
        let big = f64::from(i32::MAX);
        let mut bb = BoundingBox::new(big, big, -big, -big);
        let mut out_edges: Vec<Vec<usize>> = vec![Vec::new(); graph.nodes.len()];
        for (ei, e) in graph.edges.iter().enumerate() {
            out_edges[e.tail.0 as usize].push(ei);
        }
        for (ni, n) in graph.nodes.iter().enumerate() {
            let c = Point::new(POINTS_PER_INCH * npos[ni].0, POINTS_PER_INCH * npos[ni].1);
            let s2 = Point::new(
                n.width * POINTS_PER_INCH / 2.0,
                n.height * POINTS_PER_INCH / 2.0,
            );
            bb.ll.x = bb.ll.x.min(c.x - s2.x);
            bb.ll.y = bb.ll.y.min(c.y - s2.y);
            bb.ur.x = bb.ur.x.max(c.x + s2.x);
            bb.ur.y = bb.ur.y.max(c.y + s2.y);
            if let Some(l) = node_xl[ni].filter(|l| l.set) {
                add_label_bb(&mut bb, &l);
            }
            for &ei in &out_edges[ni] {
                if !has_spline(graph, ei) {
                    continue;
                }
                for seg in &graph.edges[ei].spline.segments {
                    for p in &seg.points {
                        bb.ll.x = bb.ll.x.min(p.x);
                        bb.ll.y = bb.ll.y.min(p.y);
                        bb.ur.x = bb.ur.x.max(p.x);
                        bb.ur.y = bb.ur.y.max(p.y);
                    }
                }
                let el = &edge_labels[ei];
                for l in [el.label, el.head, el.tail, el.xlabel]
                    .into_iter()
                    .flatten()
                {
                    if l.set {
                        add_label_bb(&mut bb, &l);
                    }
                }
            }
        }
        for &ci in &root_clusters {
            let cb = graph.subgraphs[clusters[ci].id.0 as usize].bb;
            bb.ll.x = bb.ll.x.min(cb.ll.x);
            bb.ll.y = bb.ll.y.min(cb.ll.y);
            bb.ur.x = bb.ur.x.max(cb.ur.x);
            bb.ur.y = bb.ur.y.max(cb.ur.y);
        }
        if let Some((l, _, _)) = root_label
            && l.set
        {
            add_label_bb(&mut bb, l);
        }
        bb
    };
    let mut bb = compute_bb(graph, &npos, &node_xl, &edge_labels, &root_label);

    // neato_translate
    let translate_all = |graph: &mut Graph,
                         npos: &mut [(f64, f64)],
                         node_xl: &mut [Option<Lbl>],
                         edge_labels: &mut [EdgeLabels],
                         clusters: &mut [Clust],
                         root_label: &mut Option<(Lbl, u8, Point)>,
                         bb: &mut BoundingBox| {
        let ll = bb.ll;
        let off = (ll.x / POINTS_PER_INCH, ll.y / POINTS_PER_INCH);
        for (ni, p) in npos.iter_mut().enumerate() {
            p.0 -= off.0;
            p.1 -= off.1;
            if let Some(l) = node_xl[ni].as_mut().filter(|l| l.set) {
                l.pos.x -= ll.x;
                l.pos.y -= ll.y;
            }
        }
        for (ei, e) in graph.edges.iter_mut().enumerate() {
            if e.spline.segments.is_empty() {
                continue;
            }
            for seg in &mut e.spline.segments {
                for p in &mut seg.points {
                    p.x -= ll.x;
                    p.y -= ll.y;
                }
                for p in [&mut seg.start_arrow, &mut seg.end_arrow]
                    .into_iter()
                    .flatten()
                {
                    p.x -= ll.x;
                    p.y -= ll.y;
                }
            }
            let el = &mut edge_labels[ei];
            for l in [&mut el.label, &mut el.xlabel, &mut el.head, &mut el.tail]
                .into_iter()
                .flatten()
            {
                if l.set {
                    l.pos.x -= ll.x;
                    l.pos.y -= ll.y;
                }
            }
        }
        bb.ll.x -= ll.x;
        bb.ll.y -= ll.y;
        bb.ur.x -= ll.x;
        bb.ur.y -= ll.y;
        if let Some((l, _, _)) = root_label.as_mut()
            && l.set
        {
            l.pos.x -= ll.x;
            l.pos.y -= ll.y;
        }
        for c in clusters.iter_mut() {
            let sb = &mut graph.subgraphs[c.id.0 as usize].bb;
            sb.ll.x -= ll.x;
            sb.ll.y -= ll.y;
            sb.ur.x -= ll.x;
            sb.ur.y -= ll.y;
            if let Some(l) = c.label.as_mut().filter(|l| l.set) {
                l.pos.x -= ll.x;
                l.pos.y -= ll.y;
            }
        }
    };
    let translate = !mapbool(graph.graph_attrs.get("notranslate").map(String::as_str));
    if translate && (bb.ll.x != 0.0 || bb.ll.y != 0.0) {
        translate_all(
            graph,
            &mut npos,
            &mut node_xl,
            &mut edge_labels,
            &mut clusters,
            &mut root_label,
            &mut bb,
        );
    }

    // neato_set_aspect
    let ratio = graph
        .graph_attrs
        .get("ratio")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let ratio_kind = match ratio.as_str() {
        "" => 0,
        "auto" => 1,
        "compress" => 2,
        "expand" => 3,
        "fill" => 4,
        s if atof(s) > 0.0 => 5,
        _ => 0,
    };
    let mut did_shift = false;
    if ratio_kind != 0 {
        if bb.ll.x != 0.0 || bb.ll.y != 0.0 {
            did_shift = true;
            translate_all(
                graph,
                &mut npos,
                &mut node_xl,
                &mut edge_labels,
                &mut clusters,
                &mut root_label,
                &mut bb,
            );
        }
        let (size, _) =
            crate::common::init::getdoubles2ptf(graph.graph_attrs.get("size").map(String::as_str));
        let size = Some((size.x, size.y));
        let factors = match ratio_kind {
            4 => size.filter(|s| s.0 > 0.0).map(|(sx, sy)| {
                let mut xf = sx / bb.ur.x;
                let mut yf = sy / bb.ur.y;
                if xf < 1.0 || yf < 1.0 {
                    if xf < yf {
                        yf /= xf;
                        xf = 1.0;
                    } else {
                        xf /= yf;
                        yf = 1.0;
                    }
                }
                (xf, yf)
            }),
            3 => size.filter(|s| s.0 > 0.0).and_then(|(sx, sy)| {
                let (xf, yf) = (sx / bb.ur.x, sy / bb.ur.y);
                (xf > 1.0 && yf > 1.0).then(|| {
                    let scale = xf.min(yf);
                    (scale, scale)
                })
            }),
            5 => {
                let desired = atof(&ratio);
                let actual = bb.ur.y / bb.ur.x;
                Some(if actual < desired {
                    (1.0, desired / actual)
                } else {
                    (actual / desired, 1.0)
                })
            }
            _ => None,
        };
        if let Some((xf, yf)) = factors {
            did_shift = true;
            if nop > 1 {
                for e in &mut graph.edges {
                    if e.spline.segments.is_empty() {
                        continue;
                    }
                    let (h, t) = (npos[e.head.0 as usize], npos[e.tail.0 as usize]);
                    let delh = (
                        POINTS_PER_INCH * (h.0 * (xf - 1.0)),
                        POINTS_PER_INCH * (h.1 * (yf - 1.0)),
                    );
                    let delt = (
                        POINTS_PER_INCH * (t.0 * (xf - 1.0)),
                        POINTS_PER_INCH * (t.1 * (yf - 1.0)),
                    );
                    let nseg = e.spline.segments.len();
                    for (i, seg) in e.spline.segments.iter_mut().enumerate() {
                        let npts = seg.points.len();
                        for (j, p) in seg.points.iter_mut().enumerate() {
                            if i == 0 && j == 0 {
                                p.x += delt.0;
                                p.y += delt.1;
                            } else if i == nseg - 1 && j == npts - 1 {
                                p.x += delh.0;
                                p.y += delh.1;
                            } else {
                                p.x *= xf;
                                p.y *= yf;
                            }
                        }
                        if let Some(p) = seg.start_arrow.as_mut() {
                            p.x += delt.0;
                            p.y += delt.1;
                        }
                        if let Some(p) = seg.end_arrow.as_mut() {
                            p.x += delh.0;
                            p.y += delh.1;
                        }
                    }
                }
            }
            for p in &mut npos {
                p.0 *= xf;
                p.1 *= yf;
            }
            bb.ll.x *= xf;
            bb.ll.y *= yf;
            bb.ur.x *= xf;
            bb.ur.y *= yf;
            if let Some((l, _, _)) = root_label.as_mut()
                && l.set
            {
                l.pos.x *= xf;
                l.pos.y *= yf;
            }
            for c in &mut clusters {
                let sb = &mut graph.subgraphs[c.id.0 as usize].bb;
                sb.ll.x *= xf;
                sb.ll.y *= yf;
                sb.ur.x *= xf;
                sb.ur.y *= yf;
                if let Some(l) = c.label.as_mut().filter(|l| l.set) {
                    l.pos.x *= xf;
                    l.pos.y *= yf;
                }
            }
        }
    }
    for (ni, node) in graph.nodes.iter_mut().enumerate() {
        node.pos = Point::new(POINTS_PER_INCH * npos[ni].0, POINTS_PER_INCH * npos[ni].1);
    }

    // Edges without a usable spline (all of them under -n1, or after a rescale) are routed
    // by neato's spline_edges0 (`spline_edges_` with `useEdges = Nop > 1`).
    let reroute_all = !no_edges && did_shift;
    if !all_edges || reroute_all || nop < 2 {
        let keep: Vec<bool> = (0..graph.edges.len())
            .map(|ei| !reroute_all && nop >= 2 && !graph.edges[ei].spline.segments.is_empty())
            .collect();
        // The ports were resolved, and warned about, by the first LGraph.
        let routed = graphviz_pure_core::diag::silently(|| route_missing_edges(graph, &keep));
        for (ei, (spline, labels)) in routed.into_iter().enumerate() {
            if keep[ei] {
                continue;
            }
            graph.edges[ei].spline = spline;
            let el = &mut edge_labels[ei];
            for (slot, pos) in [&mut el.label, &mut el.xlabel, &mut el.head, &mut el.tail]
                .into_iter()
                .zip(labels)
            {
                if let (Some(l), Some(p)) = (slot.as_mut(), pos)
                    && !l.set
                {
                    l.pos = p;
                    l.set = true;
                }
            }
        }
    }

    // gv_postprocess(g, 0): place cluster labels, then the root label.
    fn place_graph_label(graph: &Graph, clusters: &mut [Clust], idx: usize) {
        let bb = graph.subgraphs[clusters[idx].id.0 as usize].bb;
        let (loc, border) = (clusters[idx].label_loc, clusters[idx].border);
        if let Some(l) = clusters[idx].label.as_mut()
            && !l.set
        {
            let y = if loc & 1 != 0 {
                bb.ur.y - border.y / 2.0
            } else {
                bb.ll.y + border.y / 2.0
            };
            let x = if loc & 4 != 0 {
                bb.ur.x - border.x / 2.0
            } else if loc & 2 != 0 {
                bb.ll.x + border.x / 2.0
            } else {
                (bb.ll.x + bb.ur.x) / 2.0
            };
            l.pos = Point::new(x, y);
            l.set = true;
        }
        for c in clusters[idx].children.clone() {
            place_graph_label(graph, clusters, c);
        }
    }
    for &ci in &root_clusters {
        place_graph_label(graph, &mut clusters, ci);
    }
    if let Some((l, loc, _)) = root_label.as_mut()
        && !l.set
    {
        let dimen = Point::new(l.dimen.x + LABEL_PAD_X, l.dimen.y + LABEL_PAD_Y);
        if *loc & 1 != 0 {
            bb.ur.y += dimen.y;
        } else {
            bb.ll.y -= dimen.y;
        }
        if dimen.x > bb.ur.x - bb.ll.x {
            let diff = (dimen.x - (bb.ur.x - bb.ll.x)) / 2.0;
            bb.ll.x -= diff;
            bb.ur.x += diff;
        }
        // place_root_label
        let x = if *loc & 4 != 0 {
            bb.ur.x - dimen.x / 2.0
        } else if *loc & 2 != 0 {
            bb.ll.x + dimen.x / 2.0
        } else {
            (bb.ll.x + bb.ur.x) / 2.0
        };
        let y = if *loc & 1 != 0 {
            bb.ur.y - dimen.y / 2.0
        } else {
            bb.ll.y + dimen.y / 2.0
        };
        l.pos = Point::new(x, y);
        l.set = true;
    }

    // Store the results.
    graph.bb = bb;
    graph
        .graph_attrs
        .insert("\u{1}nop".to_string(), nop.to_string());
    // Graph and cluster label positions go to a private key: `lp` itself keeps the parsed
    // value, which subgraphs inherited when they were opened.
    if let Some((l, _, _)) = root_label {
        graph
            .graph_attrs
            .insert(LABEL_POS_KEY.to_string(), exact_point(l.pos));
    }
    for c in &clusters {
        if let Some(l) = c.label {
            graph.subgraphs[c.id.0 as usize]
                .graph_attrs
                .insert(LABEL_POS_KEY.to_string(), exact_point(l.pos));
        }
    }
    for (ni, xl) in node_xl.iter().enumerate() {
        if let Some(l) = xl.filter(|l| l.set) {
            graph.nodes[ni]
                .attrs
                .insert("xlp".to_string(), exact_point(l.pos));
        }
    }
    for (ei, el) in edge_labels.iter().enumerate() {
        let e = &mut graph.edges[ei];
        e.label_pos = el.label.filter(|l| l.set).map(|l| l.pos);
        for (key, l) in [
            ("xlp", el.xlabel),
            ("head_lp", el.head),
            ("tail_lp", el.tail),
        ] {
            if let Some(l) = l.filter(|l| l.set) {
                e.attrs.insert(key.to_string(), exact_point(l.pos));
            }
        }
    }
    Ok(())
}

type RoutedEdge = (Spline, [Option<Point>; 4]);

/// `spline_edges_` for the edges not in `keep`: straight lines (or the `splines` routing)
/// clipped to the node shapes, through the neato spline infrastructure.
fn route_missing_edges(graph: &Graph, keep: &[bool]) -> Vec<RoutedEdge> {
    use crate::common::types::{Bezier, EdgeType};
    use crate::neatogen::neatosplines::{make_self_arcs, make_straight_edge};
    let mut lg = crate::common::types::LGraph::new(graph, 2, EdgeType::Line);
    for (i, n) in graph.nodes.iter().enumerate() {
        lg.nodes[i].coord = n.pos;
        lg.nodes[i].pos = vec![n.pos.x / POINTS_PER_INCH, n.pos.y / POINTS_PER_INCH];
    }
    for (ei, e) in graph.edges.iter().enumerate() {
        lg.edges[ei].spl = keep[ei].then(|| {
            e.spline
                .segments
                .iter()
                .map(|s| Bezier {
                    list: s.points.clone(),
                    sflag: u32::from(s.start_arrow.is_some()),
                    eflag: u32::from(s.end_arrow.is_some()),
                    sp: s.start_arrow.unwrap_or_default(),
                    ep: s.end_arrow.unwrap_or_default(),
                })
                .collect()
        });
        lg.edges[ei].count = 0;
        lg.edges[ei].to_virt = None;
    }
    let sub = lg.root_sub();
    for n in 0..lg.nodes.len() {
        let outs: Vec<usize> = lg.outs(&sub, n).collect();
        for e in outs {
            crate::common::ports::resolve_ports(&mut lg, e);
        }
    }
    // equivEdge classes among the edges to route; kept edges are their own class.
    let bits = |p: Point| [p.x.to_bits(), p.y.to_bits()];
    let mut map: std::collections::HashMap<(usize, [u64; 2], usize, [u64; 2]), usize> =
        std::collections::HashMap::new();
    for n in 0..lg.nodes.len() {
        let outs: Vec<usize> = lg.outs(&sub, n).collect();
        for e in outs {
            if keep[e] {
                lg.edges[e].count += 1;
                continue;
            }
            let ed = &lg.edges[e];
            let (t, h, tp, hp) = (ed.tail, ed.head, ed.tail_port.p, ed.head_port.p);
            let key = if t < h {
                (t, bits(tp), h, bits(hp))
            } else if t > h {
                (h, bits(hp), t, bits(tp))
            } else {
                let (p1, p2) = if (tp.x, tp.y) <= (hp.x, hp.y) {
                    (tp, hp)
                } else {
                    (hp, tp)
                };
                (t, bits(p1), t, bits(p2))
            };
            match map.get(&key) {
                Some(&leader) => {
                    lg.edges[leader].count += 1;
                    lg.edges[e].to_virt = lg.edges[leader].to_virt;
                    lg.edges[leader].to_virt = Some(e);
                }
                None => {
                    lg.edges[e].count = 1;
                    map.insert(key, e);
                }
            }
        }
    }
    let et = lg.edge_type;
    let router = if et >= EdgeType::Pline {
        let margin = crate::neatogen::adjust::esep_factor(&lg);
        crate::neatogen::routing::Router::new(&mut lg, &sub, &margin, et)
    } else {
        None
    };
    if let Some(r) = &router {
        r.path_pass(&mut lg, &sub);
    }
    for n in 0..lg.nodes.len() {
        let outs: Vec<usize> = lg.outs(&sub, n).collect();
        for e in outs {
            if keep[e] || lg.edges[e].count == 0 {
                continue;
            }
            if lg.edges[e].head == n {
                let step = lg.nodesep as f64;
                make_self_arcs(&mut lg, e, step);
            } else if let Some(r) = &router {
                r.route_edge(&mut lg, &sub, e, et);
            } else {
                make_straight_edge(&mut lg, &sub, e, et);
            }
        }
    }
    lg.edges
        .iter()
        .map(|e| {
            let spline = Spline {
                segments: e
                    .spl
                    .as_ref()
                    .map(|v| {
                        v.iter()
                            .map(|b| BezierSegment {
                                points: b.list.clone(),
                                start_arrow: (b.sflag != 0).then_some(b.sp),
                                end_arrow: (b.eflag != 0).then_some(b.ep),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            };
            let pos = |l: &Option<crate::common::types::TextLabel>| {
                l.as_ref().filter(|l| l.set).map(|l| l.pos)
            };
            (
                spline,
                [
                    pos(&e.label),
                    pos(&e.xlabel),
                    pos(&e.head_label),
                    pos(&e.tail_label),
                ],
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Issue #220: user_spline's sscanf("%lf,%lf%n") reads hexadecimal floats and `nan(chars)`.
    #[test]
    fn user_spline_reads_hex_and_nan_chars_like_c() {
        let s = user_spline("0x10,0 10,10 20,20 30,30", true, true).unwrap();
        assert_eq!(s.segments[0].points[0], Point::new(16.0, 0.0));
        let s = user_spline("nan(1),0 10,10 20,20 0x1p-1,30", true, true).unwrap();
        assert!(s.segments[0].points[0].x.is_nan());
        assert_eq!(s.segments[0].points[3], Point::new(0.5, 30.0));
        assert_eq!(scan_point("0x1.8p1,2x").map(|x| x.1), Some("x"));
    }

    #[test]
    fn user_spline_parses_arrows_and_multiple_beziers() {
        let s = user_spline("s,1,2 e,9,9 1,1 2,2 3,3 4,4;5,5 6,6 7,7 8,8", true, true).unwrap();
        assert_eq!(s.segments.len(), 2);
        assert_eq!(s.segments[0].start_arrow, Some(Point::new(1.0, 2.0)));
        assert_eq!(s.segments[0].end_arrow, Some(Point::new(9.0, 9.0)));
        assert_eq!(s.segments[1].points.len(), 4);
        // An `s,` point without a tail arrow is dropped, as `sflag = stype` is 0.
        let s = user_spline("s,1,2 1,1 2,2 3,3 4,4", false, true).unwrap();
        assert_eq!(s.segments[0].start_arrow, None);
        assert_eq!(
            user_spline("1,1 2,2 3,3", true, true).err(),
            Some(SplineError::Count)
        );
        assert_eq!(
            user_spline("1,1 2,2 x,3 4,4", true, true).err(),
            Some(SplineError::Syntax)
        );
        // numFields counts a ';' after white space as a field.
        let p = "0,0 10,10 20,20 30,30";
        assert_eq!(
            user_spline(&format!("{p} ;{p}"), true, true).err(),
            Some(SplineError::Count)
        );
        assert_eq!(
            user_spline("0,0 10,10 20,20 ;", true, true).err(),
            Some(SplineError::Syntax)
        );
        assert_eq!(
            user_spline(&format!("{p};{p}"), true, true)
                .unwrap()
                .segments
                .len(),
            2
        );
        assert_eq!(
            user_spline(&format!("{p} \x0b;"), true, true).err(),
            Some(SplineError::Count)
        );
    }

    // Issue #181: C skips one byte after a segment, so a segment that ends in a multi-byte
    // character leaves its continuation bytes as the next segment. The port sliced the
    // `str` at that byte and panicked.
    #[test]
    fn user_spline_skips_one_byte_after_a_segment_like_c() {
        let p = "0,0 10,10 20,20 30,30";
        // One field left (the continuation byte): not 3n+1.
        assert_eq!(
            user_spline(&format!("{p}é"), true, true).err(),
            Some(SplineError::Count)
        );
        // The continuation byte is a field of its own, so five fields, not a second segment.
        assert_eq!(
            user_spline(&format!("{p}é 1,1 2,2 3,3 4,4"), true, true).err(),
            Some(SplineError::Count)
        );
        // Four fields, the first starting with the continuation byte: a syntax error.
        assert_eq!(
            user_spline(&format!("{p}é 2,2 3,3 4,4"), true, true).err(),
            Some(SplineError::Syntax)
        );
        assert_eq!(
            user_spline(&format!("{p}é1,1 2,2 3,3 4,4"), true, true).err(),
            Some(SplineError::Syntax)
        );
        // `s,` and `e,` do not match after the skipped byte either.
        assert_eq!(
            user_spline(&format!("{p}és,1,1 2,2 3,3 4,4"), true, true).err(),
            Some(SplineError::Syntax)
        );
    }

    #[test]
    fn nop_translates_to_origin_and_keeps_edges() {
        let mut g = graphviz_pure_parser_free_graph();
        layout_nop(&mut g, 2).unwrap();
        assert_eq!(g.nodes[0].pos, Point::new(27.0, 18.0));
        assert_eq!(g.bb.ll, Point::new(0.0, 0.0));
        assert_eq!(
            g.edges[0].spline.segments[0].points[0],
            Point::new(27.0, 18.0)
        );
    }

    fn graphviz_pure_parser_free_graph() -> Graph {
        let mut g = Graph::new("G", GraphKind::Undirected, false);
        let a = g.ensure_node("a");
        let b = g.ensure_node("b");
        g.nodes[0].attrs.insert("pos".into(), "127,118".into());
        g.nodes[1].attrs.insert("pos".into(), "227,118".into());
        let mut attrs = AttrMap::new();
        attrs.insert("pos".into(), "127,118 150,118 180,118 227,118".into());
        g.add_edge(a, b, attrs);
        g
    }
}
