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

use gv_core::font::{DEFAULT_FONT_NAME, DEFAULT_FONT_SIZE, LabelSubstContext};
use gv_core::{
    AttrMap, BezierSegment, BoundingBox, Graph, GraphKind, GraphvizError, Point, Result, Spline,
    SubgraphId,
};

const POINTS_PER_INCH: f64 = 72.0;

/// Graph attribute holding a graph or cluster label position computed by the layout, in
/// exact notation. Renderers prefer it to `lp`; output writers never print it.
pub const LABEL_POS_KEY: &str = "\u{1}lp";
/// `PAD` in `const.h`: horizontal and vertical label padding.
const LABEL_PAD_X: f64 = 16.0;
const LABEL_PAD_Y: f64 = 8.0;

/// Parses a C `%lf` prefix (after optional white space) and returns the value and the rest.
pub(crate) fn scan_double(s: &str) -> Option<(f64, &str)> {
    let t = s.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let b = t.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let lower = t[i..].to_ascii_lowercase();
    for word in ["infinity", "inf", "nan"] {
        if lower.starts_with(word) {
            let end = i + word.len();
            return t[..end].parse::<f64>().ok().map(|v| (v, &t[end..]));
        }
    }
    let digits_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut mantissa_digits = i - digits_start;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let frac_start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        mantissa_digits += i - frac_start;
    }
    if mantissa_digits == 0 {
        return None;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let exp_start = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp_start {
            i = j;
        }
    }
    let text = &t[..i];
    let text = text.strip_suffix('.').unwrap_or(text);
    let v = if text.is_empty() || text == "+" || text == "-" {
        0.0
    } else {
        text.parse::<f64>().ok()?
    };
    Some((v, &t[i..]))
}

/// `sscanf(s, "%lf,%lf", ...) == 2`.
pub(crate) fn scan_point(s: &str) -> Option<(Point, &str)> {
    let (x, rest) = scan_double(s)?;
    let rest = rest.strip_prefix(',')?;
    let (y, rest) = scan_double(rest)?;
    Some((Point::new(x, y), rest))
}

/// `sscanf(s, "%lf,%lf,%lf,%lf", ...) == 4`.
fn scan_box(s: &str) -> Option<BoundingBox> {
    let (a, rest) = scan_double(s)?;
    let (b, rest) = scan_double(rest.strip_prefix(',')?)?;
    let (c, rest) = scan_double(rest.strip_prefix(',')?)?;
    let (d, _) = scan_double(rest.strip_prefix(',')?)?;
    Some(BoundingBox::new(a, b, c, d))
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
    name.len() >= 7 && name[..7].eq_ignore_ascii_case("cluster")
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

fn count_fields(s: &str) -> usize {
    // numFields: whitespace-separated tokens up to the next ';'.
    let b = s.as_bytes();
    let mut i = 0;
    let mut cnt = 0;
    loop {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || b[i] == b';' {
            return cnt;
        }
        cnt += 1;
        while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b';' {
            i += 1;
        }
        if i >= b.len() || b[i] == b';' {
            return cnt;
        }
    }
}

/// `user_spline`: parses an edge `pos` attribute.
fn user_spline(pos: &str, tail_arrow: bool, head_arrow: bool) -> Option<Spline> {
    if pos.is_empty() {
        return None;
    }
    let mut segments = Vec::new();
    let mut rest = pos;
    // As in C, the arrow points persist across `;`-separated segments: a later segment
    // without its own `s,`/`e,` reuses the previous one.
    let mut sp = None;
    let mut ep = None;
    loop {
        if let Some(r) = rest.strip_prefix("s,")
            && let Some((p, r)) = scan_point(r)
        {
            sp = Some(p);
            rest = r;
        }
        let trimmed = rest.trim_start();
        if let Some(r) = trimmed.strip_prefix("e,")
            && let Some((p, r)) = scan_point(r)
        {
            ep = Some(p);
            rest = r;
        }
        let npts = count_fields(rest);
        if npts < 4 || npts % 3 != 1 {
            return None;
        }
        let mut points = Vec::with_capacity(npts);
        for _ in 0..npts {
            let (p, r) = scan_point(rest)?;
            points.push(p);
            rest = r;
        }
        rest = rest.trim_start();
        let more = !rest.is_empty();
        if more {
            rest = &rest[1..];
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
    Some(Spline { segments })
}

/// Size of a label in points (`make_label`).
fn label_dimen(
    text: &str,
    html: bool,
    fontname: &str,
    fontsize: f64,
    ctx: &LabelSubstContext<'_>,
) -> Point {
    if text.is_empty() {
        return Point::default();
    }
    gv_geom::node_size::make_label(text, html, ctx, fontname, fontsize, false).dimen
}

fn attr_f64(attrs: &AttrMap, key: &str, default: f64, min: f64) -> f64 {
    match attrs.get(key).and_then(|s| scan_double(s)) {
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
    let dimen = label_dimen(text, html.contains(text), fontname, fontsize, &ctx);
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
    gv_geom::node_size::size_graph_nodes(graph);

    // user_pos: positions in points, stored in inches as ND_pos.
    let mut npos: Vec<(f64, f64)> = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        let parsed = node.attrs.get("pos").and_then(|p| scan_point(p));
        match parsed {
            Some((p, _)) => npos.push((p.x / POINTS_PER_INCH, p.y / POINTS_PER_INCH)),
            None if node.name.starts_with("cluster") => npos.push((0.0, 0.0)),
            None => {
                return Err(GraphvizError::LayoutError {
                    stage: "nop",
                    message: format!(
                        "node {} in graph {} has no position (as required by the -n flag)",
                        node.name, graph.name
                    ),
                });
            }
        }
    }

    // Node xlabels.
    let mut node_xl: Vec<Option<Lbl>> = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        let xl = node.attrs.get("xlabel").filter(|s| !s.is_empty()).map(|t| {
            let fs = attr_f64(&node.attrs, "fontsize", DEFAULT_FONT_SIZE, 1.0);
            let fname = attr_str(&node.attrs, "fontname", DEFAULT_FONT_NAME);
            let ctx = LabelSubstContext {
                graph_name: &graph.name,
                node_name: &node.name,
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
    for ei in 0..graph.edges.len() {
        let (tail_arrow, head_arrow) = arrow_flags(graph, &graph.edges[ei].attrs);
        let e = &graph.edges[ei];
        let spline = if nop >= 2 {
            e.attrs
                .get("pos")
                .and_then(|p| user_spline(p, tail_arrow, head_arrow))
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
        s => match scan_double(s) {
            Some((v, _)) if v > 0.0 => 5,
            _ => 0,
        },
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
        let size = graph.graph_attrs.get("size").and_then(|s| {
            let (w, rest) = scan_double(s)?;
            let h = rest
                .strip_prefix(',')
                .and_then(scan_double)
                .map_or(w, |x| x.0);
            Some((w * POINTS_PER_INCH, h * POINTS_PER_INCH))
        });
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
                let desired = scan_double(&ratio).map_or(1.0, |x| x.0);
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
        let routed = route_missing_edges(graph, &keep);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_double_matches_strtod_prefixes() {
        assert_eq!(scan_double("12.5,3").map(|x| x.0), Some(12.5));
        assert_eq!(
            scan_double(" -3e2x").map(|x| (x.0, x.1)),
            Some((-300.0, "x"))
        );
        assert_eq!(scan_double(".5").map(|x| x.0), Some(0.5));
        assert_eq!(scan_double("5.").map(|x| x.0), Some(5.0));
        assert!(scan_double("abc").is_none());
        assert!(scan_double("-").is_none());
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
        assert!(user_spline("1,1 2,2 3,3", true, true).is_none());
    }

    #[test]
    fn nop_translates_to_origin_and_keeps_edges() {
        let mut g = gv_parser_free_graph();
        layout_nop(&mut g, 2).unwrap();
        assert_eq!(g.nodes[0].pos, Point::new(27.0, 18.0));
        assert_eq!(g.bb.ll, Point::new(0.0, 0.0));
        assert_eq!(
            g.edges[0].spline.segments[0].points[0],
            Point::new(27.0, 18.0)
        );
    }

    fn gv_parser_free_graph() -> Graph {
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
