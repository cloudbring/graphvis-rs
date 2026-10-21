//! The laid-out graph as the emitter sees it: what C keeps in `Agraphinfo_t`, `Agnodeinfo_t`
//! and `Agedgeinfo_t` after layout (`GD_bb`, `GD_clust`, `GD_label`, `ND_coord`, `ND_lw/rw/ht`,
//! `ND_shape_info`, `ND_label`, `ED_spl`, `ED_label`, ...).
//!
//! Node shapes and labels come from `graphviz_pure_geom::node_size` (`common_init_node` and the shape
//! init functions); positions come from the layout (`NodeData::pos`, `EdgeData::spline`,
//! `EdgeData::label_pos`, `SubgraphData::bb`, `Graph::bb`) and the label position attributes
//! (`lp`, `xlp`, `head_lp`, `tail_lp`).

use super::arrows::arrow_flags;
use super::attrs::{Attrs, strtod};
use graphviz_pure_core::font::{PostscriptFontAlias, TextJustify, translate_postscript_fontname};
use graphviz_pure_core::{AttrMap, BoundingBox, Graph, GraphKind, Point};
use graphviz_pure_geom::html_label::HtmlLabel;
use graphviz_pure_geom::node_size::{
    LABEL_AT_LEFT, LABEL_AT_RIGHT, LABEL_AT_TOP, LabelGeometry, NodeGeometry, NodeSizeEnv,
    edge_label_geometry, graph_label_geometry_in, node_geometry,
};

/// `textfont_t`.
#[derive(Debug, Clone)]
pub(crate) struct TextFont {
    pub name: String,
    pub size: f64,
    pub flags: u8,
    pub ps: Option<&'static PostscriptFontAlias>,
}

impl TextFont {
    pub fn new(name: &str, size: f64, flags: u8) -> Self {
        TextFont {
            name: name.to_string(),
            size,
            flags,
            ps: translate_postscript_fontname(name),
        }
    }
}

/// `textspan_t`.
#[derive(Debug, Clone)]
pub(crate) struct TextSpan {
    pub str: String,
    /// `'l'`, `'r'` or `'n'`.
    pub just: char,
    pub width: f64,
    pub height: f64,
    pub yoffset_centerline: f64,
    pub font: TextFont,
}

/// `textlabel_t`.
#[derive(Debug, Clone)]
pub(crate) struct TextLabel {
    pub text: String,
    pub fontname: String,
    pub fontcolor: String,
    pub fontsize: f64,
    pub dimen: Point,
    pub space: Point,
    pub pos: Point,
    pub set: bool,
    pub valign: char,
    pub html: Option<Box<HtmlLabel>>,
    pub is_html: bool,
    pub spans: Vec<TextSpan>,
}

impl TextLabel {
    /// Builds a label from its geometry. `raw` is the attribute value.
    pub fn from_geometry(g: &LabelGeometry, raw: &str, fontcolor: &str) -> Self {
        let mut spans = Vec::new();
        let mut text = raw.to_string();
        if g.html {
            text = match g.html_label.as_deref() {
                Some(HtmlLabel::Table(_)) => "<TABLE>".to_string(),
                // `make_html_label` after a parse error: `lp->html` is cleared and
                // `lp->text` stays the unconverted source.
                None if g.html_failed => shapes_html_inner(raw).to_string(),
                // HTML text: `lp->text` is the source string, entities not decoded.
                _ => shapes_html_inner(raw).to_string(),
            };
        }
        if let Some(t) = &g.text {
            text = t.source.clone();
            for line in &t.lines {
                spans.push(TextSpan {
                    str: line.text.clone(),
                    just: match line.justify {
                        TextJustify::Left => 'l',
                        TextJustify::Right => 'r',
                        TextJustify::Center => 'n',
                    },
                    width: line.width,
                    height: line.height,
                    yoffset_centerline: 0.1 * g.font_size,
                    font: TextFont::new(&g.font_name, g.font_size, 0),
                });
            }
        }
        TextLabel {
            text,
            fontname: g.font_name.clone(),
            fontcolor: fontcolor.to_string(),
            fontsize: g.font_size,
            dimen: g.dimen,
            space: g.space,
            pos: Point::default(),
            set: false,
            valign: if g.valign == '\0' { 'c' } else { g.valign },
            html: g.html_label.clone(),
            is_html: g.html && !g.html_failed,
            spans,
        }
    }
}

/// `raw` without the `<`...`>` delimiters the parser keeps around HTML strings.
fn shapes_html_inner(raw: &str) -> &str {
    if raw.len() >= 2 && raw.starts_with('<') && raw.ends_with('>') {
        &raw[1..raw.len() - 1]
    } else {
        raw
    }
}

/// One Bézier of an edge (`bezier`).
#[derive(Debug, Clone, Default)]
pub(crate) struct Bz {
    pub list: Vec<Point>,
    pub sflag: u32,
    pub eflag: u32,
    pub sp: Point,
    pub ep: Point,
}

pub(crate) struct RNode<'g> {
    pub name: &'g str,
    pub attrs: Attrs<'g>,
    pub geom: NodeGeometry,
    pub label: TextLabel,
    pub xlabel: Option<TextLabel>,
    pub coord: Point,
    pub lw: f64,
    pub rw: f64,
    pub ht: f64,
}

pub(crate) struct REdge<'g> {
    pub tail: usize,
    pub head: usize,
    pub attrs: Attrs<'g>,
    pub bz: Vec<Bz>,
    pub label: Option<TextLabel>,
    pub xlabel: Option<TextLabel>,
    pub head_label: Option<TextLabel>,
    pub tail_label: Option<TextLabel>,
    pub tail_port: Option<&'g str>,
    pub head_port: Option<&'g str>,
}

pub(crate) struct RCluster<'g> {
    pub name: &'g str,
    pub attrs: Attrs<'g>,
    pub sub: usize,
    pub bb: BoundingBox,
    pub label: Option<TextLabel>,
    pub children: Vec<usize>,
    /// Nodes (indices) directly or indirectly in the cluster.
    #[allow(dead_code)]
    pub nodes: Vec<usize>,
}

/// Private attribute with a layout-computed graph or cluster label position (exact).
pub(crate) const LABEL_POS_KEY: &str = "\u{1}lp";
pub(crate) use graphviz_pure_core::CLUST_NODE_KEY;

/// `GD_has_labels` bits.
pub(crate) const EDGE_LABEL: u32 = 1 << 0;
pub(crate) const HEAD_LABEL: u32 = 1 << 1;
pub(crate) const TAIL_LABEL: u32 = 1 << 2;
pub(crate) const GRAPH_LABEL: u32 = 1 << 3;
pub(crate) const NODE_XLABEL: u32 = 1 << 4;
pub(crate) const EDGE_XLABEL: u32 = 1 << 5;

pub(crate) struct Model<'g> {
    pub g: &'g Graph,
    pub directed: bool,
    pub bb: BoundingBox,
    pub label: Option<TextLabel>,
    pub nodes: Vec<RNode<'g>>,
    pub edges: Vec<REdge<'g>>,
    /// Out-edges of each node in edge sequence order (`agfstout`/`agnxtout`).
    pub out_edges: Vec<Vec<usize>>,
    pub clusters: Vec<RCluster<'g>>,
    pub root_clusters: Vec<usize>,
    #[allow(dead_code)]
    pub has_labels: u32,
}

impl Model<'_> {
    /// `AGSEQ(n)`.
    pub fn node_seq(&self, n: usize) -> usize {
        seq_of(&self.g.nodes[n].attrs, n)
    }
    /// `AGSEQ(e)`.
    pub fn edge_seq(&self, e: usize) -> usize {
        seq_of(&self.g.edges[e].attrs, e)
    }
}

fn seq_of(attrs: &AttrMap, index: usize) -> usize {
    attrs
        .get(graphviz_pure_core::SEQ_KEY)
        .and_then(|s| s.parse().ok())
        .unwrap_or(index + 1)
}

fn parse_point(s: Option<&str>) -> Option<Point> {
    let s = s?;
    let (x, rest) = strtod(s)?;
    let (y, _) = strtod(rest.strip_prefix(',')?)?;
    Some(Point::new(x, y))
}

fn is_a_cluster(name: &str, attrs: &AttrMap) -> bool {
    (name.len() >= 7 && name[..7].eq_ignore_ascii_case("cluster"))
        || Attrs(attrs).mapbool("cluster")
}

impl<'g> Model<'g> {
    pub fn root_attrs(&self) -> Attrs<'g> {
        Attrs(&self.g.graph_attrs)
    }

    /// `GD_drawing(g)->size` in points and `filled` (a trailing `!`).
    pub fn drawing_size(&self) -> Option<(f64, f64, bool)> {
        let s = self.root_attrs().nonempty("size")?;
        let (w, rest) = strtod(s)?;
        let (h, rest) = match rest.strip_prefix(',').and_then(strtod) {
            Some((h, r)) => (h, r),
            None => (w, rest),
        };
        let filled = rest.trim_start().starts_with('!');
        Some((w * 72.0, h * 72.0, filled))
    }

    pub fn landscape(&self) -> bool {
        let a = self.root_attrs();
        if a.get("rotate").is_some_and(|r| super::attrs::atoi(r) == 90) {
            return true;
        }
        if a.get("orientation")
            .is_some_and(|o| o.starts_with('l') || o.starts_with('L'))
        {
            return true;
        }
        a.mapbool("landscape")
    }

    /// `N_penwidth != NULL`: some node declares `penwidth`.
    pub fn node_penwidth_declared(&self) -> bool {
        self.g
            .nodes
            .iter()
            .any(|n| n.attrs.contains_key("penwidth"))
            || self.g.node_defaults.contains_key("penwidth")
    }

    pub fn centered(&self) -> bool {
        self.root_attrs().mapbool("center")
    }

    /// The render model of `g`. Its node, edge and cluster labels are sized again here
    /// (`diag::quietly`): C made them in layout, and reported their diagnostics there.
    pub fn build(g: &'g Graph) -> Self {
        graphviz_pure_core::diag::quietly(|| Self::build_quiet(g))
    }

    fn build_quiet(g: &'g Graph) -> Self {
        let directed = g.kind == GraphKind::Directed;
        let env = NodeSizeEnv::from_graph(g);
        let ga = Attrs(&g.graph_attrs);

        let mut has_labels = 0u32;
        let mut nodes = Vec::with_capacity(g.nodes.len());
        for n in &g.nodes {
            let geom = node_geometry(n, &env);
            let a = Attrs(&n.attrs);
            let fontcolor = a.late_nnstring("fontcolor", "black");
            let raw = a.get("label").unwrap_or("\\N");
            let mut label = TextLabel::from_geometry(&geom.label, raw, fontcolor);
            label.pos = n.pos;
            label.set = true;
            let xlabel = geom.xlabel.as_ref().map(|xg| {
                has_labels |= NODE_XLABEL;
                let mut l = TextLabel::from_geometry(xg, a.get("xlabel").unwrap_or(""), fontcolor);
                if let Some(p) = parse_point(a.get("xlp")) {
                    l.pos = p;
                    l.set = true;
                }
                l
            });
            let w = n.width * 72.0;
            // The layout's ND_lw/ND_rw when they differ (dot's self-loop space in ND_rw).
            let (lw, rw) = n
                .attrs
                .get(graphviz_pure_core::NODE_LW_RW_KEY)
                .and_then(|s| s.split_once(','))
                .and_then(|(l, r)| Some((l.parse().ok()?, r.parse().ok()?)))
                .unwrap_or((w / 2.0, w / 2.0));
            nodes.push(RNode {
                name: &n.name,
                attrs: a,
                geom,
                label,
                xlabel,
                coord: n.pos,
                lw,
                rw,
                ht: n.height * 72.0,
            });
        }

        let no_edge_init = g
            .graph_attrs
            .contains_key(graphviz_pure_core::NO_EDGE_INIT_KEY);
        let mut edges = Vec::with_capacity(g.edges.len());
        let mut out_edges = vec![Vec::new(); g.nodes.len()];
        for (ei, e) in g.edges.iter().enumerate() {
            out_edges[e.tail.0 as usize].push(ei);
            let a = Attrs(&e.attrs);
            let (mut stype, mut etype) = arrow_flags(a, directed);
            // concentrate=true merges the arrowheads of an opposing edge into
            // this one (`ED_conc_opp_flag` in arrow_flags); the layout only
            // records that an end has an arrow, so take its type from there.
            let has_s = e.spline.segments.iter().any(|s| s.start_arrow.is_some());
            let has_e = e.spline.segments.iter().any(|s| s.end_arrow.is_some());
            if (has_s && stype == 0) || (has_e && etype == 0) {
                let opp = g
                    .edges
                    .iter()
                    .find(|o| o.tail == e.head && o.head == e.tail)
                    .map(|o| arrow_flags(Attrs(&o.attrs), directed));
                if let Some((os, oe)) = opp {
                    stype |= oe;
                    etype |= os;
                }
            }
            let bz = e
                .spline
                .segments
                .iter()
                .map(|s| Bz {
                    list: s.points.clone(),
                    // An end the layout swapped (`swapEnds`, e.g. flat edges
                    // routed backwards) carries the other end's arrow type.
                    sflag: match (s.start_arrow.is_some(), stype) {
                        (false, _) => 0,
                        (true, 0) => etype,
                        (true, t) => t,
                    },
                    eflag: match (s.end_arrow.is_some(), etype) {
                        (false, _) => 0,
                        (true, 0) => stype,
                        (true, t) => t,
                    },
                    sp: s.start_arrow.unwrap_or_default(),
                    ep: s.end_arrow.unwrap_or_default(),
                })
                .collect();
            let tname = &g.nodes[e.tail.0 as usize].name;
            let hname = &g.nodes[e.head.0 as usize].name;
            let lg = if no_edge_init {
                graphviz_pure_geom::node_size::EdgeLabelGeometry::default()
            } else {
                edge_label_geometry(&e.attrs, tname, hname, directed, &env)
            };
            let fontcolor = a.late_nnstring("fontcolor", "black");
            let lfontcolor = a.late_nnstring("labelfontcolor", fontcolor);
            let mk = |geo: &Option<LabelGeometry>,
                      key: &str,
                      pos: Option<Point>,
                      color: &str|
             -> Option<TextLabel> {
                geo.as_ref().map(|lgeo| {
                    let mut l = TextLabel::from_geometry(lgeo, a.get(key).unwrap_or(""), color);
                    if let Some(p) = pos {
                        l.pos = p;
                        l.set = true;
                    }
                    l
                })
            };
            let mut label = mk(&lg.label, "label", e.label_pos, fontcolor);
            if e.label_unplaced
                && let Some(l) = label.as_mut()
            {
                l.set = false;
            }
            let xlabel = mk(&lg.xlabel, "xlabel", parse_point(a.get("xlp")), fontcolor);
            let mut head_label = mk(
                &lg.head_label,
                "headlabel",
                parse_point(a.get("head_lp")),
                lfontcolor,
            );
            let mut tail_label = mk(
                &lg.tail_label,
                "taillabel",
                parse_point(a.get("tail_lp")),
                lfontcolor,
            );
            if let Some(u) = e.attrs.get(graphviz_pure_core::UNPLACED_PORT_LABELS_KEY) {
                for (c, l) in [('h', &mut head_label), ('t', &mut tail_label)] {
                    if u.contains(c)
                        && let Some(l) = l.as_mut()
                    {
                        l.set = false;
                    }
                }
            }
            if label.is_some() {
                has_labels |= EDGE_LABEL;
            }
            if xlabel.is_some() {
                has_labels |= EDGE_XLABEL;
            }
            if head_label.is_some() {
                has_labels |= HEAD_LABEL;
            }
            if tail_label.is_some() {
                has_labels |= TAIL_LABEL;
            }
            edges.push(REdge {
                tail: e.tail.0 as usize,
                head: e.head.0 as usize,
                attrs: a,
                bz,
                label,
                xlabel,
                head_label,
                tail_label,
                tail_port: e
                    .attrs
                    .get(graphviz_pure_core::TAIL_PORT_NAME_KEY)
                    .map_or_else(
                        || {
                            e.tail_port
                                .compass
                                .as_deref()
                                .or(e.tail_port.name.as_deref())
                        },
                        |s| Some(s.as_str()).filter(|s| !s.is_empty()),
                    ),
                head_port: e
                    .attrs
                    .get(graphviz_pure_core::HEAD_PORT_NAME_KEY)
                    .map_or_else(
                        || {
                            e.head_port
                                .compass
                                .as_deref()
                                .or(e.head_port.name.as_deref())
                        },
                        |s| Some(s.as_str()).filter(|s| !s.is_empty()),
                    ),
            });
        }

        // `agfstout`/`agnxtout` order: by head node id (cgraph `agedgeseqcmpf`), then by edge
        // sequence. Node ids follow the interning of their names, approximated by creation order.
        for list in &mut out_edges {
            list.sort_by_key(|&ei| (g.edges[ei].head.0, ei));
        }

        // Cluster tree (`GD_clust`): clusters with a box, found through plain subgraphs.
        let mut is_child = vec![false; g.subgraphs.len()];
        for s in &g.subgraphs {
            for c in &s.subgraphs {
                is_child[c.0 as usize] = true;
            }
        }
        let mut clusters: Vec<RCluster<'g>> = Vec::new();
        let mut root_clusters = Vec::new();
        let flip = false;
        fn dfs<'g>(
            g: &'g Graph,
            sid: usize,
            out: &mut Vec<usize>,
            clusters: &mut Vec<RCluster<'g>>,
            has_labels: &mut u32,
            env: &NodeSizeEnv<'_>,
            flip: bool,
        ) {
            let s = &g.subgraphs[sid];
            if is_a_cluster(&s.name, &s.graph_attrs)
                && (s.bb != BoundingBox::default()
                    || s.graph_attrs
                        .contains_key(graphviz_pure_core::REGISTERED_CLUSTER_KEY))
                && !s
                    .graph_attrs
                    .contains_key(graphviz_pure_core::UNREGISTERED_CLUSTER_KEY)
            {
                let a = Attrs(&s.graph_attrs);
                let label = if s
                    .graph_attrs
                    .contains_key(graphviz_pure_core::NO_GRAPH_LABEL_KEY)
                {
                    None
                } else {
                    graph_label_geometry_in(&s.name, &s.graph_attrs, false, flip, env)
                };
                let label = label.map(|gl| {
                    *has_labels |= GRAPH_LABEL;
                    let mut l = TextLabel::from_geometry(
                        &gl.label,
                        a.get("label").unwrap_or(""),
                        a.late_nnstring("fontcolor", "black"),
                    );
                    l.pos = parse_point(a.get(LABEL_POS_KEY))
                        .or_else(|| parse_point(a.get("lp")))
                        .unwrap_or_else(|| {
                            let d = gl.border.map_or(Point::default(), |b| b.1);
                            let bb = s.bb;
                            let y = if gl.pos_flag & LABEL_AT_TOP != 0 {
                                bb.ur.y - d.y / 2.0
                            } else {
                                bb.ll.y + d.y / 2.0
                            };
                            let x = if gl.pos_flag & LABEL_AT_RIGHT != 0 {
                                bb.ur.x - d.x / 2.0
                            } else if gl.pos_flag & LABEL_AT_LEFT != 0 {
                                bb.ll.x + d.x / 2.0
                            } else {
                                (bb.ll.x + bb.ur.x) / 2.0
                            };
                            Point::new(x, y)
                        });
                    l.set = true;
                    l
                });
                let idx = clusters.len();
                clusters.push(RCluster {
                    name: &s.name,
                    attrs: a,
                    sub: sid,
                    bb: s.bb,
                    label,
                    children: Vec::new(),
                    nodes: s.nodes.iter().map(|n| n.0 as usize).collect(),
                });
                out.push(idx);
                let mut kids = Vec::new();
                for c in &s.subgraphs {
                    dfs(g, c.0 as usize, &mut kids, clusters, has_labels, env, flip);
                }
                clusters[idx].children = kids;
            } else {
                for c in &s.subgraphs {
                    dfs(g, c.0 as usize, out, clusters, has_labels, env, flip);
                }
            }
        }
        for (i, child) in is_child.iter().enumerate() {
            if !child {
                dfs(
                    g,
                    i,
                    &mut root_clusters,
                    &mut clusters,
                    &mut has_labels,
                    &env,
                    flip,
                );
            }
        }
        // A layout that exported its `GD_clust` lists (dot): walk them as they are,
        // in their order and with their repeats (`copyClusterInfo` in the pack branch).
        // A graph's `GD_clust` only holds clusters below it, so entries that are not
        // descendants (the key written in the input, inherited by subgraphs) are dropped:
        // they could make the cluster tree cyclic.
        let mut of_sub: Vec<Option<usize>> = vec![None; g.subgraphs.len()];
        for (ci, c) in clusters.iter().enumerate() {
            of_sub[c.sub] = Some(ci);
        }
        let mut parent: Vec<Option<usize>> = vec![None; g.subgraphs.len()];
        for (i, s) in g.subgraphs.iter().enumerate() {
            for c in &s.subgraphs {
                parent[c.0 as usize] = Some(i);
            }
        }
        let below = |si: usize, owner: Option<usize>| {
            let Some(owner) = owner else { return true };
            let mut cur = parent[si];
            while let Some(p) = cur {
                if p == owner {
                    return true;
                }
                cur = parent[p];
            }
            false
        };
        let listed = |attrs: &AttrMap, owner: Option<usize>| -> Option<Vec<usize>> {
            let list = attrs.get(graphviz_pure_core::CLUSTER_LIST_KEY)?;
            Some(
                list.split_ascii_whitespace()
                    .filter_map(|si| si.parse::<usize>().ok())
                    .filter(|&si| si < parent.len() && below(si, owner))
                    .filter_map(|si| of_sub[si])
                    .collect(),
            )
        };
        if let Some(list) = listed(&g.graph_attrs, None) {
            root_clusters = list;
        }
        // Only the root's list repeats clusters (`copyClusterInfo`); repeats below it
        // would multiply at every level.
        for c in &mut clusters {
            if let Some(mut list) = listed(&g.subgraphs[c.sub].graph_attrs, Some(c.sub)) {
                let mut seen = std::collections::HashSet::new();
                list.retain(|&k| seen.insert(k));
                c.children = list;
            }
        }

        let bb = g.bb;
        let label = graph_label_geometry_in(&g.name, &g.graph_attrs, true, flip, &env).map(|gl| {
            has_labels |= GRAPH_LABEL;
            let mut l = TextLabel::from_geometry(
                &gl.label,
                ga.get("label").unwrap_or(""),
                ga.late_nnstring("fontcolor", "black"),
            );
            l.pos = parse_point(ga.get(LABEL_POS_KEY))
                .or_else(|| parse_point(ga.get("lp")))
                .unwrap_or_else(|| {
                    // place_root_label
                    let d = Point::new(gl.label.dimen.x + 16.0, gl.label.dimen.y + 8.0);
                    let x = if gl.pos_flag & LABEL_AT_RIGHT != 0 {
                        bb.ur.x - d.x / 2.0
                    } else if gl.pos_flag & LABEL_AT_LEFT != 0 {
                        bb.ll.x + d.x / 2.0
                    } else {
                        (bb.ll.x + bb.ur.x) / 2.0
                    };
                    let y = if gl.pos_flag & LABEL_AT_TOP != 0 {
                        bb.ur.y - d.y / 2.0
                    } else {
                        bb.ll.y + d.y / 2.0
                    };
                    Point::new(x, y)
                });
            l.set = true;
            l
        });

        Model {
            g,
            directed,
            bb,
            label,
            nodes,
            edges,
            out_edges,
            clusters,
            root_clusters,
            has_labels,
        }
    }
}
