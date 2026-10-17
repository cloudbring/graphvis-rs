//! Authentic Graphviz SVG renderer ported from `plugin/core/gvrender_core_svg.c`,
//! `lib/common/emit.c`, `lib/common/shapes.c`, and `lib/common/arrows.c`.

use gv_core::font::{
    DEFAULT_FONT_NAME, DEFAULT_FONT_SIZE, DEFAULT_LABEL_FONT_SIZE, LINESPACING, LabelSubstContext,
    TextJustify, TextLabelLayout, measure_multiline_label_with_context, substitute_object_escapes,
    translate_postscript_fontname,
};
use gv_core::{
    AttrMap, BoundingBox, EdgeData, Graph, GraphKind, NodeData, Point, Spline, SubgraphData,
    resolve_color_with_scheme,
};
use gv_geom::straight_spline;
use gv_parser::is_html_string;
use std::f64::consts::PI;
use std::fmt::Write;

const DEFAULT_PAD_PT: f64 = 4.0;
const PERIPHERY_GAP_PT: f64 = 4.0;
const PENWIDTH_NORMAL: f64 = 1.0;
const GVPRINT_DOUBLE_THRESHOLD: f64 = 0.005;
const SDASHARRAY: &str = "5,2";
const SDOTARRAY: &str = "1,5";
const ARROW_LENGTH: f64 = 10.0;

/// Formats a floating-point number in Graphviz `gvprintdouble` style (`%.02f` with
/// trailing zeros and trailing decimal point stripped, and `(-0.005, 0.005)` mapped to `"0"`).
pub(crate) fn fmt_gv_double(num: f64) -> String {
    if !num.is_finite() || (-GVPRINT_DOUBLE_THRESHOLD..GVPRINT_DOUBLE_THRESHOLD).contains(&num) {
        return "0".to_string();
    }
    let s = format!("{num:.2}");
    if let Some(dot_idx) = s.find('.') {
        let bytes = s.as_bytes();
        if bytes.len() == dot_idx + 3 && bytes[dot_idx + 2] == b'0' {
            if bytes[dot_idx + 1] == b'0' {
                let prefix = &s[..dot_idx];
                if prefix == "-0" {
                    return "0".to_string();
                }
                return prefix.to_string();
            }
            return s[..dot_idx + 2].to_string();
        }
    }
    if s == "-0" || s == "-0.00" {
        "0".to_string()
    } else {
        s
    }
}

/// XML-escapes a string following `gv_xml_escape` in `lib/util/xml.c`.
pub(crate) fn xml_escape(s: &str, escape_dash: bool, escape_nbsp: bool, raw: bool) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    let mut prev = '\0';
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let ch = s[i..].chars().next().unwrap_or('\0');
        let ch_len = ch.len_utf8();
        match ch {
            '&' => {
                if raw || !xml_is_entity(&s[i..]) {
                    out.push_str("&amp;");
                } else {
                    out.push('&');
                }
            }
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '-' if escape_dash => out.push_str("&#45;"),
            ' ' if prev == ' ' && escape_nbsp => out.push_str("&#160;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '\n' if raw => out.push_str("&#10;"),
            '\r' if raw => out.push_str("&#13;"),
            _ => out.push(ch),
        }
        prev = ch;
        i += ch_len;
    }
    out
}

fn xml_is_entity(s: &str) -> bool {
    let Some(rest) = s.strip_prefix('&') else {
        return false;
    };
    if rest.is_empty() || rest.starts_with(';') {
        return false;
    }
    if let Some(after_hash) = rest.strip_prefix('#') {
        if let Some(hex_part) = after_hash
            .strip_prefix('x')
            .or_else(|| after_hash.strip_prefix('X'))
        {
            let digits = hex_part
                .bytes()
                .take_while(|b| b.is_ascii_hexdigit())
                .count();
            digits > 0 && hex_part.as_bytes().get(digits) == Some(&b';')
        } else {
            let digits = after_hash
                .bytes()
                .take_while(|b| b.is_ascii_digit())
                .count();
            digits > 0 && after_hash.as_bytes().get(digits) == Some(&b';')
        }
    } else {
        let letters = rest.bytes().take_while(|b| b.is_ascii_alphabetic()).count();
        letters > 0 && rest.as_bytes().get(letters) == Some(&b';')
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ParsedStyle {
    pub filled: bool,
    pub rounded: bool,
    pub dashed: bool,
    pub dotted: bool,
    pub bold: bool,
    pub invis: bool,
    pub diagonals: bool,
    pub linewidth: Option<f64>,
}

pub(crate) fn parse_style(style_attr: Option<&str>) -> ParsedStyle {
    let mut parsed = ParsedStyle::default();
    let Some(raw) = style_attr else {
        return parsed;
    };
    for token in raw.split(',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if let Some(arg_part) = token
            .strip_prefix("setlinewidth(")
            .and_then(|s| s.strip_suffix(')'))
        {
            if let Ok(w) = arg_part.trim().parse::<f64>() {
                parsed.linewidth = Some(w.max(0.0));
            }
            continue;
        }
        match token.to_ascii_lowercase().as_str() {
            "filled" => parsed.filled = true,
            "rounded" => parsed.rounded = true,
            "dashed" => parsed.dashed = true,
            "dotted" => parsed.dotted = true,
            "bold" => parsed.bold = true,
            "invis" | "invisible" => parsed.invis = true,
            "diagonals" => parsed.diagonals = true,
            _ => {}
        }
    }
    parsed
}

/// Computes the effective bounding box of the graph in points.
pub(crate) fn effective_graph_bb(graph: &Graph) -> BoundingBox {
    if graph.bb.width() > 0.0 && graph.bb.height() > 0.0 {
        return graph.bb;
    }
    // C prints a degenerate box as is: `0,0,0,0` for an empty graph or one
    // whose only nodes have zero size (e.g. `shape=plain` with no label).
    if graph
        .nodes
        .iter()
        .all(|n| n.width == 0.0 && n.height == 0.0)
    {
        return graph.bb;
    }
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for node in &graph.nodes {
        let rx = (node.width * 72.0 / 2.0).max(1.0);
        let ry = (node.height * 72.0 / 2.0).max(1.0);
        min_x = min_x.min(node.pos.x - rx);
        min_y = min_y.min(node.pos.y - ry);
        max_x = max_x.max(node.pos.x + rx);
        max_y = max_y.max(node.pos.y + ry);
    }
    for subg in &graph.subgraphs {
        if subg.bb.width() > 0.0 && subg.bb.height() > 0.0 {
            min_x = min_x.min(subg.bb.ll.x);
            min_y = min_y.min(subg.bb.ll.y);
            max_x = max_x.max(subg.bb.ur.x);
            max_y = max_y.max(subg.bb.ur.y);
        }
    }
    if !min_x.is_finite() || !min_y.is_finite() {
        return BoundingBox::new(0.0, 0.0, 54.0, 36.0);
    }
    BoundingBox::new(
        min_x.min(0.0),
        min_y.min(0.0),
        max_x.max(54.0),
        max_y.max(36.0),
    )
}

pub(crate) fn effective_cluster_bb(graph: &Graph, subg: &SubgraphData) -> BoundingBox {
    if subg.bb.width() > 0.0 && subg.bb.height() > 0.0 {
        return subg.bb;
    }
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for &nid in &subg.nodes {
        if let Some(node) = graph.nodes.get(nid.0 as usize) {
            let rx = (node.width * 72.0 / 2.0).max(1.0);
            let ry = (node.height * 72.0 / 2.0).max(1.0);
            min_x = min_x.min(node.pos.x - rx);
            min_y = min_y.min(node.pos.y - ry);
            max_x = max_x.max(node.pos.x + rx);
            max_y = max_y.max(node.pos.y + ry);
        }
    }
    for &cid in &subg.subgraphs {
        if let Some(child) = graph.subgraphs.get(cid.0 as usize) {
            let cbb = effective_cluster_bb(graph, child);
            if cbb.width() > 0.0 && cbb.height() > 0.0 {
                min_x = min_x.min(cbb.ll.x);
                min_y = min_y.min(cbb.ll.y);
                max_x = max_x.max(cbb.ur.x);
                max_y = max_y.max(cbb.ur.y);
            }
        }
    }

    if !min_x.is_finite() || !min_y.is_finite() {
        return BoundingBox::new(8.0, 8.0, 62.0, 44.0);
    }
    let margin = 8.0;
    let mut ur_y = max_y + margin;
    if let Some(lbl) = subg.graph_attrs.get("label")
        && !lbl.is_empty()
    {
        let fs = parse_f64_attr(&subg.graph_attrs, "fontsize", DEFAULT_FONT_SIZE);
        ur_y += fs * LINESPACING + 4.0;
    }
    BoundingBox::new(min_x - margin, min_y - margin, max_x + margin, ur_y)
}

fn parse_pad_pt(attrs: &AttrMap) -> (f64, f64) {
    let Some(pad_str) = attrs.get("pad") else {
        return (DEFAULT_PAD_PT, DEFAULT_PAD_PT);
    };
    let mut parts = pad_str
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .filter(|s| !s.is_empty());
    let Some(px_in) = parts.next().and_then(|s| s.parse::<f64>().ok()) else {
        return (DEFAULT_PAD_PT, DEFAULT_PAD_PT);
    };
    let py_in = parts
        .next()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(px_in);
    ((px_in * 72.0).max(0.0), (py_in * 72.0).max(0.0))
}

pub(crate) fn parse_f64_attr(attrs: &AttrMap, key: &str, default: f64) -> f64 {
    attrs
        .get(key)
        .and_then(|v| gv_geom::shapes::parse_f64_strip_units(v))
        .filter(|v| v.is_finite())
        .unwrap_or(default)
}

fn first_color_token(spec: &str) -> &str {
    spec.split(':').next().unwrap_or(spec).trim()
}

fn format_paint_and_opacity(
    spec: Option<&str>,
    default_name: &str,
    colorscheme: Option<&str>,
) -> (String, Option<f64>) {
    let raw = spec
        .map(first_color_token)
        .filter(|s| !s.is_empty())
        .unwrap_or(default_name);
    // `gvrender_resolve_color`: names the SVG renderer knows are written through unchanged,
    // whatever the colorscheme; anything else goes through `colorxlate`.
    if is_svg_known_color(raw) {
        if raw.eq_ignore_ascii_case("transparent") {
            return ("none".to_string(), None);
        }
        return (raw.to_string(), None);
    }
    match resolve_color_with_scheme(raw, colorscheme) {
        Some(rgba) if rgba.is_transparent() => ("none".to_string(), None),
        Some(rgba) => {
            let opacity = if rgba.a < 255 {
                Some(f64::from(rgba.a) / 255.0)
            } else {
                None
            };
            (rgba.to_hex_rgb(), opacity)
        }
        None => {
            gv_core::warn_unknown_color(raw);
            ("#000000".to_string(), None)
        }
    }
}

/// Checks if a color name is in `svg_knowncolors` in `gvrender_core_svg.c`.
fn is_svg_known_color(name: &str) -> bool {
    const SVG_KNOWN_COLORS: &[&str] = &[
        "aliceblue",
        "antiquewhite",
        "aqua",
        "aquamarine",
        "azure",
        "beige",
        "bisque",
        "black",
        "blanchedalmond",
        "blue",
        "blueviolet",
        "brown",
        "burlywood",
        "cadetblue",
        "chartreuse",
        "chocolate",
        "coral",
        "cornflowerblue",
        "cornsilk",
        "crimson",
        "cyan",
        "darkblue",
        "darkcyan",
        "darkgoldenrod",
        "darkgray",
        "darkgreen",
        "darkgrey",
        "darkkhaki",
        "darkmagenta",
        "darkolivegreen",
        "darkorange",
        "darkorchid",
        "darkred",
        "darksalmon",
        "darkseagreen",
        "darkslateblue",
        "darkslategray",
        "darkslategrey",
        "darkturquoise",
        "darkviolet",
        "deeppink",
        "deepskyblue",
        "dimgray",
        "dimgrey",
        "dodgerblue",
        "firebrick",
        "floralwhite",
        "forestgreen",
        "fuchsia",
        "gainsboro",
        "ghostwhite",
        "gold",
        "goldenrod",
        "gray",
        "green",
        "greenyellow",
        "grey",
        "honeydew",
        "hotpink",
        "indianred",
        "indigo",
        "ivory",
        "khaki",
        "lavender",
        "lavenderblush",
        "lawngreen",
        "lemonchiffon",
        "lightblue",
        "lightcoral",
        "lightcyan",
        "lightgoldenrodyellow",
        "lightgray",
        "lightgreen",
        "lightgrey",
        "lightpink",
        "lightsalmon",
        "lightseagreen",
        "lightskyblue",
        "lightslategray",
        "lightslategrey",
        "lightsteelblue",
        "lightyellow",
        "lime",
        "limegreen",
        "linen",
        "magenta",
        "maroon",
        "mediumaquamarine",
        "mediumblue",
        "mediumorchid",
        "mediumpurple",
        "mediumseagreen",
        "mediumslateblue",
        "mediumspringgreen",
        "mediumturquoise",
        "mediumvioletred",
        "midnightblue",
        "mintcream",
        "mistyrose",
        "moccasin",
        "navajowhite",
        "navy",
        "oldlace",
        "olive",
        "olivedrab",
        "orange",
        "orangered",
        "orchid",
        "palegoldenrod",
        "palegreen",
        "paleturquoise",
        "palevioletred",
        "papayawhip",
        "peachpuff",
        "peru",
        "pink",
        "plum",
        "powderblue",
        "purple",
        "red",
        "rosybrown",
        "royalblue",
        "saddlebrown",
        "salmon",
        "sandybrown",
        "seagreen",
        "seashell",
        "sienna",
        "silver",
        "skyblue",
        "slateblue",
        "slategray",
        "slategrey",
        "snow",
        "springgreen",
        "steelblue",
        "tan",
        "teal",
        "thistle",
        "tomato",
        "transparent",
        "turquoise",
        "violet",
        "wheat",
        "white",
        "whitesmoke",
        "yellow",
        "yellowgreen",
    ];
    let lower = name.trim().to_ascii_lowercase();
    SVG_KNOWN_COLORS.binary_search(&lower.as_str()).is_ok()
}

#[derive(Debug, Clone)]
struct GraphicStyle {
    fill: String,
    fill_opacity: Option<f64>,
    stroke: String,
    stroke_opacity: Option<f64>,
    penwidth: f64,
    dashed: bool,
    dotted: bool,
}

impl GraphicStyle {
    fn write_svg_attrs(&self, out: &mut String, filled: bool) {
        out.push_str(" fill=\"");
        if filled {
            out.push_str(&self.fill);
            if let Some(op) = self.fill_opacity {
                let _ = write!(out, "\" fill-opacity=\"{op:.6}");
            }
        } else {
            out.push_str("none");
        }
        out.push_str("\" stroke=\"");
        out.push_str(&self.stroke);
        if (self.penwidth - PENWIDTH_NORMAL).abs() >= GVPRINT_DOUBLE_THRESHOLD {
            out.push_str("\" stroke-width=\"");
            out.push_str(&fmt_gv_double(self.penwidth));
        }
        if self.dashed {
            let _ = write!(out, "\" stroke-dasharray=\"{SDASHARRAY}");
        } else if self.dotted {
            let _ = write!(out, "\" stroke-dasharray=\"{SDOTARRAY}");
        }
        if let Some(op) = self.stroke_opacity {
            let _ = write!(out, "\" stroke-opacity=\"{op:.6}");
        }
        out.push('"');
    }
}

/// Renders a [`Graph`] into an authentic Graphviz SVG document (`-Tsvg`).
pub fn render_svg(graph: &Graph) -> String {
    let mut out = String::with_capacity(2048);

    // 1. Standalone XML & DOCTYPE headers (matching `svg_begin_job`)
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n");
    if let Some(stylesheet) = graph.graph_attrs.get("stylesheet")
        && !stylesheet.is_empty()
    {
        out.push_str("<?xml-stylesheet href=\"");
        out.push_str(&xml_escape(stylesheet, false, false, false));
        out.push_str("\" type=\"text/css\"?>\n");
    }
    out.push_str(
        "<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\"\n \
         \"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\">\n",
    );
    out.push_str("<!-- Generated by graphvis-rs 0.1.0 (Graphviz 16.1.0 compatible)\n -->\n");

    // 2. Graph comment & <svg> root element (matching `svg_begin_graph`)
    out.push_str("<!--");
    if !graph.name.is_empty() && !graph.name.starts_with('%') {
        out.push_str(" Title: ");
        out.push_str(&xml_escape(&graph.name, true, true, false));
    }
    out.push_str(" Pages: 1 -->\n");

    let bb = effective_graph_bb(graph);
    let (pad_x, pad_y) = parse_pad_pt(&graph.graph_attrs);
    let clip_ll = Point::new(bb.ll.x - pad_x, bb.ll.y - pad_y);
    let clip_ur = Point::new(bb.ur.x + pad_x, bb.ur.y + pad_y);

    let sz_x = clip_ur.x - clip_ll.x;
    let sz_y = clip_ur.y - clip_ll.y;

    let rotation = if graph
        .graph_attrs
        .get("rotate")
        .and_then(|s| s.trim().parse::<i32>().ok())
        == Some(90)
        || graph.graph_attrs.get("orientation").is_some_and(|s| {
            s.trim()
                .as_bytes()
                .first()
                .is_some_and(|b| b.eq_ignore_ascii_case(&b'l'))
        })
        || gv_geom::shapes::parse_graphviz_bool(
            graph.graph_attrs.get("landscape").map(String::as_str),
            false,
        ) {
        90
    } else {
        0
    };

    let mut zoom = 1.0_f64;
    if let Some(raw_size) = graph.graph_attrs.get("size") {
        let trimmed = raw_size.trim();
        let is_bang = trimmed.ends_with('!');
        let clean = trimmed.trim_end_matches('!');
        let mut parts = clean.split(',').map(str::trim);
        if let (Some(Ok(w_in)), Some(Ok(h_in))) = (
            parts.next().map(str::parse::<f64>),
            parts.next().map(str::parse::<f64>),
        ) {
            let size_x = w_in * 72.0;
            let size_y = h_in * 72.0;
            let is_filled = is_bang
                || graph
                    .graph_attrs
                    .get("ratio")
                    .is_some_and(|r| r.trim().eq_ignore_ascii_case("fill"));
            if size_x > 0.001 && size_y > 0.001 && (size_x < sz_x || size_y < sz_y || is_filled) {
                zoom = (size_x / sz_x).min(size_y / sz_y);
            }
        }
    }

    let view_x = zoom * sz_x;
    let view_y = zoom * sz_y;
    let (width_pt, height_pt) = if rotation == 90 {
        (
            view_y.round().max(1.0) as i64,
            view_x.round().max(1.0) as i64,
        )
    } else {
        (
            view_x.round().max(1.0) as i64,
            view_y.round().max(1.0) as i64,
        )
    };

    let _ = writeln!(out, "<svg width=\"{width_pt}pt\" height=\"{height_pt}pt\"");
    let _ = write!(
        out,
        " viewBox=\"0.00 0.00 {width_pt}.00 {height_pt}.00\" \
         xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\""
    );
    if let Some(svgid) = graph.graph_attrs.get("svgid")
        && !svgid.is_empty()
    {
        out.push_str(" id=\"");
        out.push_str(&xml_escape(svgid, true, true, false));
        out.push('"');
    }
    out.push_str(">\n");

    // 3. Page group `<g id="graph0" class="graph" transform="...">` (matching `svg_begin_page`)
    let (tx, ty, rot_deg) = if rotation == 90 {
        (-clip_ur.x, clip_ur.y, -90)
    } else {
        (-clip_ll.x, clip_ur.y, 0)
    };
    let zoom_str = if (zoom - 1.0).abs() < 1e-6 {
        "1".to_string()
    } else {
        let s = format!("{zoom:.6}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    let graph_id = graph
        .graph_attrs
        .get("id")
        .filter(|s| !s.is_empty())
        .map(|s| xml_escape(s, true, true, false))
        .unwrap_or_else(|| "graph0".to_string());
    let mut graph_class = "graph".to_string();
    if let Some(cls) = graph.graph_attrs.get("class")
        && !cls.is_empty()
    {
        graph_class.push(' ');
        graph_class.push_str(&xml_escape(cls, true, true, false));
    }
    let _ = writeln!(
        out,
        "<g id=\"{graph_id}\" class=\"{graph_class}\" transform=\"scale({zoom_str} {zoom_str}) rotate({rot_deg}) translate({} {})\">",
        fmt_gv_double(tx),
        fmt_gv_double(ty)
    );
    if !graph.name.is_empty() && !graph.name.starts_with('%') {
        let _ = writeln!(
            out,
            "<title>{}</title>",
            xml_escape(&graph.name, true, true, false)
        );
    }

    // 4. Background polygon (matching `emit_background`)
    let root_scheme = graph.graph_attrs.get("colorscheme").map(String::as_str);
    let bgcolor = graph.graph_attrs.get("bgcolor").map(String::as_str);
    if !matches!(bgcolor, Some(s) if s.eq_ignore_ascii_case("transparent") || s.eq_ignore_ascii_case("none"))
    {
        let (bg_fill, bg_opacity) = format_paint_and_opacity(bgcolor, "white", root_scheme);
        let bg_style = GraphicStyle {
            fill: bg_fill,
            fill_opacity: bg_opacity,
            stroke: "none".to_string(),
            stroke_opacity: None,
            penwidth: PENWIDTH_NORMAL,
            dashed: false,
            dotted: false,
        };
        let bg_pts = [
            Point::new(clip_ll.x, clip_ll.y),
            Point::new(clip_ll.x, clip_ur.y),
            Point::new(clip_ur.x, clip_ur.y),
            Point::new(clip_ur.x, clip_ll.y),
        ];
        svg_polygon(&mut out, &bg_pts, &bg_style, true);
    }

    // 5. Graph label (if any)
    if let Some(g_label) = graph.graph_attrs.get("label")
        && !g_label.is_empty()
    {
        let font_name = graph
            .graph_attrs
            .get("fontname")
            .map(String::as_str)
            .unwrap_or(DEFAULT_FONT_NAME);
        let font_size = parse_f64_attr(&graph.graph_attrs, "fontsize", DEFAULT_FONT_SIZE);
        let font_color = graph
            .graph_attrs
            .get("fontcolor")
            .map(String::as_str)
            .unwrap_or("black");
        let labelloc = graph
            .graph_attrs
            .get("labelloc")
            .map(String::as_str)
            .unwrap_or("b");
        let ctx = LabelSubstContext {
            graph_name: &graph.name,
            label_text: g_label,
            is_directed: graph.kind == GraphKind::Directed,
            ..Default::default()
        };
        let layout = measure_multiline_label_with_context(g_label, &ctx, font_name, font_size);
        let lp_x = (bb.ll.x + bb.ur.x) / 2.0;
        let lp_y = if labelloc.starts_with('t') {
            bb.ur.y - layout.height / 2.0 - 4.0
        } else {
            bb.ll.y + layout.height / 2.0 + 4.0
        };
        emit_text_label(
            &mut out,
            &layout,
            Point::new(lp_x, lp_y),
            font_color,
            root_scheme,
            false,
            false,
            false,
        );
    }

    // 6. Clusters (`emit_clusters`)
    let mut clust_counter = 1usize;
    for subg in &graph.subgraphs {
        let is_clust = subg.is_cluster || subg.name.starts_with("cluster");
        if !is_clust {
            continue;
        }
        emit_cluster(&mut out, graph, subg, &mut clust_counter, root_scheme);
    }

    // 7. Nodes and Edges (`emit_view`)
    let edges_first = graph
        .graph_attrs
        .get("outputorder")
        .is_some_and(|v| v.eq_ignore_ascii_case("edgesfirst"));

    if edges_first {
        for (idx, edge) in graph.edges.iter().enumerate() {
            emit_edge(&mut out, graph, edge, idx + 1, root_scheme);
        }
        for (idx, node) in graph.nodes.iter().enumerate() {
            emit_node(&mut out, graph, node, idx + 1, root_scheme);
        }
    } else {
        for (idx, node) in graph.nodes.iter().enumerate() {
            emit_node(&mut out, graph, node, idx + 1, root_scheme);
        }
        for (idx, edge) in graph.edges.iter().enumerate() {
            emit_edge(&mut out, graph, edge, idx + 1, root_scheme);
        }
    }

    out.push_str("</g>\n</svg>\n");
    out
}

fn emit_cluster(
    out: &mut String,
    graph: &Graph,
    subg: &SubgraphData,
    clust_counter: &mut usize,
    parent_scheme: Option<&str>,
) {
    let style = parse_style(subg.graph_attrs.get("style").map(String::as_str));
    if style.invis {
        return;
    }

    let clust_idx = *clust_counter;
    *clust_counter += 1;

    let scheme = subg
        .graph_attrs
        .get("colorscheme")
        .map(String::as_str)
        .or(parent_scheme);

    if !subg.name.is_empty() {
        let _ = writeln!(
            out,
            "<!-- {} -->",
            xml_escape(&subg.name, true, true, false)
        );
    }

    let clust_id = subg
        .graph_attrs
        .get("id")
        .filter(|s| !s.is_empty())
        .map(|s| xml_escape(s, true, true, false))
        .unwrap_or_else(|| format!("clust{clust_idx}"));
    let mut clust_class = "cluster".to_string();
    if let Some(cls) = subg.graph_attrs.get("class")
        && !cls.is_empty()
    {
        clust_class.push(' ');
        clust_class.push_str(&xml_escape(cls, true, true, false));
    }
    let _ = writeln!(out, "<g id=\"{clust_id}\" class=\"{clust_class}\">");
    let _ = writeln!(
        out,
        "<title>{}</title>",
        xml_escape(&subg.name, true, true, false)
    );

    let cbb = effective_cluster_bb(graph, subg);
    let peripheries = subg
        .graph_attrs
        .get("peripheries")
        .and_then(|s| s.trim().parse::<usize>().ok())
        .unwrap_or(1);

    let stroke_spec = subg
        .graph_attrs
        .get("pencolor")
        .or_else(|| subg.graph_attrs.get("color"))
        .map(String::as_str);
    let fill_spec = subg
        .graph_attrs
        .get("fillcolor")
        .or_else(|| subg.graph_attrs.get("bgcolor"))
        .map(String::as_str)
        .or(stroke_spec);

    let is_filled = style.filled || subg.graph_attrs.contains_key("bgcolor");
    let default_fill = if style.filled { "lightgrey" } else { "none" };
    let (fill, fill_opacity) = format_paint_and_opacity(fill_spec, default_fill, scheme);
    let (stroke, stroke_opacity) = if peripheries == 0 {
        ("none".to_string(), None)
    } else {
        format_paint_and_opacity(stroke_spec, "black", scheme)
    };

    let mut penwidth = parse_f64_attr(&subg.graph_attrs, "penwidth", PENWIDTH_NORMAL);
    if let Some(lw) = style.linewidth {
        penwidth = lw;
    } else if style.bold && (penwidth - PENWIDTH_NORMAL).abs() < 1e-6 {
        penwidth = 2.0;
    }

    let gstyle = GraphicStyle {
        fill,
        fill_opacity,
        stroke,
        stroke_opacity,
        penwidth,
        dashed: style.dashed,
        dotted: style.dotted,
    };

    if peripheries > 0 || is_filled {
        let count = peripheries.max(1);
        for p in 0..count {
            let inset = (p as f64) * PERIPHERY_GAP_PT;
            let b = BoundingBox::new(
                cbb.ll.x + inset,
                cbb.ll.y + inset,
                (cbb.ur.x - inset).max(cbb.ll.x + inset + 1.0),
                (cbb.ur.y - inset).max(cbb.ll.y + inset + 1.0),
            );
            let pts = [
                Point::new(b.ll.x, b.ur.y),
                Point::new(b.ll.x, b.ll.y),
                Point::new(b.ur.x, b.ll.y),
                Point::new(b.ur.x, b.ur.y),
            ];
            let fill_this = is_filled && p == 0;
            if style.rounded {
                svg_rounded_box(out, b, &gstyle, fill_this);
            } else {
                svg_polygon(out, &pts, &gstyle, fill_this);
            }
        }
    }

    if let Some(label) = subg.graph_attrs.get("label")
        && !label.is_empty()
    {
        let font_name = subg
            .graph_attrs
            .get("fontname")
            .or_else(|| graph.graph_attrs.get("fontname"))
            .map(String::as_str)
            .unwrap_or(DEFAULT_FONT_NAME);
        let font_size = parse_f64_attr(
            &subg.graph_attrs,
            "fontsize",
            parse_f64_attr(&graph.graph_attrs, "fontsize", DEFAULT_FONT_SIZE),
        );
        let font_color = subg
            .graph_attrs
            .get("fontcolor")
            .or_else(|| graph.graph_attrs.get("fontcolor"))
            .map(String::as_str)
            .unwrap_or("black");
        let labelloc = subg
            .graph_attrs
            .get("labelloc")
            .map(String::as_str)
            .unwrap_or("t");

        if is_html_string(label) {
            let h_bb = BoundingBox::new(
                cbb.ll.x + 4.0,
                cbb.ur.y - (font_size * 2.0).min(cbb.height()),
                cbb.ur.x - 4.0,
                cbb.ur.y - 4.0,
            );
            emit_html_label(
                out,
                label,
                h_bb,
                font_name,
                font_size,
                font_color,
                &gstyle.stroke,
                scheme,
            );
        } else {
            let ctx = LabelSubstContext {
                graph_name: &subg.name,
                label_text: label,
                is_directed: graph.kind == GraphKind::Directed,
                ..Default::default()
            };
            let layout = measure_multiline_label_with_context(label, &ctx, font_name, font_size);
            let lp_x = (cbb.ll.x + cbb.ur.x) / 2.0;
            let lp_y = if labelloc.starts_with('b') {
                cbb.ll.y + layout.height / 2.0 + 4.0
            } else {
                cbb.ur.y - layout.height / 2.0 - 4.0
            };
            emit_text_label(
                out,
                &layout,
                Point::new(lp_x, lp_y),
                font_color,
                scheme,
                style.bold,
                false,
                false,
            );
        }
    }

    out.push_str("</g>\n");
}

fn emit_node(
    out: &mut String,
    graph: &Graph,
    node: &NodeData,
    node_idx: usize,
    parent_scheme: Option<&str>,
) {
    let style = parse_style(node.attrs.get("style").map(String::as_str));
    if style.invis {
        return;
    }

    let scheme = node
        .attrs
        .get("colorscheme")
        .map(String::as_str)
        .or(parent_scheme);

    let _ = writeln!(
        out,
        "<!-- {} -->",
        xml_escape(&node.name, true, true, false)
    );
    if let Some(comment) = node.attrs.get("comment")
        && !comment.is_empty()
    {
        let _ = writeln!(out, "<!-- {} -->", xml_escape(comment, true, true, false));
    }

    let node_id = node
        .attrs
        .get("id")
        .filter(|s| !s.is_empty())
        .map(|s| xml_escape(s, true, true, false))
        .unwrap_or_else(|| format!("node{node_idx}"));
    let mut node_class = "node".to_string();
    if let Some(cls) = node.attrs.get("class")
        && !cls.is_empty()
    {
        node_class.push(' ');
        node_class.push_str(&xml_escape(cls, true, true, false));
    }
    let _ = writeln!(out, "<g id=\"{node_id}\" class=\"{node_class}\">");
    let _ = writeln!(
        out,
        "<title>{}</title>",
        xml_escape(&node.name, true, true, false)
    );

    let href = node
        .attrs
        .get("href")
        .or_else(|| node.attrs.get("URL"))
        .filter(|s| !s.is_empty());
    let tooltip = node.attrs.get("tooltip").filter(|s| !s.is_empty());
    let target = node.attrs.get("target").filter(|s| !s.is_empty());
    let has_anchor = href.is_some() || tooltip.is_some();

    let subst_ctx = LabelSubstContext {
        node_name: &node.name,
        graph_name: &graph.name,
        is_directed: graph.kind == GraphKind::Directed,
        label_text: node.attrs.get("label").map(String::as_str).unwrap_or("\\N"),
        ..Default::default()
    };

    if has_anchor {
        let resolved_href = href.map(|h| substitute_object_escapes(h, &subst_ctx));
        let resolved_tip = tooltip.map(|t| substitute_object_escapes(t, &subst_ctx));
        let resolved_target = target.map(|t| substitute_object_escapes(t, &subst_ctx));
        svg_begin_anchor(
            out,
            Some(&node_id),
            resolved_href.as_deref(),
            resolved_tip.as_deref(),
            resolved_target.as_deref(),
        );
    }

    let shape = node
        .attrs
        .get("shape")
        .map(String::as_str)
        .unwrap_or("ellipse")
        .trim();
    let shape_lower = shape.to_ascii_lowercase();

    let rx = (node.width * 72.0 / 2.0).max(0.5);
    let ry = (node.height * 72.0 / 2.0).max(0.5);
    let center = node.pos;
    let node_bb = BoundingBox::new(center.x - rx, center.y - ry, center.x + rx, center.y + ry);

    let stroke_spec = node
        .attrs
        .get("pencolor")
        .or_else(|| node.attrs.get("color"))
        .map(String::as_str);
    let fill_spec = node
        .attrs
        .get("fillcolor")
        .map(String::as_str)
        .or(stroke_spec);

    let is_point_shape = shape_lower == "point";
    let is_filled = style.filled || is_point_shape;
    let default_fill = if is_point_shape {
        "black"
    } else if is_filled {
        "lightgrey"
    } else {
        "none"
    };
    let (fill, fill_opacity) = format_paint_and_opacity(fill_spec, default_fill, scheme);
    let (stroke, stroke_opacity) = format_paint_and_opacity(stroke_spec, "black", scheme);

    let mut penwidth = parse_f64_attr(&node.attrs, "penwidth", PENWIDTH_NORMAL);
    if let Some(lw) = style.linewidth {
        penwidth = lw;
    } else if style.bold && (penwidth - PENWIDTH_NORMAL).abs() < 1e-6 {
        penwidth = 2.0;
    }

    let gstyle = GraphicStyle {
        fill,
        fill_opacity,
        stroke,
        stroke_opacity,
        penwidth,
        dashed: style.dashed,
        dotted: style.dotted,
    };

    let font_name = node
        .attrs
        .get("fontname")
        .map(String::as_str)
        .unwrap_or(DEFAULT_FONT_NAME);
    let font_size = parse_f64_attr(&node.attrs, "fontsize", DEFAULT_FONT_SIZE);
    let font_color = node
        .attrs
        .get("fontcolor")
        .map(String::as_str)
        .unwrap_or("black");
    let raw_label = node.attrs.get("label").map(String::as_str).unwrap_or("\\N");

    if shape_lower == "record" || shape_lower == "mrecord" {
        let is_mrecord = shape_lower == "mrecord" || style.rounded;
        emit_record_node(
            out, graph, node, node_bb, &gstyle, is_filled, is_mrecord, raw_label, &subst_ctx,
            font_name, font_size, font_color, scheme,
        );
    } else if is_point_shape {
        let pt_r = rx.min(ry).max(1.8);
        svg_ellipse(out, center, pt_r, pt_r, &gstyle, true);
    } else {
        let is_plain_shape = matches!(shape_lower.as_str(), "none" | "plaintext" | "plain");
        if !is_plain_shape || is_filled {
            emit_node_shape(
                out,
                &shape_lower,
                node,
                center,
                rx,
                ry,
                node_bb,
                &gstyle,
                &style,
                is_filled,
            );
        }

        if !raw_label.is_empty() {
            if is_html_string(raw_label) {
                emit_html_label(
                    out,
                    raw_label,
                    node_bb,
                    font_name,
                    font_size,
                    font_color,
                    &gstyle.stroke,
                    scheme,
                );
            } else {
                let layout = measure_multiline_label_with_context(
                    raw_label, &subst_ctx, font_name, font_size,
                );
                let labelloc = node
                    .attrs
                    .get("labelloc")
                    .map(String::as_str)
                    .unwrap_or("c");
                let label_pos = match labelloc.as_bytes().first().copied() {
                    Some(b't') => Point::new(center.x, node_bb.ur.y - layout.height / 2.0 - 2.0),
                    Some(b'b') => Point::new(center.x, node_bb.ll.y + layout.height / 2.0 + 2.0),
                    _ => center,
                };
                emit_text_label(
                    out, &layout, label_pos, font_color, scheme, style.bold, false, false,
                );
            }
        }
    }

    // External node label (`xlabel`) if present
    if let Some(xlabel) = node.attrs.get("xlabel")
        && !xlabel.is_empty()
    {
        let layout = measure_multiline_label_with_context(xlabel, &subst_ctx, font_name, font_size);
        let xlp = Point::new(
            node_bb.ll.x - layout.width / 2.0 - 2.0,
            node_bb.ur.y + layout.height / 2.0,
        );
        emit_text_label(
            out, &layout, xlp, font_color, scheme, style.bold, false, false,
        );
    }

    if has_anchor {
        svg_end_anchor(out);
    }

    out.push_str("</g>\n");
}

#[allow(clippy::too_many_arguments)]
fn emit_node_shape(
    out: &mut String,
    shape_lower: &str,
    node: &NodeData,
    center: Point,
    rx: f64,
    ry: f64,
    node_bb: BoundingBox,
    gstyle: &GraphicStyle,
    style: &ParsedStyle,
    is_filled: bool,
) {
    let default_peripheries = match shape_lower {
        "doublecircle" | "doubleoctagon" => 2,
        "tripleoctagon" => 3,
        "none" | "plaintext" | "plain" => usize::from(is_filled),
        _ => 1,
    };
    let peripheries = node
        .attrs
        .get("peripheries")
        .and_then(|s| s.trim().parse::<usize>().ok())
        .unwrap_or(default_peripheries);
    if peripheries == 0 && !is_filled {
        return;
    }
    let count = peripheries.max(1);

    match shape_lower {
        "ellipse" | "oval" | "circle" | "doublecircle" | "mcircle" => {
            let is_circle = matches!(shape_lower, "circle" | "doublecircle" | "mcircle");
            let base_rx = if is_circle { rx.max(ry) } else { rx };
            let base_ry = if is_circle { rx.max(ry) } else { ry };
            for p in 0..count {
                let erx = (base_rx - (count - 1 - p) as f64 * PERIPHERY_GAP_PT).max(1.0);
                let ery = (base_ry - (count - 1 - p) as f64 * PERIPHERY_GAP_PT).max(1.0);
                svg_ellipse(out, center, erx, ery, gstyle, is_filled && p == 0);
            }
            if shape_lower == "mcircle" || style.diagonals {
                let dy = base_ry * std::f64::consts::FRAC_1_SQRT_2;
                let dx = base_rx * std::f64::consts::FRAC_1_SQRT_2;
                svg_polyline(
                    out,
                    &[
                        Point::new(center.x - dx, center.y + dy),
                        Point::new(center.x + dx, center.y + dy),
                    ],
                    gstyle,
                );
                svg_polyline(
                    out,
                    &[
                        Point::new(center.x - dx, center.y - dy),
                        Point::new(center.x + dx, center.y - dy),
                    ],
                    gstyle,
                );
            }
        }
        "box" | "rect" | "rectangle" | "square" | "msquare" | "none" | "plaintext" | "plain" => {
            let is_sq = matches!(shape_lower, "square" | "msquare");
            let brx = if is_sq { rx.max(ry) } else { rx };
            let bry = if is_sq { rx.max(ry) } else { ry };
            for p in 0..count {
                let inset = (count - 1 - p) as f64 * PERIPHERY_GAP_PT;
                let b = BoundingBox::new(
                    center.x - (brx - inset).max(1.0),
                    center.y - (bry - inset).max(1.0),
                    center.x + (brx - inset).max(1.0),
                    center.y + (bry - inset).max(1.0),
                );
                let fill_this = is_filled && p == 0;
                if style.rounded {
                    svg_rounded_box(out, b, gstyle, fill_this);
                } else {
                    let pts = [
                        Point::new(b.ur.x, b.ur.y),
                        Point::new(b.ll.x, b.ur.y),
                        Point::new(b.ll.x, b.ll.y),
                        Point::new(b.ur.x, b.ll.y),
                    ];
                    svg_polygon(out, &pts, gstyle, fill_this);
                }
            }
            if shape_lower == "msquare" || style.diagonals {
                let d = brx.min(bry) * 0.35;
                let b = BoundingBox::new(
                    center.x - brx,
                    center.y - bry,
                    center.x + brx,
                    center.y + bry,
                );
                svg_polyline(
                    out,
                    &[
                        Point::new(b.ll.x, b.ur.y - d),
                        Point::new(b.ll.x + d, b.ur.y),
                    ],
                    gstyle,
                );
                svg_polyline(
                    out,
                    &[
                        Point::new(b.ur.x - d, b.ur.y),
                        Point::new(b.ur.x, b.ur.y - d),
                    ],
                    gstyle,
                );
                svg_polyline(
                    out,
                    &[
                        Point::new(b.ll.x, b.ll.y + d),
                        Point::new(b.ll.x + d, b.ll.y),
                    ],
                    gstyle,
                );
                svg_polyline(
                    out,
                    &[
                        Point::new(b.ur.x - d, b.ll.y),
                        Point::new(b.ur.x, b.ll.y + d),
                    ],
                    gstyle,
                );
            }
        }
        "diamond" | "mdiamond" => {
            let pts = [
                Point::new(center.x, center.y + ry),
                Point::new(center.x - rx, center.y),
                Point::new(center.x, center.y - ry),
                Point::new(center.x + rx, center.y),
            ];
            svg_polygon(out, &pts, gstyle, is_filled);
            if shape_lower == "mdiamond" || style.diagonals {
                let frac = 0.25;
                svg_polyline(
                    out,
                    &[
                        Point::new(center.x - rx * frac, center.y + ry * (1.0 - frac)),
                        Point::new(center.x + rx * frac, center.y + ry * (1.0 - frac)),
                    ],
                    gstyle,
                );
                svg_polyline(
                    out,
                    &[
                        Point::new(center.x - rx * frac, center.y - ry * (1.0 - frac)),
                        Point::new(center.x + rx * frac, center.y - ry * (1.0 - frac)),
                    ],
                    gstyle,
                );
                svg_polyline(
                    out,
                    &[
                        Point::new(center.x - rx * (1.0 - frac), center.y + ry * frac),
                        Point::new(center.x - rx * (1.0 - frac), center.y - ry * frac),
                    ],
                    gstyle,
                );
                svg_polyline(
                    out,
                    &[
                        Point::new(center.x + rx * (1.0 - frac), center.y + ry * frac),
                        Point::new(center.x + rx * (1.0 - frac), center.y - ry * frac),
                    ],
                    gstyle,
                );
            }
        }
        "triangle" | "invtriangle" => {
            let inv = shape_lower == "invtriangle";
            let pts = if inv {
                [
                    Point::new(center.x, center.y - ry),
                    Point::new(center.x + rx, center.y + ry * 0.5),
                    Point::new(center.x - rx, center.y + ry * 0.5),
                ]
            } else {
                [
                    Point::new(center.x, center.y + ry),
                    Point::new(center.x - rx, center.y - ry * 0.5),
                    Point::new(center.x + rx, center.y - ry * 0.5),
                ]
            };
            svg_polygon(out, &pts, gstyle, is_filled);
        }
        "pentagon" | "hexagon" | "septagon" | "octagon" | "doubleoctagon" | "tripleoctagon"
        | "polygon" => {
            let sides = match shape_lower {
                "pentagon" => 5,
                "hexagon" => 6,
                "septagon" => 7,
                "octagon" | "doubleoctagon" | "tripleoctagon" => 8,
                _ => node
                    .attrs
                    .get("sides")
                    .and_then(|s| s.trim().parse::<usize>().ok())
                    .unwrap_or(4)
                    .max(3),
            };
            let orientation_deg = parse_f64_attr(&node.attrs, "orientation", 0.0);
            let orientation_rad = orientation_deg.to_radians();
            for p in 0..count {
                let inset = (count - 1 - p) as f64 * PERIPHERY_GAP_PT;
                let prx = (rx - inset).max(1.0);
                let pry = (ry - inset).max(1.0);
                let pts = regular_polygon_vertices(center, prx, pry, sides, orientation_rad);
                svg_polygon(out, &pts, gstyle, is_filled && p == 0);
            }
        }
        "trapezium" | "invtrapezium" => {
            let inv = shape_lower == "invtrapezium";
            let top_rx = if inv { rx } else { rx * 0.65 };
            let bot_rx = if inv { rx * 0.65 } else { rx };
            let pts = [
                Point::new(center.x + top_rx, center.y + ry),
                Point::new(center.x - top_rx, center.y + ry),
                Point::new(center.x - bot_rx, center.y - ry),
                Point::new(center.x + bot_rx, center.y - ry),
            ];
            svg_polygon(out, &pts, gstyle, is_filled);
        }
        "parallelogram" => {
            let dx = rx * 0.3;
            let pts = [
                Point::new(center.x + rx, center.y + ry),
                Point::new(center.x - rx + dx, center.y + ry),
                Point::new(center.x - rx, center.y - ry),
                Point::new(center.x + rx - dx, center.y - ry),
            ];
            svg_polygon(out, &pts, gstyle, is_filled);
        }
        "house" | "invhouse" => {
            let inv = shape_lower == "invhouse";
            let pts = if inv {
                [
                    Point::new(center.x, center.y - ry),
                    Point::new(center.x + rx, center.y - ry * 0.15),
                    Point::new(center.x + rx * 0.85, center.y + ry),
                    Point::new(center.x - rx * 0.85, center.y + ry),
                    Point::new(center.x - rx, center.y - ry * 0.15),
                ]
            } else {
                [
                    Point::new(center.x + rx, center.y + ry * 0.15),
                    Point::new(center.x, center.y + ry),
                    Point::new(center.x - rx, center.y + ry * 0.15),
                    Point::new(center.x - rx * 0.85, center.y - ry),
                    Point::new(center.x + rx * 0.85, center.y - ry),
                ]
            };
            svg_polygon(out, &pts, gstyle, is_filled);
        }
        "star" => {
            let mut pts = Vec::with_capacity(10);
            for i in 0..10 {
                let angle = PI / 2.0 + (i as f64) * PI / 5.0;
                let r_scale = if i % 2 == 0 { 1.0 } else { 0.4 };
                pts.push(Point::new(
                    center.x + rx * r_scale * angle.cos(),
                    center.y + ry * r_scale * angle.sin(),
                ));
            }
            svg_polygon(out, &pts, gstyle, is_filled);
        }
        "note" => {
            let fold = 6.0_f64.min(rx * 0.4).min(ry * 0.4);
            let pts = [
                Point::new(node_bb.ur.x - fold, node_bb.ur.y),
                Point::new(node_bb.ll.x, node_bb.ur.y),
                Point::new(node_bb.ll.x, node_bb.ll.y),
                Point::new(node_bb.ur.x, node_bb.ll.y),
                Point::new(node_bb.ur.x, node_bb.ur.y - fold),
            ];
            svg_polygon(out, &pts, gstyle, is_filled);
            svg_polyline(
                out,
                &[
                    Point::new(node_bb.ur.x - fold, node_bb.ur.y),
                    Point::new(node_bb.ur.x - fold, node_bb.ur.y - fold),
                    Point::new(node_bb.ur.x, node_bb.ur.y - fold),
                ],
                gstyle,
            );
        }
        "tab" | "folder" => {
            let tab_w = (rx * 0.45).min(14.0);
            let tab_h = 4.0_f64.min(ry * 0.3);
            let pts = [
                Point::new(node_bb.ur.x, node_bb.ur.y - tab_h),
                Point::new(node_bb.ll.x + tab_w, node_bb.ur.y - tab_h),
                Point::new(node_bb.ll.x + tab_w, node_bb.ur.y),
                Point::new(node_bb.ll.x, node_bb.ur.y),
                Point::new(node_bb.ll.x, node_bb.ll.y),
                Point::new(node_bb.ur.x, node_bb.ll.y),
            ];
            svg_polygon(out, &pts, gstyle, is_filled);
        }
        "box3d" => {
            let depth = 4.0_f64.min(rx * 0.3).min(ry * 0.3);
            let pts = [
                Point::new(node_bb.ur.x, node_bb.ur.y),
                Point::new(node_bb.ll.x + depth, node_bb.ur.y),
                Point::new(node_bb.ll.x, node_bb.ur.y - depth),
                Point::new(node_bb.ll.x, node_bb.ll.y),
                Point::new(node_bb.ur.x - depth, node_bb.ll.y),
                Point::new(node_bb.ur.x, node_bb.ll.y + depth),
            ];
            svg_polygon(out, &pts, gstyle, is_filled);
            svg_polyline(
                out,
                &[
                    Point::new(node_bb.ur.x - depth, node_bb.ur.y - depth),
                    Point::new(node_bb.ll.x, node_bb.ur.y - depth),
                ],
                gstyle,
            );
            svg_polyline(
                out,
                &[
                    Point::new(node_bb.ur.x - depth, node_bb.ur.y - depth),
                    Point::new(node_bb.ur.x - depth, node_bb.ll.y),
                ],
                gstyle,
            );
            svg_polyline(
                out,
                &[
                    Point::new(node_bb.ur.x - depth, node_bb.ur.y - depth),
                    Point::new(node_bb.ur.x, node_bb.ur.y),
                ],
                gstyle,
            );
        }
        "component" => {
            let pts = [
                Point::new(node_bb.ur.x, node_bb.ur.y),
                Point::new(node_bb.ll.x, node_bb.ur.y),
                Point::new(node_bb.ll.x, node_bb.ll.y),
                Point::new(node_bb.ur.x, node_bb.ll.y),
            ];
            svg_polygon(out, &pts, gstyle, is_filled);
            for y_off in [ry * 0.35, -ry * 0.35] {
                let b = BoundingBox::new(
                    node_bb.ll.x - 4.0,
                    center.y + y_off - 2.0,
                    node_bb.ll.x + 4.0,
                    center.y + y_off + 2.0,
                );
                let p = [
                    Point::new(b.ur.x, b.ur.y),
                    Point::new(b.ll.x, b.ur.y),
                    Point::new(b.ll.x, b.ll.y),
                    Point::new(b.ur.x, b.ll.y),
                ];
                svg_polygon(out, &p, gstyle, is_filled);
            }
        }
        "underline" => {
            svg_polyline(
                out,
                &[
                    Point::new(node_bb.ll.x, node_bb.ll.y),
                    Point::new(node_bb.ur.x, node_bb.ll.y),
                ],
                gstyle,
            );
        }
        _ => {
            svg_ellipse(out, center, rx, ry, gstyle, is_filled);
        }
    }
}

fn regular_polygon_vertices(
    center: Point,
    rx: f64,
    ry: f64,
    sides: usize,
    orientation_rad: f64,
) -> Vec<Point> {
    let mut pts = Vec::with_capacity(sides);
    let sector = 2.0 * PI / (sides as f64);
    let start_angle = (sector - PI) / 2.0 + orientation_rad;
    let mut max_cx = 0.0_f64;
    let mut max_cy = 0.0_f64;
    let mut unit_pts = Vec::with_capacity(sides);
    for i in 0..sides {
        let a = start_angle + (i as f64) * sector;
        let ux = a.cos();
        let uy = a.sin();
        max_cx = max_cx.max(ux.abs());
        max_cy = max_cy.max(uy.abs());
        unit_pts.push((ux, uy));
    }
    let sx = if max_cx > 1e-9 { rx / max_cx } else { rx };
    let sy = if max_cy > 1e-9 { ry / max_cy } else { ry };
    for (ux, uy) in unit_pts {
        pts.push(Point::new(center.x + ux * sx, center.y + uy * sy));
    }
    pts
}

// ---------------------------------------------------------------------------
// Record / Mrecord parser & renderer (`lib/common/shapes.c`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) struct RecordField {
    #[allow(dead_code)]
    pub id: Option<String>,
    pub text: Option<String>,
    pub children: Vec<RecordField>,
    pub horizontal: bool,
    pub bb: BoundingBox,
}

pub(crate) fn parse_record_fields(label: &str, root_horizontal: bool) -> RecordField {
    let chars: Vec<char> = label.chars().collect();
    let mut pos = 0;
    parse_record_group(&chars, &mut pos, root_horizontal)
}

fn parse_record_group(chars: &[char], pos: &mut usize, horizontal: bool) -> RecordField {
    let mut children = Vec::new();
    let mut cur_id: Option<String> = None;
    let mut cur_text = String::new();
    let mut has_sub = false;

    while *pos < chars.len() {
        let ch = chars[*pos];
        if ch == '}' {
            break;
        }
        if ch == '|' {
            *pos += 1;
            if !has_sub {
                children.push(RecordField {
                    id: cur_id.take(),
                    text: Some(std::mem::take(&mut cur_text).trim().to_string()),
                    children: Vec::new(),
                    horizontal: !horizontal,
                    bb: BoundingBox::default(),
                });
            }
            has_sub = false;
            continue;
        }
        if ch == '{' {
            *pos += 1;
            let sub = parse_record_group(chars, pos, !horizontal);
            if *pos < chars.len() && chars[*pos] == '}' {
                *pos += 1;
            }
            children.push(sub);
            has_sub = true;
            cur_id = None;
            cur_text.clear();
            continue;
        }
        if ch == '<' && cur_id.is_none() && cur_text.trim().is_empty() {
            *pos += 1;
            let mut port_name = String::new();
            while *pos < chars.len() && chars[*pos] != '>' {
                if chars[*pos] == '\\' && *pos + 1 < chars.len() {
                    *pos += 1;
                }
                port_name.push(chars[*pos]);
                *pos += 1;
            }
            if *pos < chars.len() && chars[*pos] == '>' {
                *pos += 1;
            }
            cur_id = Some(port_name.trim().to_string());
            continue;
        }
        if ch == '\\' && *pos + 1 < chars.len() {
            let next = chars[*pos + 1];
            if matches!(next, '|' | '{' | '}' | '<' | '>' | ' ') {
                cur_text.push(next);
                *pos += 2;
                continue;
            }
        }
        cur_text.push(ch);
        *pos += 1;
    }

    if !has_sub {
        children.push(RecordField {
            id: cur_id.take(),
            text: Some(cur_text.trim().to_string()),
            children: Vec::new(),
            horizontal: !horizontal,
            bb: BoundingBox::default(),
        });
    }

    if children.len() == 1 && children[0].children.is_empty() {
        return children.remove(0);
    }

    RecordField {
        id: None,
        text: None,
        children,
        horizontal,
        bb: BoundingBox::default(),
    }
}

pub(crate) fn layout_record_fields(field: &mut RecordField, bb: BoundingBox) {
    field.bb = bb;
    let n = field.children.len();
    if n == 0 {
        return;
    }
    if field.horizontal {
        let step = bb.width() / (n as f64);
        for (i, child) in field.children.iter_mut().enumerate() {
            let x0 = bb.ll.x + (i as f64) * step;
            let x1 = if i + 1 == n { bb.ur.x } else { x0 + step };
            layout_record_fields(child, BoundingBox::new(x0, bb.ll.y, x1, bb.ur.y));
        }
    } else {
        let step = bb.height() / (n as f64);
        for (i, child) in field.children.iter_mut().enumerate() {
            let y1 = bb.ur.y - (i as f64) * step;
            let y0 = if i + 1 == n { bb.ll.y } else { y1 - step };
            layout_record_fields(child, BoundingBox::new(bb.ll.x, y0, bb.ur.x, y1));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_record_node(
    out: &mut String,
    graph: &Graph,
    _node: &NodeData,
    node_bb: BoundingBox,
    gstyle: &GraphicStyle,
    is_filled: bool,
    is_mrecord: bool,
    raw_label: &str,
    subst_ctx: &LabelSubstContext<'_>,
    font_name: &str,
    font_size: f64,
    font_color: &str,
    scheme: Option<&str>,
) {
    if is_mrecord {
        svg_rounded_box(out, node_bb, gstyle, is_filled);
    } else {
        let pts = [
            Point::new(node_bb.ll.x, node_bb.ll.y),
            Point::new(node_bb.ll.x, node_bb.ur.y),
            Point::new(node_bb.ur.x, node_bb.ur.y),
            Point::new(node_bb.ur.x, node_bb.ll.y),
        ];
        svg_polygon(out, &pts, gstyle, is_filled);
    }

    let rankdir = graph
        .graph_attrs
        .get("rankdir")
        .map(String::as_str)
        .unwrap_or("TB");
    if let Some(geom_rec) = gv_geom::shapes::record_layout(_node, &graph.name, rankdir) {
        let center = Point::new(
            (node_bb.ll.x + node_bb.ur.x) / 2.0,
            (node_bb.ll.y + node_bb.ur.y) / 2.0,
        );
        emit_geom_record_field_recursive(
            out, &geom_rec, center, gstyle, subst_ctx, font_name, font_size, font_color, scheme,
        );
        return;
    }
    let root_horizontal = !matches!(rankdir, "LR" | "RL");
    let substituted = substitute_object_escapes(raw_label, subst_ctx);
    let mut root_field = parse_record_fields(&substituted, root_horizontal);
    layout_record_fields(&mut root_field, node_bb);

    emit_record_field_recursive(
        out,
        &root_field,
        gstyle,
        subst_ctx,
        font_name,
        font_size,
        font_color,
        scheme,
    );
}

#[allow(clippy::too_many_arguments)]
fn emit_record_field_recursive(
    out: &mut String,
    field: &RecordField,
    gstyle: &GraphicStyle,
    subst_ctx: &LabelSubstContext<'_>,
    font_name: &str,
    font_size: f64,
    font_color: &str,
    scheme: Option<&str>,
) {
    if let Some(txt) = &field.text
        && !txt.is_empty()
    {
        let layout = measure_multiline_label_with_context(txt, subst_ctx, font_name, font_size);
        let center = Point::new(
            (field.bb.ll.x + field.bb.ur.x) / 2.0,
            (field.bb.ll.y + field.bb.ur.y) / 2.0,
        );
        emit_text_label(
            out, &layout, center, font_color, scheme, false, false, false,
        );
    }

    for (i, child) in field.children.iter().enumerate() {
        if i > 0 {
            let line_pts = if field.horizontal {
                [
                    Point::new(child.bb.ll.x, child.bb.ll.y),
                    Point::new(child.bb.ll.x, child.bb.ur.y),
                ]
            } else {
                [
                    Point::new(child.bb.ll.x, child.bb.ur.y),
                    Point::new(child.bb.ur.x, child.bb.ur.y),
                ]
            };
            svg_polyline(out, &line_pts, gstyle);
        }
        emit_record_field_recursive(
            out, child, gstyle, subst_ctx, font_name, font_size, font_color, scheme,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_geom_record_field_recursive(
    out: &mut String,
    field: &gv_geom::shapes::RecordField,
    center: Point,
    gstyle: &GraphicStyle,
    subst_ctx: &LabelSubstContext<'_>,
    font_name: &str,
    font_size: f64,
    font_color: &str,
    scheme: Option<&str>,
) {
    let f_bb = BoundingBox::new(
        center.x + field.bbox.ll.x,
        center.y + field.bbox.ll.y,
        center.x + field.bbox.ur.x,
        center.y + field.bbox.ur.y,
    );
    if field.children.is_empty() && !field.text.trim().is_empty() {
        let layout =
            measure_multiline_label_with_context(&field.text, subst_ctx, font_name, font_size);
        let f_center = Point::new((f_bb.ll.x + f_bb.ur.x) / 2.0, (f_bb.ll.y + f_bb.ur.y) / 2.0);
        emit_text_label(
            out, &layout, f_center, font_color, scheme, false, false, false,
        );
    }

    for (i, child) in field.children.iter().enumerate() {
        if i > 0 {
            let c_bb = BoundingBox::new(
                center.x + child.bbox.ll.x,
                center.y + child.bbox.ll.y,
                center.x + child.bbox.ur.x,
                center.y + child.bbox.ur.y,
            );
            let line_pts = if field.horizontal {
                [
                    Point::new(c_bb.ll.x, c_bb.ll.y),
                    Point::new(c_bb.ll.x, c_bb.ur.y),
                ]
            } else {
                [
                    Point::new(c_bb.ll.x, c_bb.ur.y),
                    Point::new(c_bb.ur.x, c_bb.ur.y),
                ]
            };
            svg_polyline(out, &line_pts, gstyle);
        }
        emit_geom_record_field_recursive(
            out, child, center, gstyle, subst_ctx, font_name, font_size, font_color, scheme,
        );
    }
}

// ---------------------------------------------------------------------------
// HTML-like label parser & SVG renderer (`<<table>...</table>>`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct HtmlCell {
    text: String,
    bold: bool,
    italic: bool,
    underline: bool,
    bgcolor: Option<String>,
    color: Option<String>,
    align: TextJustify,
    colspan: usize,
    rowspan: usize,
    border: Option<f64>,
}

#[derive(Debug, Clone, Default)]
struct HtmlTable {
    rows: Vec<Vec<HtmlCell>>,
    bgcolor: Option<String>,
    color: Option<String>,
    border: f64,
    cellborder: f64,
    rounded: bool,
}

fn parse_html_table(html: &str) -> HtmlTable {
    let trimmed = html.trim();
    let inner = trimmed
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(trimmed);

    let mut table = HtmlTable {
        rows: Vec::new(),
        bgcolor: None,
        color: None,
        border: 1.0,
        cellborder: 1.0,
        rounded: false,
    };

    let lower = inner.to_ascii_lowercase();
    if !lower.contains("<table") {
        let cell = parse_html_cell_content(inner, "<td");
        table.border = 0.0;
        table.cellborder = 0.0;
        table.rows.push(vec![cell]);
        return table;
    }

    if let Some(tbl_idx) = lower.find("<table")
        && let Some(tag_end) = inner[tbl_idx..].find('>')
    {
        let tag_header = &inner[tbl_idx..tbl_idx + tag_end];
        if let Some(bg) = extract_html_attr(tag_header, "bgcolor") {
            table.bgcolor = Some(bg);
        }
        if let Some(clr) = extract_html_attr(tag_header, "color") {
            table.color = Some(clr);
        }
        if let Some(b) = extract_html_attr(tag_header, "border")
            && let Ok(v) = b.parse::<f64>()
        {
            table.border = v.max(0.0);
        }
        if let Some(cb) = extract_html_attr(tag_header, "cellborder")
            && let Ok(v) = cb.parse::<f64>()
        {
            table.cellborder = v.max(0.0);
        }
        if let Some(st) = extract_html_attr(tag_header, "style")
            && st.to_ascii_lowercase().contains("rounded")
        {
            table.rounded = true;
        }
    }

    let mut search_pos = 0;
    while let Some(tr_rel) = lower[search_pos..].find("<tr") {
        let tr_start = search_pos + tr_rel;
        let Some(tr_open_end_rel) = inner[tr_start..].find('>') else {
            break;
        };
        let tr_body_start = tr_start + tr_open_end_rel + 1;
        let tr_body_end = lower[tr_body_start..]
            .find("</tr")
            .map(|i| tr_body_start + i)
            .unwrap_or(inner.len());
        let tr_slice = &inner[tr_body_start..tr_body_end];
        let tr_lower = &lower[tr_body_start..tr_body_end];

        let mut row_cells = Vec::new();
        let mut td_pos = 0;
        while let Some(td_rel) = tr_lower[td_pos..].find("<td") {
            let td_start = td_pos + td_rel;
            let Some(td_open_end_rel) = tr_slice[td_start..].find('>') else {
                break;
            };
            let td_header = &tr_slice[td_start..td_start + td_open_end_rel];
            let td_content_start = td_start + td_open_end_rel + 1;
            let td_content_end = tr_lower[td_content_start..]
                .find("</td")
                .map(|i| td_content_start + i)
                .unwrap_or(tr_slice.len());
            let td_content = &tr_slice[td_content_start..td_content_end];

            let cell = parse_html_cell_content(td_content, td_header);
            row_cells.push(cell);
            td_pos = (td_content_end + 4).min(tr_slice.len());
        }

        if !row_cells.is_empty() {
            table.rows.push(row_cells);
        }
        search_pos = (tr_body_end + 4).min(inner.len());
    }

    if table.rows.is_empty() {
        table.rows.push(vec![parse_html_cell_content(inner, "<td")]);
    }

    table
}

fn parse_html_cell_content(content: &str, td_header: &str) -> HtmlCell {
    let mut cell = HtmlCell {
        colspan: 1,
        rowspan: 1,
        ..Default::default()
    };

    if let Some(bg) = extract_html_attr(td_header, "bgcolor") {
        cell.bgcolor = Some(bg);
    }
    if let Some(clr) = extract_html_attr(td_header, "color") {
        cell.color = Some(clr);
    }
    if let Some(cs) = extract_html_attr(td_header, "colspan")
        && let Ok(v) = cs.parse::<usize>()
    {
        cell.colspan = v.max(1);
    }
    if let Some(rs) = extract_html_attr(td_header, "rowspan")
        && let Ok(v) = rs.parse::<usize>()
    {
        cell.rowspan = v.max(1);
    }
    if let Some(b) = extract_html_attr(td_header, "border")
        && let Ok(v) = b.parse::<f64>()
    {
        cell.border = Some(v.max(0.0));
    }
    if let Some(align) = extract_html_attr(td_header, "align") {
        cell.align = match align.to_ascii_lowercase().as_str() {
            "left" => TextJustify::Left,
            "right" => TextJustify::Right,
            _ => TextJustify::Center,
        };
    }

    let mut text = String::new();
    let mut i = 0;
    let bytes = content.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'<'
            && let Some(close_rel) = content[i..].find('>')
        {
            let tag = content[i + 1..i + close_rel].trim();
            let tag_lower = tag.to_ascii_lowercase();
            if tag_lower == "b" || tag_lower == "strong" {
                cell.bold = true;
            } else if tag_lower == "i" || tag_lower == "em" {
                cell.italic = true;
            } else if tag_lower == "u" {
                cell.underline = true;
            } else if tag_lower.starts_with("br") {
                text.push('\n');
            } else if tag_lower.starts_with("font")
                && let Some(fc) = extract_html_attr(tag, "color")
            {
                cell.color = Some(fc);
            }
            i += close_rel + 1;
            continue;
        }
        let ch = content[i..].chars().next().unwrap_or('\0');
        text.push(ch);
        i += ch.len_utf8();
    }

    let cleaned_lines: Vec<String> = text
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect();
    cell.text = cleaned_lines.join("\n");
    cell
}

fn extract_html_attr(tag: &str, attr_name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let pat = format!("{}=", attr_name.to_ascii_lowercase());
    let idx = lower.find(&pat)?;
    let rest = tag[idx + pat.len()..].trim_start();
    if let Some(stripped) = rest.strip_prefix('"') {
        let end = stripped.find('"')?;
        Some(stripped[..end].to_string())
    } else if let Some(stripped) = rest.strip_prefix('\'') {
        let end = stripped.find('\'')?;
        Some(stripped[..end].to_string())
    } else {
        let end = rest
            .find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')
            .unwrap_or(rest.len());
        Some(rest[..end].to_string())
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_geom_html_table_recursive(
    out: &mut String,
    tbl: &gv_geom::shapes::HtmlTable,
    center: Point,
    default_font_color: &str,
    default_stroke: &str,
    scheme: Option<&str>,
) {
    let t_bb = BoundingBox::new(
        center.x + tbl.bbox.ll.x,
        center.y + tbl.bbox.ll.y,
        center.x + tbl.bbox.ur.x,
        center.y + tbl.bbox.ur.y,
    );
    if tbl.border > 0.0 || tbl.bgcolor.is_some() {
        let (fill, fill_opacity) = format_paint_and_opacity(tbl.bgcolor.as_deref(), "none", scheme);
        let tbl_style = GraphicStyle {
            fill,
            fill_opacity,
            stroke: if tbl.border > 0.0 {
                default_stroke.to_string()
            } else {
                "none".to_string()
            },
            stroke_opacity: None,
            penwidth: tbl.border.max(1.0),
            dashed: false,
            dotted: false,
        };
        let pts = [
            Point::new(t_bb.ll.x, t_bb.ur.y),
            Point::new(t_bb.ll.x, t_bb.ll.y),
            Point::new(t_bb.ur.x, t_bb.ll.y),
            Point::new(t_bb.ur.x, t_bb.ur.y),
        ];
        if tbl.rounded {
            svg_rounded_box(out, t_bb, &tbl_style, tbl.bgcolor.is_some());
        } else {
            svg_polygon(out, &pts, &tbl_style, tbl.bgcolor.is_some());
        }
    }

    for cell in &tbl.cells {
        let c_bb = BoundingBox::new(
            center.x + cell.bbox.ll.x,
            center.y + cell.bbox.ll.y,
            center.x + cell.bbox.ur.x,
            center.y + cell.bbox.ur.y,
        );
        if cell.border > 0.0 || cell.bgcolor.is_some() {
            let (c_fill, c_op) = format_paint_and_opacity(cell.bgcolor.as_deref(), "none", scheme);
            let c_style = GraphicStyle {
                fill: c_fill,
                fill_opacity: c_op,
                stroke: if cell.border > 0.0 {
                    default_stroke.to_string()
                } else {
                    "none".to_string()
                },
                stroke_opacity: None,
                penwidth: cell.border.max(1.0),
                dashed: false,
                dotted: false,
            };
            let c_pts = [
                Point::new(c_bb.ll.x, c_bb.ur.y),
                Point::new(c_bb.ll.x, c_bb.ll.y),
                Point::new(c_bb.ur.x, c_bb.ll.y),
                Point::new(c_bb.ur.x, c_bb.ur.y),
            ];
            svg_polygon(out, &c_pts, &c_style, cell.bgcolor.is_some());
        }

        if let Some(nested) = &cell.nested_table {
            emit_geom_html_table_recursive(
                out,
                nested,
                center,
                default_font_color,
                default_stroke,
                scheme,
            );
        } else if let Some(rich) = &cell.rich_text {
            let spans = gv_geom::html_label::emit_htextspans(rich, center, default_font_color);
            emit_html_text_spans(out, &spans, scheme);
        }
    }
}

/// `svg_textspan` (plugin/core/gvrender_core_svg.c) for the spans of an HTML text block.
fn emit_html_text_spans(
    out: &mut String,
    spans: &[gv_geom::html_label::HtmlTextSpan],
    scheme: Option<&str>,
) {
    use gv_geom::html_label::{HTML_BF, HTML_IF, HTML_OL, HTML_S, HTML_SUB, HTML_SUP, HTML_UL};
    for sp in spans {
        out.push_str("<text xml:space=\"preserve\" text-anchor=\"start\"");
        let _ = write!(
            out,
            " x=\"{}\" y=\"{}\"",
            fmt_gv_double(sp.x),
            fmt_gv_double(-(sp.y + sp.yoffset_centerline))
        );
        let (mut weight, mut style) = (false, false);
        if let Some(a) = translate_postscript_fontname(&sp.font_name) {
            let _ = write!(out, " font-family=\"{}", a.family);
            if a.svg_font_family != a.family {
                let _ = write!(out, ",{}", a.svg_font_family);
            }
            out.push('"');
            if let Some(w) = a.weight {
                let _ = write!(out, " font-weight=\"{w}\"");
                weight = true;
            }
            if let Some(st) = a.stretch {
                let _ = write!(out, " font-stretch=\"{st}\"");
            }
            if let Some(st) = a.style {
                let _ = write!(out, " font-style=\"{st}\"");
                style = true;
            }
        } else {
            let _ = write!(
                out,
                " font-family=\"{}\"",
                xml_escape(&sp.font_name, false, false, false)
            );
        }
        let flags = sp.flags;
        if flags & HTML_BF != 0 && !weight {
            out.push_str(" font-weight=\"bold\"");
        }
        if flags & HTML_IF != 0 && !style {
            out.push_str(" font-style=\"italic\"");
        }
        if flags & (HTML_UL | HTML_S | HTML_OL) != 0 {
            let mut parts = Vec::new();
            if flags & HTML_UL != 0 {
                parts.push("underline");
            }
            if flags & HTML_OL != 0 {
                parts.push("overline");
            }
            if flags & HTML_S != 0 {
                parts.push("line-through");
            }
            let _ = write!(out, " text-decoration=\"{}\"", parts.join(","));
        }
        if flags & HTML_SUP != 0 {
            out.push_str(" baseline-shift=\"super\"");
        }
        if flags & HTML_SUB != 0 {
            out.push_str(" baseline-shift=\"sub\"");
        }
        let _ = write!(out, " font-size=\"{:.2}\"", sp.font_size);
        let (fill_str, fill_opacity) = format_paint_and_opacity(Some(&sp.color), "black", scheme);
        if !fill_str.eq_ignore_ascii_case("black") {
            let _ = write!(out, " fill=\"{fill_str}\"");
            if let Some(op) = fill_opacity {
                let _ = write!(out, " fill-opacity=\"{op:.6}\"");
            }
        }
        out.push('>');
        out.push_str(&xml_escape(&sp.text, true, true, true));
        out.push_str("</text>\n");
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_html_label(
    out: &mut String,
    html: &str,
    bb: BoundingBox,
    font_name: &str,
    font_size: f64,
    default_font_color: &str,
    default_stroke: &str,
    scheme: Option<&str>,
) {
    if let Some(geom_tbl) = gv_geom::shapes::parse_html_table(html, "", "", font_name, font_size) {
        let center = Point::new((bb.ll.x + bb.ur.x) / 2.0, (bb.ll.y + bb.ur.y) / 2.0);
        emit_geom_html_table_recursive(
            out,
            &geom_tbl,
            center,
            default_font_color,
            default_stroke,
            scheme,
        );
        return;
    }
    if !html.to_ascii_lowercase().contains("<table") {
        // HTML text label (`emit_html_txt`); a label C rejects draws nothing.
        let inner = if html.len() >= 2 && html.starts_with('<') && html.ends_with('>') {
            &html[1..html.len() - 1]
        } else {
            html
        };
        let obj = LabelSubstContext::default();
        let lay = gv_geom::html_label::make_html_label(
            inner,
            &obj,
            font_name,
            font_size,
            Some(default_font_color),
            None,
        );
        if let Some(gv_geom::html_label::HtmlLabel::Text(t)) = &lay.label {
            let center = Point::new((bb.ll.x + bb.ur.x) / 2.0, (bb.ll.y + bb.ur.y) / 2.0);
            let spans = gv_geom::html_label::emit_htextspans(t, center, default_font_color);
            emit_html_text_spans(out, &spans, scheme);
        }
        return;
    }
    let table = parse_html_table(html);
    let num_rows = table.rows.len().max(1);

    let stroke_color = table
        .color
        .as_deref()
        .map(|c| format_paint_and_opacity(Some(c), default_stroke, scheme).0)
        .unwrap_or_else(|| default_stroke.to_string());

    if table.border > 0.0 || table.bgcolor.is_some() {
        let (fill, fill_opacity) =
            format_paint_and_opacity(table.bgcolor.as_deref(), "none", scheme);
        let tbl_style = GraphicStyle {
            fill,
            fill_opacity,
            stroke: if table.border > 0.0 {
                stroke_color.clone()
            } else {
                "none".to_string()
            },
            stroke_opacity: None,
            penwidth: table.border.max(1.0),
            dashed: false,
            dotted: false,
        };
        let pts = [
            Point::new(bb.ll.x, bb.ur.y),
            Point::new(bb.ll.x, bb.ll.y),
            Point::new(bb.ur.x, bb.ll.y),
            Point::new(bb.ur.x, bb.ur.y),
        ];
        if table.rounded {
            svg_rounded_box(out, bb, &tbl_style, table.bgcolor.is_some());
        } else {
            svg_polygon(out, &pts, &tbl_style, table.bgcolor.is_some());
        }
    }

    let row_h = bb.height() / (num_rows as f64);
    for (r_idx, row) in table.rows.iter().enumerate() {
        let total_cols: usize = row.iter().map(|c| c.colspan.max(1)).sum::<usize>().max(1);
        let col_w = bb.width() / (total_cols as f64);
        let y_top = bb.ur.y - (r_idx as f64) * row_h;

        let mut col_cursor = 0usize;
        for cell in row {
            let span_cols = cell.colspan.max(1);
            let span_rows = cell.rowspan.max(1);
            let x_left = bb.ll.x + (col_cursor as f64) * col_w;
            let x_right = x_left + (span_cols as f64) * col_w;
            let cell_y_bot = (y_top - (span_rows as f64) * row_h).max(bb.ll.y);
            col_cursor += span_cols;

            let cell_bb = BoundingBox::new(x_left, cell_y_bot, x_right, y_top);
            let cell_border = cell.border.unwrap_or(table.cellborder);

            if cell_border > 0.0 || cell.bgcolor.is_some() {
                let (c_fill, c_op) =
                    format_paint_and_opacity(cell.bgcolor.as_deref(), "none", scheme);
                let c_style = GraphicStyle {
                    fill: c_fill,
                    fill_opacity: c_op,
                    stroke: if cell_border > 0.0 {
                        stroke_color.clone()
                    } else {
                        "none".to_string()
                    },
                    stroke_opacity: None,
                    penwidth: cell_border.max(1.0),
                    dashed: false,
                    dotted: false,
                };
                let c_pts = [
                    Point::new(cell_bb.ll.x, cell_bb.ur.y),
                    Point::new(cell_bb.ll.x, cell_bb.ll.y),
                    Point::new(cell_bb.ur.x, cell_bb.ll.y),
                    Point::new(cell_bb.ur.x, cell_bb.ur.y),
                ];
                svg_polygon(out, &c_pts, &c_style, cell.bgcolor.is_some());
            }

            if !cell.text.is_empty() {
                let ctx = LabelSubstContext::default();
                let mut layout =
                    measure_multiline_label_with_context(&cell.text, &ctx, font_name, font_size);
                for line in &mut layout.lines {
                    line.justify = cell.align;
                }
                let cell_center = Point::new(
                    (cell_bb.ll.x + cell_bb.ur.x) / 2.0,
                    (cell_bb.ll.y + cell_bb.ur.y) / 2.0,
                );
                let fc = cell.color.as_deref().unwrap_or(default_font_color);
                emit_text_label(
                    out,
                    &layout,
                    cell_center,
                    fc,
                    scheme,
                    cell.bold,
                    cell.italic,
                    cell.underline,
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Edge & Arrowhead rendering (`lib/common/arrows.c`, `lib/common/emit.c`)
// ---------------------------------------------------------------------------

fn emit_edge(
    out: &mut String,
    graph: &Graph,
    edge: &EdgeData,
    edge_idx: usize,
    parent_scheme: Option<&str>,
) {
    let style = parse_style(edge.attrs.get("style").map(String::as_str));
    if style.invis {
        return;
    }

    let scheme = edge
        .attrs
        .get("colorscheme")
        .map(String::as_str)
        .or(parent_scheme);

    let tail_name = graph
        .nodes
        .get(edge.tail.0 as usize)
        .map(|n| n.name.as_str())
        .unwrap_or("");
    let head_name = graph
        .nodes
        .get(edge.head.0 as usize)
        .map(|n| n.name.as_str())
        .unwrap_or("");
    let edge_op = if graph.kind == GraphKind::Directed {
        "->"
    } else {
        "--"
    };
    let comment_title = format!("{tail_name}{edge_op}{head_name}");
    let _ = writeln!(
        out,
        "<!-- {} -->",
        xml_escape(&comment_title, true, true, false)
    );
    if let Some(comment) = edge.attrs.get("comment")
        && !comment.is_empty()
    {
        let _ = writeln!(out, "<!-- {} -->", xml_escape(comment, true, true, false));
    }

    let subst_ctx = LabelSubstContext {
        node_name: "",
        graph_name: &graph.name,
        tail_name,
        head_name,
        is_edge: true,
        is_directed: graph.kind == GraphKind::Directed,
        tail_port: edge.tail_port.name.as_deref(),
        head_port: edge.head_port.name.as_deref(),
        label_text: edge.attrs.get("label").map(String::as_str).unwrap_or(""),
    };

    let edge_id = edge
        .attrs
        .get("id")
        .filter(|s| !s.is_empty())
        .map(|s| xml_escape(&substitute_object_escapes(s, &subst_ctx), true, true, false))
        .unwrap_or_else(|| format!("edge{edge_idx}"));
    let mut edge_class = "edge".to_string();
    if let Some(cls) = edge.attrs.get("class")
        && !cls.is_empty()
    {
        edge_class.push(' ');
        edge_class.push_str(&xml_escape(cls, true, true, false));
    }
    let _ = writeln!(out, "<g id=\"{edge_id}\" class=\"{edge_class}\">");
    let full_edge_title = substitute_object_escapes("\\E", &subst_ctx);
    let _ = writeln!(
        out,
        "<title>{}</title>",
        xml_escape(&full_edge_title, true, true, false)
    );

    let color_spec = edge.attrs.get("color").map(String::as_str);
    let fill_spec = edge
        .attrs
        .get("fillcolor")
        .map(String::as_str)
        .or(color_spec);
    let (stroke, stroke_opacity) = format_paint_and_opacity(color_spec, "black", scheme);
    let (fill, fill_opacity) = format_paint_and_opacity(fill_spec, "black", scheme);

    let mut penwidth = parse_f64_attr(&edge.attrs, "penwidth", PENWIDTH_NORMAL);
    if let Some(lw) = style.linewidth {
        penwidth = lw;
    } else if style.bold && (penwidth - PENWIDTH_NORMAL).abs() < 1e-6 {
        penwidth = 2.0;
    }
    let arrowsize = parse_f64_attr(&edge.attrs, "arrowsize", 1.0).max(0.0);

    let path_style = GraphicStyle {
        fill: fill.clone(),
        fill_opacity,
        stroke: stroke.clone(),
        stroke_opacity,
        penwidth,
        dashed: style.dashed,
        dotted: style.dotted,
    };
    let arrow_style = GraphicStyle {
        fill,
        fill_opacity,
        stroke,
        stroke_opacity,
        penwidth,
        dashed: false,
        dotted: false,
    };

    let fallback_spline;
    let spline: &Spline = if edge.spline.segments.is_empty() {
        let p_tail = graph
            .nodes
            .get(edge.tail.0 as usize)
            .map(|n| n.pos)
            .unwrap_or_default();
        let p_head = graph
            .nodes
            .get(edge.head.0 as usize)
            .map(|n| n.pos)
            .unwrap_or_default();
        fallback_spline = straight_spline(p_tail, p_head);
        &fallback_spline
    } else {
        &edge.spline
    };

    let (want_start_arrow, want_end_arrow) = edge_arrow_directions(graph, edge);
    let arrowhead_shape = edge
        .attrs
        .get("arrowhead")
        .map(String::as_str)
        .unwrap_or("normal");
    let arrowtail_shape = edge
        .attrs
        .get("arrowtail")
        .map(String::as_str)
        .unwrap_or("normal");

    for seg in &spline.segments {
        if seg.points.is_empty() {
            continue;
        }
        svg_bezier(out, &seg.points, &path_style, false);

        if (seg.start_arrow.is_some() || want_start_arrow)
            && !arrowtail_shape.eq_ignore_ascii_case("none")
            && arrowsize > 0.0
        {
            let base = seg.points[0];
            let tip = seg.start_arrow.unwrap_or(base);
            let dir = arrow_direction_at_start(&seg.points, tip);
            emit_arrowhead(
                out,
                base,
                tip,
                dir,
                arrowtail_shape,
                arrowsize,
                &arrow_style,
            );
        }

        if (seg.end_arrow.is_some() || want_end_arrow)
            && !arrowhead_shape.eq_ignore_ascii_case("none")
            && arrowsize > 0.0
        {
            let base = *seg.points.last().unwrap_or(&seg.points[0]);
            let tip = seg.end_arrow.unwrap_or(base);
            let dir = arrow_direction_at_end(&seg.points, tip);
            emit_arrowhead(
                out,
                base,
                tip,
                dir,
                arrowhead_shape,
                arrowsize,
                &arrow_style,
            );
        }
    }

    let font_name = edge
        .attrs
        .get("fontname")
        .map(String::as_str)
        .unwrap_or(DEFAULT_FONT_NAME);
    let font_size = parse_f64_attr(&edge.attrs, "fontsize", DEFAULT_FONT_SIZE);
    let font_color = edge
        .attrs
        .get("fontcolor")
        .map(String::as_str)
        .unwrap_or("black");

    let mid_pt = edge.label_pos.unwrap_or_else(|| spline_midpoint(spline));

    if let Some(label) = edge.attrs.get("label")
        && !label.is_empty()
    {
        if is_html_string(label) {
            let (w, h) = (72.0, 24.0);
            let lbb = BoundingBox::new(
                mid_pt.x - w / 2.0,
                mid_pt.y - h / 2.0,
                mid_pt.x + w / 2.0,
                mid_pt.y + h / 2.0,
            );
            emit_html_label(
                out,
                label,
                lbb,
                font_name,
                font_size,
                font_color,
                &path_style.stroke,
                scheme,
            );
        } else {
            let layout =
                measure_multiline_label_with_context(label, &subst_ctx, font_name, font_size);
            emit_text_label(
                out, &layout, mid_pt, font_color, scheme, style.bold, false, false,
            );
        }
    }

    if let Some(xlabel) = edge.attrs.get("xlabel")
        && !xlabel.is_empty()
    {
        let layout = measure_multiline_label_with_context(xlabel, &subst_ctx, font_name, font_size);
        let xlp = Point::new(mid_pt.x, mid_pt.y + layout.height);
        emit_text_label(
            out, &layout, xlp, font_color, scheme, style.bold, false, false,
        );
    }

    let label_font_name = edge
        .attrs
        .get("labelfontname")
        .map(String::as_str)
        .unwrap_or(font_name);
    let label_font_size = parse_f64_attr(&edge.attrs, "labelfontsize", DEFAULT_LABEL_FONT_SIZE);
    let label_font_color = edge
        .attrs
        .get("labelfontcolor")
        .map(String::as_str)
        .unwrap_or(font_color);

    if let Some(headlabel) = edge.attrs.get("headlabel")
        && !headlabel.is_empty()
    {
        let layout = measure_multiline_label_with_context(
            headlabel,
            &subst_ctx,
            label_font_name,
            label_font_size,
        );
        let h_pt = spline_end_point(spline);
        let h_pos = Point::new(h_pt.x - layout.width / 2.0 - 4.0, h_pt.y + 6.0);
        emit_text_label(
            out,
            &layout,
            h_pos,
            label_font_color,
            scheme,
            style.bold,
            false,
            false,
        );
    }

    if let Some(taillabel) = edge.attrs.get("taillabel")
        && !taillabel.is_empty()
    {
        let layout = measure_multiline_label_with_context(
            taillabel,
            &subst_ctx,
            label_font_name,
            label_font_size,
        );
        let t_pt = spline_start_point(spline);
        let t_pos = Point::new(t_pt.x + layout.width / 2.0 + 4.0, t_pt.y - 6.0);
        emit_text_label(
            out,
            &layout,
            t_pos,
            label_font_color,
            scheme,
            style.bold,
            false,
            false,
        );
    }

    out.push_str("</g>\n");
}

pub(crate) fn edge_arrow_directions(graph: &Graph, edge: &EdgeData) -> (bool, bool) {
    let default_end = graph.kind == GraphKind::Directed;
    let Some(dir) = edge.attrs.get("dir") else {
        return (false, default_end);
    };
    match dir.trim().to_ascii_lowercase().as_str() {
        "forward" => (false, true),
        "back" => (true, false),
        "both" => (true, true),
        "none" => (false, false),
        _ => (false, default_end),
    }
}

pub(crate) fn arrow_direction_at_end(points: &[Point], tip: Point) -> Point {
    let Some(&last_pt) = points.last() else {
        return Point::new(0.0, -1.0);
    };
    let delta_tip = Point::new(tip.x - last_pt.x, tip.y - last_pt.y);
    if delta_tip.x.hypot(delta_tip.y) > 0.05 {
        return delta_tip.normalized_or_zero();
    }
    for prev in points.iter().rev().skip(1) {
        let d = Point::new(last_pt.x - prev.x, last_pt.y - prev.y);
        if d.x.hypot(d.y) > 0.05 {
            return d.normalized_or_zero();
        }
    }
    Point::new(0.0, -1.0)
}

pub(crate) fn arrow_direction_at_start(points: &[Point], tip: Point) -> Point {
    let Some(&first_pt) = points.first() else {
        return Point::new(0.0, 1.0);
    };
    let delta_tip = Point::new(tip.x - first_pt.x, tip.y - first_pt.y);
    if delta_tip.x.hypot(delta_tip.y) > 0.05 {
        return delta_tip.normalized_or_zero();
    }
    for next in points.iter().skip(1) {
        let d = Point::new(first_pt.x - next.x, first_pt.y - next.y);
        if d.x.hypot(d.y) > 0.05 {
            return d.normalized_or_zero();
        }
    }
    Point::new(0.0, 1.0)
}

/// Computes the 3 vertices of a standard arrowhead polygon oriented along `dir` (unit vector
/// pointing toward the arrow tip), matching `arrow_type_normal0` in `lib/common/arrows.c`.
pub(crate) fn compute_normal_arrow_polygon(
    base: Point,
    tip: Point,
    dir: Point,
    arrowsize: f64,
    penwidth: f64,
    inverted: bool,
) -> [Point; 3] {
    let u_dir = if dir.x == 0.0 && dir.y == 0.0 {
        Point::new(0.0, -1.0)
    } else {
        dir
    };
    let dist_bt = base.dist(tip);
    let len = if dist_bt > 1.0 {
        dist_bt
    } else {
        ARROW_LENGTH * arrowsize
    };
    let actual_base = if dist_bt > 1.0 {
        base
    } else {
        Point::new(tip.x - u_dir.x * len, tip.y - u_dir.y * len)
    };

    let mut arrowwidth = 0.35;
    if penwidth > 4.0 {
        arrowwidth *= penwidth / 4.0;
    }
    let half_w = len * arrowwidth;
    let nx = -u_dir.y * half_w;
    let ny = u_dir.x * half_w;

    if inverted {
        [
            Point::new(tip.x - nx, tip.y - ny),
            actual_base,
            Point::new(tip.x + nx, tip.y + ny),
        ]
    } else {
        [
            Point::new(actual_base.x - nx, actual_base.y - ny),
            tip,
            Point::new(actual_base.x + nx, actual_base.y + ny),
        ]
    }
}

fn emit_arrowhead(
    out: &mut String,
    base: Point,
    tip: Point,
    dir: Point,
    shape_spec: &str,
    arrowsize: f64,
    style: &GraphicStyle,
) {
    let s = shape_spec.trim().to_ascii_lowercase();
    let is_open = s.starts_with('o') || s.starts_with("empty") || s == "invempty";
    let is_inv = s.contains("inv");
    let len = if base.dist(tip) > 1.0 {
        base.dist(tip)
    } else {
        ARROW_LENGTH * arrowsize
    };
    let u_dir = if dir.x == 0.0 && dir.y == 0.0 {
        Point::new(0.0, -1.0)
    } else {
        dir
    };
    let actual_base = if base.dist(tip) > 1.0 {
        base
    } else {
        Point::new(tip.x - u_dir.x * len, tip.y - u_dir.y * len)
    };

    if s.contains("dot") {
        let r = 0.4 * ARROW_LENGTH * arrowsize;
        let center = Point::new(tip.x - u_dir.x * r, tip.y - u_dir.y * r);
        svg_ellipse(out, center, r, r, style, !is_open);
    } else if s.contains("diamond") {
        let half_w = len * 0.35;
        let nx = -u_dir.y * half_w;
        let ny = u_dir.x * half_w;
        let mid = Point::new((actual_base.x + tip.x) / 2.0, (actual_base.y + tip.y) / 2.0);
        let pts = [
            actual_base,
            Point::new(mid.x - nx, mid.y - ny),
            tip,
            Point::new(mid.x + nx, mid.y + ny),
        ];
        svg_polygon(out, &pts, style, !is_open);
    } else if s.contains("box") {
        let half_w = len * 0.35;
        let nx = -u_dir.y * half_w;
        let ny = u_dir.x * half_w;
        let pts = [
            Point::new(actual_base.x - nx, actual_base.y - ny),
            Point::new(tip.x - nx, tip.y - ny),
            Point::new(tip.x + nx, tip.y + ny),
            Point::new(actual_base.x + nx, actual_base.y + ny),
        ];
        svg_polygon(out, &pts, style, !is_open);
    } else if s.contains("tee") {
        let half_w = len * 0.5;
        let nx = -u_dir.y * half_w;
        let ny = u_dir.x * half_w;
        let pts = [
            Point::new(tip.x - nx, tip.y - ny),
            Point::new(tip.x + nx, tip.y + ny),
        ];
        svg_polyline(out, &pts, style);
    } else if s.contains("vee") || s.contains("crow") || s == "open" {
        let half_w = len * 0.45;
        let nx = -u_dir.y * half_w;
        let ny = u_dir.x * half_w;
        let mid = Point::new(
            actual_base.x + u_dir.x * (len * 0.5),
            actual_base.y + u_dir.y * (len * 0.5),
        );
        let pts = [
            Point::new(actual_base.x - nx, actual_base.y - ny),
            tip,
            Point::new(actual_base.x + nx, actual_base.y + ny),
            mid,
        ];
        svg_polygon(out, &pts, style, true);
    } else {
        let pts = compute_normal_arrow_polygon(
            actual_base,
            tip,
            u_dir,
            arrowsize,
            style.penwidth,
            is_inv,
        );
        svg_polygon(out, &pts, style, !is_open);
    }
}

fn spline_midpoint(spline: &Spline) -> Point {
    let Some(seg) = spline.segments.first() else {
        return Point::default();
    };
    if seg.points.is_empty() {
        return Point::default();
    }
    let mid_idx = seg.points.len() / 2;
    if seg.points.len() == 4 {
        let p0 = seg.points[0];
        let p1 = seg.points[1];
        let p2 = seg.points[2];
        let p3 = seg.points[3];
        Point::new(
            0.125 * (p0.x + 3.0 * p1.x + 3.0 * p2.x + p3.x),
            0.125 * (p0.y + 3.0 * p1.y + 3.0 * p2.y + p3.y),
        )
    } else {
        seg.points[mid_idx]
    }
}

fn spline_start_point(spline: &Spline) -> Point {
    spline
        .segments
        .first()
        .and_then(|s| s.start_arrow.or_else(|| s.points.first().copied()))
        .unwrap_or_default()
}

fn spline_end_point(spline: &Spline) -> Point {
    spline
        .segments
        .last()
        .and_then(|s| s.end_arrow.or_else(|| s.points.last().copied()))
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Low-level SVG primitive emitters (`gvrender_core_svg.c`)
// ---------------------------------------------------------------------------

fn svg_ellipse(
    out: &mut String,
    center: Point,
    rx: f64,
    ry: f64,
    style: &GraphicStyle,
    filled: bool,
) {
    out.push_str("<ellipse");
    style.write_svg_attrs(out, filled);
    let _ = writeln!(
        out,
        " cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\"/>",
        fmt_gv_double(center.x),
        fmt_gv_double(-center.y),
        fmt_gv_double(rx),
        fmt_gv_double(ry)
    );
}

fn svg_polygon(out: &mut String, pts: &[Point], style: &GraphicStyle, filled: bool) {
    if pts.is_empty() {
        return;
    }
    out.push_str("<polygon");
    style.write_svg_attrs(out, filled);
    out.push_str(" points=\"");
    for pt in pts {
        let _ = write!(out, "{},{} ", fmt_gv_double(pt.x), fmt_gv_double(-pt.y));
    }
    let _ = writeln!(
        out,
        "{},{}\"/>",
        fmt_gv_double(pts[0].x),
        fmt_gv_double(-pts[0].y)
    );
}

fn svg_polyline(out: &mut String, pts: &[Point], style: &GraphicStyle) {
    if pts.is_empty() {
        return;
    }
    out.push_str("<polyline");
    style.write_svg_attrs(out, false);
    out.push_str(" points=\"");
    for (i, pt) in pts.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let _ = write!(out, "{},{}", fmt_gv_double(pt.x), fmt_gv_double(-pt.y));
    }
    out.push_str("\"/>\n");
}

fn svg_bezier(out: &mut String, pts: &[Point], style: &GraphicStyle, filled: bool) {
    if pts.is_empty() {
        return;
    }
    out.push_str("<path");
    style.write_svg_attrs(out, filled);
    out.push_str(" d=\"");
    for (i, pt) in pts.iter().enumerate() {
        let prefix = if i == 0 {
            'M'
        } else if i == 1 {
            'C'
        } else {
            ' '
        };
        let _ = write!(
            out,
            "{prefix}{},{}",
            fmt_gv_double(pt.x),
            fmt_gv_double(-pt.y)
        );
    }
    out.push_str("\"/>\n");
}

fn svg_rounded_box(out: &mut String, b: BoundingBox, style: &GraphicStyle, filled: bool) {
    let r = 12.0_f64.min(b.width() / 3.0).min(b.height() / 3.0);
    let c = 0.5 * r;
    let pts = [
        Point::new(b.ll.x + r, b.ll.y),
        Point::new(b.ur.x - r, b.ll.y),
        Point::new(b.ur.x - c, b.ll.y),
        Point::new(b.ur.x, b.ll.y + c),
        Point::new(b.ur.x, b.ll.y + r),
        Point::new(b.ur.x, b.ur.y - r),
        Point::new(b.ur.x, b.ur.y - c),
        Point::new(b.ur.x - c, b.ur.y),
        Point::new(b.ur.x - r, b.ur.y),
        Point::new(b.ll.x + r, b.ur.y),
        Point::new(b.ll.x + c, b.ur.y),
        Point::new(b.ll.x, b.ur.y - c),
        Point::new(b.ll.x, b.ur.y - r),
        Point::new(b.ll.x, b.ll.y + r),
        Point::new(b.ll.x, b.ll.y + c),
        Point::new(b.ll.x + c, b.ll.y),
        Point::new(b.ll.x + r, b.ll.y),
    ];
    out.push_str("<path");
    style.write_svg_attrs(out, filled);
    let _ = writeln!(
        out,
        " d=\"M{},{}C{},{} {},{} {},{}C{},{} {},{} {},{}C{},{} {},{} {},{}C{},{} {},{} {},{}C{},{} {},{} {},{}C{},{} {},{} {},{}C{},{} {},{} {},{}C{},{} {},{} {},{}\"/>",
        fmt_gv_double(pts[0].x),
        fmt_gv_double(-pts[0].y),
        fmt_gv_double(pts[0].x),
        fmt_gv_double(-pts[0].y),
        fmt_gv_double(pts[1].x),
        fmt_gv_double(-pts[1].y),
        fmt_gv_double(pts[1].x),
        fmt_gv_double(-pts[1].y),
        fmt_gv_double(pts[2].x),
        fmt_gv_double(-pts[2].y),
        fmt_gv_double(pts[3].x),
        fmt_gv_double(-pts[3].y),
        fmt_gv_double(pts[4].x),
        fmt_gv_double(-pts[4].y),
        fmt_gv_double(pts[4].x),
        fmt_gv_double(-pts[4].y),
        fmt_gv_double(pts[5].x),
        fmt_gv_double(-pts[5].y),
        fmt_gv_double(pts[5].x),
        fmt_gv_double(-pts[5].y),
        fmt_gv_double(pts[6].x),
        fmt_gv_double(-pts[6].y),
        fmt_gv_double(pts[7].x),
        fmt_gv_double(-pts[7].y),
        fmt_gv_double(pts[8].x),
        fmt_gv_double(-pts[8].y),
        fmt_gv_double(pts[8].x),
        fmt_gv_double(-pts[8].y),
        fmt_gv_double(pts[9].x),
        fmt_gv_double(-pts[9].y),
        fmt_gv_double(pts[9].x),
        fmt_gv_double(-pts[9].y),
        fmt_gv_double(pts[10].x),
        fmt_gv_double(-pts[10].y),
        fmt_gv_double(pts[11].x),
        fmt_gv_double(-pts[11].y),
        fmt_gv_double(pts[12].x),
        fmt_gv_double(-pts[12].y),
        fmt_gv_double(pts[12].x),
        fmt_gv_double(-pts[12].y),
        fmt_gv_double(pts[13].x),
        fmt_gv_double(-pts[13].y),
        fmt_gv_double(pts[13].x),
        fmt_gv_double(-pts[13].y),
        fmt_gv_double(pts[14].x),
        fmt_gv_double(-pts[14].y),
        fmt_gv_double(pts[15].x),
        fmt_gv_double(-pts[15].y),
        fmt_gv_double(pts[16].x),
        fmt_gv_double(-pts[16].y),
    );
}

fn svg_begin_anchor(
    out: &mut String,
    id: Option<&str>,
    href: Option<&str>,
    tooltip: Option<&str>,
    target: Option<&str>,
) {
    out.push_str("<g");
    if let Some(id) = id {
        let _ = write!(out, " id=\"a_{}\"", xml_escape(id, true, true, false));
    }
    out.push_str("><a");
    if let Some(href) = href
        && !href.is_empty()
    {
        let _ = write!(
            out,
            " xlink:href=\"{}\"",
            xml_escape(href, false, false, false)
        );
    }
    if let Some(tooltip) = tooltip
        && !tooltip.is_empty()
    {
        let _ = write!(
            out,
            " xlink:title=\"{}\"",
            xml_escape(tooltip, true, true, true)
        );
    }
    if let Some(target) = target
        && !target.is_empty()
    {
        let _ = write!(out, " target=\"{}\"", xml_escape(target, true, true, false));
    }
    out.push_str(">\n");
}

fn svg_end_anchor(out: &mut String) {
    out.push_str("</a>\n</g>\n");
}

/// Emits `<text>` spans for a measured multiline label, matching `emit_label` in `labels.c`
/// and `svg_textspan` in `gvrender_core_svg.c`.
#[allow(clippy::too_many_arguments)]
fn emit_text_label(
    out: &mut String,
    layout: &TextLabelLayout,
    pos: Point,
    font_color: &str,
    scheme: Option<&str>,
    force_bold: bool,
    force_italic: bool,
    force_underline: bool,
) {
    if layout.lines.is_empty() {
        return;
    }

    let font_size = layout.font_size;
    let mut py = pos.y + layout.height / 2.0 - font_size;

    let (fill_str, fill_opacity) = format_paint_and_opacity(Some(font_color), "black", scheme);
    let alias = translate_postscript_fontname(&layout.font_name);

    for line in &layout.lines {
        let (anchor, px) = match line.justify {
            TextJustify::Left => ("start", pos.x - layout.width / 2.0),
            TextJustify::Right => ("end", pos.x + layout.width / 2.0),
            TextJustify::Center => ("middle", pos.x),
        };
        let span_y = py + 0.1 * font_size;

        out.push_str("<text xml:space=\"preserve\"");
        let _ = write!(
            out,
            " text-anchor=\"{anchor}\" x=\"{}\" y=\"{}\"",
            fmt_gv_double(px),
            fmt_gv_double(-span_y)
        );

        let mut weight_emitted = false;
        let mut style_emitted = false;
        if let Some(p_alias) = alias {
            let _ = write!(out, " font-family=\"{}", p_alias.family);
            if p_alias.svg_font_family != p_alias.family {
                let _ = write!(out, ",{}", p_alias.svg_font_family);
            }
            out.push('"');
            if let Some(w) = p_alias.weight {
                let _ = write!(out, " font-weight=\"{w}\"");
                weight_emitted = true;
            }
            if let Some(st) = p_alias.stretch {
                let _ = write!(out, " font-stretch=\"{st}\"");
            }
            if let Some(s) = p_alias.style {
                let _ = write!(out, " font-style=\"{s}\"");
                style_emitted = true;
            }
        } else {
            let _ = write!(
                out,
                " font-family=\"{}\"",
                xml_escape(&layout.font_name, false, false, false)
            );
        }

        if force_bold && !weight_emitted {
            out.push_str(" font-weight=\"bold\"");
        }
        if force_italic && !style_emitted {
            out.push_str(" font-style=\"italic\"");
        }
        if force_underline {
            out.push_str(" text-decoration=\"underline\"");
        }

        let _ = write!(out, " font-size=\"{font_size:.2}\"");

        if !fill_str.eq_ignore_ascii_case("black") {
            let _ = write!(out, " fill=\"{fill_str}\"");
            if let Some(op) = fill_opacity {
                let _ = write!(out, " fill-opacity=\"{op:.6}\"");
            }
        }
        out.push('>');
        out.push_str(&xml_escape(&line.text, true, true, true));
        out.push_str("</text>\n");

        py -= line.height;
    }
}

#[cfg(test)]
mod tests {
    use super::format_paint_and_opacity;

    #[test]
    fn paint_follows_gvrender_resolve_color() {
        let paint = |s, scheme| format_paint_and_opacity(Some(s), "lightgrey", scheme).0;
        assert_eq!(paint("/blues9/5", None), "#6baed6");
        assert_eq!(paint("5", Some("blues9")), "#6baed6");
        assert_eq!(paint("Red", None), "Red");
        assert_eq!(paint("red", Some("blues9")), "red");
        assert_eq!(paint("transparent", None), "none");
        assert_eq!(paint("nosuchcolor", None), "#000000");
        assert_eq!(
            format_paint_and_opacity(None, "lightgrey", None).0,
            "lightgrey"
        );
    }
}
