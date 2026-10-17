//! Positioned DOT (`-Tdot`), extended XDOT (`-Txdot`), and plain text (`-Tplain`) renderers
//! ported from `plugin/core/gvrender_core_dot.c` and `lib/common/output.c`.

use crate::svg::{
    RecordField, arrow_direction_at_end, arrow_direction_at_start, compute_normal_arrow_polygon,
    edge_arrow_directions, effective_cluster_bb, effective_graph_bb, layout_record_fields,
    parse_f64_attr, parse_record_fields, parse_style,
};
use gv_core::font::{
    DEFAULT_FONT_NAME, DEFAULT_FONT_SIZE, LINESPACING, LabelSubstContext, TextJustify,
    TextLabelLayout, measure_multiline_label_with_context, substitute_object_escapes,
};
use gv_core::{
    BoundingBox, EdgeData, Graph, GraphKind, NodeData, Point, RgbaColor, Spline, SubgraphData,
    resolve_color_with_scheme,
};
use gv_geom::straight_spline;
use gv_parser::{canon_str, canon_value, write_canon};
use std::fmt::Write;

const POINTS_PER_INCH: f64 = 72.0;

/// Formats a floating-point value similar to C `%.5g` (up to 5 significant digits, no trailing zeros).
pub(crate) fn fmt_g5(v: f64) -> String {
    if !v.is_finite() || v.abs() < 1e-9 {
        return "0".to_string();
    }
    // Round to 5 significant digits
    let abs_v = v.abs();
    let log10 = abs_v.log10().floor() as i32;
    let decimals = (4 - log10).clamp(0, 6) as usize;
    let s = format!("{v:.decimals$}");
    trim_trailing_zeros(&s)
}

/// Formats a floating-point value for xdot operations (`%.02f` with trailing zeros and decimal point stripped).
pub(crate) fn fmt_xdot_num(v: f64) -> String {
    if !v.is_finite() || v.abs() < 0.005 {
        return "0".to_string();
    }
    let s = format!("{v:.2}");
    trim_trailing_zeros(&s)
}

fn trim_trailing_zeros(s: &str) -> String {
    if !s.contains('.') {
        return if s == "-0" {
            "0".to_string()
        } else {
            s.to_string()
        };
    }
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-0" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

/// A label position the layout stored in `lp` (`x,y`).
fn layout_lp(attrs: &gv_core::AttrMap) -> Option<Point> {
    let (x, y) = attrs.get("lp")?.split_once(',')?;
    Some(Point::new(x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// Formats an edge spline into Graphviz `pos` attribute syntax (`"e,ex,ey x0,y0 x1,y1 ..."`).
pub(crate) fn format_edge_pos(_graph: &Graph, edge: &EdgeData) -> String {
    // An edge without a spline has no `pos`, as in C (e.g. the non-leader
    // edges of `concentrate=true` in neato).
    if edge.spline.segments.is_empty() {
        return String::new();
    }
    let spline = &edge.spline;

    // Layouts record arrowheads per Bézier (`sflag`/`eflag`, as C); a
    // segment without them has no `s,`/`e,` point.
    let mut out = String::new();
    for (seg_idx, seg) in spline.segments.iter().enumerate() {
        if seg.points.is_empty() {
            continue;
        }
        if seg_idx > 0 {
            out.push(';');
        }
        if let Some(sp) = seg.start_arrow {
            let _ = write!(out, "s,{},{} ", fmt_g5(sp.x), fmt_g5(sp.y));
        }
        if let Some(ep) = seg.end_arrow {
            let _ = write!(out, "e,{},{} ", fmt_g5(ep.x), fmt_g5(ep.y));
        }
        for (i, pt) in seg.points.iter().enumerate() {
            if i > 0 {
                out.push(' ');
            }
            let _ = write!(out, "{},{}", fmt_g5(pt.x), fmt_g5(pt.y));
        }
    }
    out
}

/// Populates layout position/bounding-box attributes (and xdot `_draw_` attributes when `xdot` is true)
/// onto a cloned [`Graph`], matching `attach_attrs_and_arrows` in `lib/common/output.c`.
pub(crate) fn build_positioned_graph(graph: &Graph, xdot: bool) -> Graph {
    let mut g = graph.clone();
    let bb = effective_graph_bb(&g);
    g.bb = bb;

    let root_scheme = g.graph_attrs.get("colorscheme").cloned();

    // 1. Root graph `bb` and label attributes
    g.graph_attrs.insert(
        "bb".to_string(),
        format!(
            "{},{},{},{}",
            fmt_g5(bb.ll.x),
            fmt_g5(bb.ll.y),
            fmt_g5(bb.ur.x),
            fmt_g5(bb.ur.y)
        ),
    );
    if xdot {
        let version = g
            .graph_attrs
            .get("xdotversion")
            .cloned()
            .unwrap_or_else(|| "1.7".to_string());
        g.graph_attrs.insert("xdotversion".to_string(), version);
        let bg_spec = g.graph_attrs.get("bgcolor").map(String::as_str);
        let bg_hex = xdot_color_hex(bg_spec, RgbaColor::WHITE, root_scheme.as_deref());
        let mut gdraw = String::new();
        push_xdot_str(&mut gdraw, "c ", "#fffffe00");
        push_xdot_str(&mut gdraw, "C ", &bg_hex);
        let _ = write!(
            gdraw,
            "P 4 {} {} {} {} {} {} {} {} ",
            fmt_xdot_num(bb.ll.x),
            fmt_xdot_num(bb.ll.y),
            fmt_xdot_num(bb.ll.x),
            fmt_xdot_num(bb.ur.y),
            fmt_xdot_num(bb.ur.x),
            fmt_xdot_num(bb.ur.y),
            fmt_xdot_num(bb.ur.x),
            fmt_xdot_num(bb.ll.y),
        );
        g.graph_attrs.insert("_draw_".to_string(), gdraw);
    }

    if let Some(g_label) = g.graph_attrs.get("label").cloned()
        && !g_label.is_empty()
    {
        let font_name = g
            .graph_attrs
            .get("fontname")
            .map(String::as_str)
            .unwrap_or(DEFAULT_FONT_NAME)
            .to_string();
        let font_size = parse_f64_attr(&g.graph_attrs, "fontsize", DEFAULT_FONT_SIZE);
        let font_color = g
            .graph_attrs
            .get("fontcolor")
            .map(String::as_str)
            .unwrap_or("black")
            .to_string();
        let ctx = LabelSubstContext {
            graph_name: &g.name,
            label_text: &g_label,
            is_directed: g.kind == GraphKind::Directed,
            ..Default::default()
        };
        let layout = measure_multiline_label_with_context(&g_label, &ctx, &font_name, font_size);
        // Layouts that place the root label (dot's `place_root_label`) leave it in `lp`.
        let lp = layout_lp(&g.graph_attrs).unwrap_or(Point::new(
            (bb.ll.x + bb.ur.x) / 2.0,
            bb.ll.y + layout.height / 2.0 + 4.0,
        ));
        g.graph_attrs.insert(
            "lp".to_string(),
            format!("{},{}", fmt_g5(lp.x), fmt_g5(lp.y)),
        );
        g.graph_attrs.insert(
            "lwidth".to_string(),
            format!("{:.2}", layout.width / POINTS_PER_INCH),
        );
        g.graph_attrs.insert(
            "lheight".to_string(),
            format!("{:.2}", layout.height / POINTS_PER_INCH),
        );
        if xdot {
            let ldraw = xdot_label_ops(&layout, lp, &font_color, root_scheme.as_deref());
            if !ldraw.is_empty() {
                g.graph_attrs.insert("_ldraw_".to_string(), ldraw);
            }
        }
    }

    // 2. Clusters (`subgraphs`)
    for idx in 0..g.subgraphs.len() {
        // Layouts only keep clusters with nodes (`collapse_cluster` skips
        // empty ones), so those get no `bb`.
        let is_clust = (g.subgraphs[idx].is_cluster
            || g.subgraphs[idx].name.starts_with("cluster"))
            && !g.subgraphs[idx].nodes.is_empty();
        if !is_clust {
            continue;
        }
        // A cluster the layout gave no box (e.g. a nested cluster in neato,
        // which only registers top-level clusters) gets no `bb`, as in C.
        if g.subgraphs[idx].bb == BoundingBox::default() && !g.subgraphs[idx].nodes.is_empty() {
            continue;
        }
        let cbb = effective_cluster_bb(&g, &g.subgraphs[idx]);
        let subg = &mut g.subgraphs[idx];
        subg.bb = cbb;
        subg.graph_attrs.insert(
            "bb".to_string(),
            format!(
                "{},{},{},{}",
                fmt_g5(cbb.ll.x),
                fmt_g5(cbb.ll.y),
                fmt_g5(cbb.ur.x),
                fmt_g5(cbb.ur.y)
            ),
        );

        let scheme = subg
            .graph_attrs
            .get("colorscheme")
            .cloned()
            .or_else(|| root_scheme.clone());

        if let Some(c_label) = subg.graph_attrs.get("label").cloned()
            && !c_label.is_empty()
        {
            let font_name = subg
                .graph_attrs
                .get("fontname")
                .map(String::as_str)
                .unwrap_or(DEFAULT_FONT_NAME)
                .to_string();
            let font_size = parse_f64_attr(&subg.graph_attrs, "fontsize", DEFAULT_FONT_SIZE);
            let font_color = subg
                .graph_attrs
                .get("fontcolor")
                .map(String::as_str)
                .unwrap_or("black")
                .to_string();
            let ctx = LabelSubstContext {
                graph_name: &subg.name,
                label_text: &c_label,
                is_directed: g.kind == GraphKind::Directed,
                ..Default::default()
            };
            let layout =
                measure_multiline_label_with_context(&c_label, &ctx, &font_name, font_size);
            let lp = layout_lp(&subg.graph_attrs).unwrap_or(Point::new(
                (cbb.ll.x + cbb.ur.x) / 2.0,
                cbb.ur.y - layout.height / 2.0 - 4.0,
            ));
            subg.graph_attrs.insert(
                "lp".to_string(),
                format!("{},{}", fmt_g5(lp.x), fmt_g5(lp.y)),
            );
            subg.graph_attrs.insert(
                "lwidth".to_string(),
                format!("{:.2}", layout.width / POINTS_PER_INCH),
            );
            subg.graph_attrs.insert(
                "lheight".to_string(),
                format!("{:.2}", layout.height / POINTS_PER_INCH),
            );
            if xdot {
                let ldraw = xdot_label_ops(&layout, lp, &font_color, scheme.as_deref());
                if !ldraw.is_empty() {
                    subg.graph_attrs.insert("_ldraw_".to_string(), ldraw);
                }
            }
        }

        if xdot {
            let cdraw = xdot_cluster_ops(subg, cbb, scheme.as_deref());
            if !cdraw.is_empty() {
                subg.graph_attrs.insert("_draw_".to_string(), cdraw);
            }
        }
    }

    // 3. Nodes
    let graph_name = g.name.clone();
    let is_directed = g.kind == GraphKind::Directed;
    let rankdir = g
        .graph_attrs
        .get("rankdir")
        .cloned()
        .unwrap_or_else(|| "TB".to_string());

    for node in &mut g.nodes {
        node.attrs.insert(
            "pos".to_string(),
            format!("{},{}", fmt_g5(node.pos.x), fmt_g5(node.pos.y)),
        );
        node.attrs.insert("width".to_string(), fmt_g5(node.width));
        node.attrs.insert("height".to_string(), fmt_g5(node.height));

        let shape = node
            .attrs
            .get("shape")
            .map(String::as_str)
            .unwrap_or("ellipse")
            .to_ascii_lowercase();
        let rx = (node.width * POINTS_PER_INCH / 2.0).max(0.5);
        let ry = (node.height * POINTS_PER_INCH / 2.0).max(0.5);
        let nbb = BoundingBox::new(
            node.pos.x - rx,
            node.pos.y - ry,
            node.pos.x + rx,
            node.pos.y + ry,
        );

        let raw_label = node
            .attrs
            .get("label")
            .cloned()
            .unwrap_or_else(|| "\\N".to_string());
        let subst_ctx = LabelSubstContext {
            node_name: &node.name,
            graph_name: &graph_name,
            is_directed,
            label_text: &raw_label,
            ..Default::default()
        };

        if shape == "record" || shape == "mrecord" {
            let root_horizontal = !matches!(rankdir.as_str(), "LR" | "RL");
            let substituted = substitute_object_escapes(&raw_label, &subst_ctx);
            let mut rf = parse_record_fields(&substituted, root_horizontal);
            layout_record_fields(&mut rf, nbb);
            let mut rects = Vec::new();
            collect_record_rects(&rf, &mut rects);
            if !rects.is_empty() {
                node.attrs.insert("rects".to_string(), rects.join(" "));
            }
        }

        if xdot {
            let scheme = node
                .attrs
                .get("colorscheme")
                .cloned()
                .or_else(|| root_scheme.clone());
            let ndraw = xdot_node_ops(node, nbb, &shape, scheme.as_deref());
            if !ndraw.is_empty() {
                node.attrs.insert("_draw_".to_string(), ndraw);
            }
            if !raw_label.is_empty() && shape != "point" {
                let font_name = node
                    .attrs
                    .get("fontname")
                    .map(String::as_str)
                    .unwrap_or(DEFAULT_FONT_NAME)
                    .to_string();
                let font_size = parse_f64_attr(&node.attrs, "fontsize", DEFAULT_FONT_SIZE);
                let font_color = node
                    .attrs
                    .get("fontcolor")
                    .map(String::as_str)
                    .unwrap_or("black")
                    .to_string();
                let layout = measure_multiline_label_with_context(
                    &raw_label, &subst_ctx, &font_name, font_size,
                );
                let ldraw = xdot_label_ops(&layout, node.pos, &font_color, scheme.as_deref());
                if !ldraw.is_empty() {
                    node.attrs.insert("_ldraw_".to_string(), ldraw);
                }
            }
        }
    }

    // 4. Edges
    let g_ref = g.clone();
    for edge in &mut g.edges {
        let pos_str = format_edge_pos(&g_ref, edge);
        if !pos_str.is_empty() {
            edge.attrs.insert("pos".to_string(), pos_str);
        }

        let scheme = edge
            .attrs
            .get("colorscheme")
            .cloned()
            .or_else(|| root_scheme.clone());

        let fallback_spl;
        let spline: &Spline = if edge.spline.segments.is_empty() {
            let p_tail = g_ref
                .nodes
                .get(edge.tail.0 as usize)
                .map(|n| n.pos)
                .unwrap_or_default();
            let p_head = g_ref
                .nodes
                .get(edge.head.0 as usize)
                .map(|n| n.pos)
                .unwrap_or_default();
            fallback_spl = straight_spline(p_tail, p_head);
            &fallback_spl
        } else {
            &edge.spline
        };

        let tail_name = g_ref
            .nodes
            .get(edge.tail.0 as usize)
            .map(|n| n.name.as_str())
            .unwrap_or("");
        let head_name = g_ref
            .nodes
            .get(edge.head.0 as usize)
            .map(|n| n.name.as_str())
            .unwrap_or("");
        let tail_port_owned = edge.tail_port.name.clone();
        let head_port_owned = edge.head_port.name.clone();
        let label_text_owned = edge.attrs.get("label").cloned().unwrap_or_default();
        let subst_ctx = LabelSubstContext {
            node_name: "",
            graph_name: &graph_name,
            tail_name,
            head_name,
            is_edge: true,
            is_directed,
            tail_port: tail_port_owned.as_deref(),
            head_port: head_port_owned.as_deref(),
            label_text: &label_text_owned,
        };

        // C writes `lp` only for edges that have splines.
        if let Some(e_label) = edge.attrs.get("label").cloned()
            && !e_label.is_empty()
            && !edge.spline.segments.is_empty()
        {
            let lp = edge.label_pos.unwrap_or_else(|| {
                spline
                    .segments
                    .first()
                    .and_then(|s| s.points.get(s.points.len() / 2).copied())
                    .unwrap_or_default()
            });
            edge.attrs.insert(
                "lp".to_string(),
                format!("{},{}", fmt_g5(lp.x), fmt_g5(lp.y)),
            );

            if xdot {
                let font_name = edge
                    .attrs
                    .get("fontname")
                    .map(String::as_str)
                    .unwrap_or(DEFAULT_FONT_NAME)
                    .to_string();
                let font_size = parse_f64_attr(&edge.attrs, "fontsize", DEFAULT_FONT_SIZE);
                let font_color = edge
                    .attrs
                    .get("fontcolor")
                    .map(String::as_str)
                    .unwrap_or("black")
                    .to_string();
                let layout = measure_multiline_label_with_context(
                    &e_label, &subst_ctx, &font_name, font_size,
                );
                let ldraw = xdot_label_ops(&layout, lp, &font_color, scheme.as_deref());
                if !ldraw.is_empty() {
                    edge.attrs.insert("_ldraw_".to_string(), ldraw);
                }
            }
        }

        if xdot {
            let (edraw, hdraw, tdraw) = xdot_edge_ops(&g_ref, edge, spline, scheme.as_deref());
            if !edraw.is_empty() {
                edge.attrs.insert("_draw_".to_string(), edraw);
            }
            if let Some(hd) = hdraw {
                edge.attrs.insert("_hdraw_".to_string(), hd);
            }
            if let Some(td) = tdraw {
                edge.attrs.insert("_tdraw_".to_string(), td);
            }
        }
    }

    g
}

fn collect_record_rects(field: &RecordField, out: &mut Vec<String>) {
    if field.children.is_empty() {
        out.push(format!(
            "{},{},{},{}",
            fmt_g5(field.bb.ll.x),
            fmt_g5(field.bb.ll.y),
            fmt_g5(field.bb.ur.x),
            fmt_g5(field.bb.ur.y)
        ));
    } else {
        for child in &field.children {
            collect_record_rects(child, out);
        }
    }
}

fn xdot_color_hex(spec: Option<&str>, default: RgbaColor, scheme: Option<&str>) -> String {
    let rgba = match spec.and_then(|s| s.split(':').next()).map(str::trim) {
        Some(s) if !s.is_empty() => resolve_color_with_scheme(s, scheme).unwrap_or_else(|| {
            gv_core::warn_unknown_color(s);
            RgbaColor::BLACK
        }),
        _ => default,
    };
    if rgba.a == 255 {
        rgba.to_hex_rgb()
    } else {
        rgba.to_hex_rgba()
    }
}

fn push_xdot_str(out: &mut String, prefix: &str, val: &str) {
    let _ = write!(out, "{prefix}{} -{val} ", val.len());
}

fn xdot_cluster_ops(subg: &SubgraphData, cbb: BoundingBox, scheme: Option<&str>) -> String {
    let style = parse_style(subg.graph_attrs.get("style").map(String::as_str));
    if style.invis {
        return String::new();
    }
    let mut out = String::new();
    let stroke_spec = subg
        .graph_attrs
        .get("pencolor")
        .or_else(|| subg.graph_attrs.get("color"))
        .map(String::as_str);
    let pen_hex = xdot_color_hex(stroke_spec, RgbaColor::BLACK, scheme);
    push_xdot_str(&mut out, "c ", &pen_hex);

    let is_filled = style.filled || subg.graph_attrs.contains_key("bgcolor");
    let op = if is_filled {
        let fill_spec = subg
            .graph_attrs
            .get("fillcolor")
            .or_else(|| subg.graph_attrs.get("bgcolor"))
            .map(String::as_str)
            .or(stroke_spec);
        let fill_hex = xdot_color_hex(fill_spec, RgbaColor::rgb(211, 211, 211), scheme);
        push_xdot_str(&mut out, "C ", &fill_hex);
        'P'
    } else {
        'p'
    };

    let _ = write!(
        out,
        "{op} 4 {} {} {} {} {} {} {} {} ",
        fmt_xdot_num(cbb.ll.x),
        fmt_xdot_num(cbb.ll.y),
        fmt_xdot_num(cbb.ll.x),
        fmt_xdot_num(cbb.ur.y),
        fmt_xdot_num(cbb.ur.x),
        fmt_xdot_num(cbb.ur.y),
        fmt_xdot_num(cbb.ur.x),
        fmt_xdot_num(cbb.ll.y),
    );
    out.trim_end().to_string()
}

fn xdot_node_ops(node: &NodeData, nbb: BoundingBox, shape: &str, scheme: Option<&str>) -> String {
    let style = parse_style(node.attrs.get("style").map(String::as_str));
    if style.invis || matches!(shape, "none" | "plaintext" | "plain") {
        return String::new();
    }
    let mut out = String::new();
    let stroke_spec = node
        .attrs
        .get("pencolor")
        .or_else(|| node.attrs.get("color"))
        .map(String::as_str);
    let pen_hex = xdot_color_hex(stroke_spec, RgbaColor::BLACK, scheme);
    push_xdot_str(&mut out, "c ", &pen_hex);

    let is_filled = style.filled || shape == "point";
    if is_filled {
        let fill_spec = node
            .attrs
            .get("fillcolor")
            .map(String::as_str)
            .or(stroke_spec);
        let dflt = if shape == "point" {
            RgbaColor::BLACK
        } else {
            RgbaColor::rgb(211, 211, 211)
        };
        let fill_hex = xdot_color_hex(fill_spec, dflt, scheme);
        push_xdot_str(&mut out, "C ", &fill_hex);
    }

    let rx = (nbb.width() / 2.0).max(0.5);
    let ry = (nbb.height() / 2.0).max(0.5);
    match shape {
        "box" | "rect" | "rectangle" | "square" | "msquare" | "record" | "mrecord" => {
            let op = if is_filled { 'P' } else { 'p' };
            let _ = write!(
                out,
                "{op} 4 {} {} {} {} {} {} {} {} ",
                fmt_xdot_num(nbb.ur.x),
                fmt_xdot_num(nbb.ur.y),
                fmt_xdot_num(nbb.ll.x),
                fmt_xdot_num(nbb.ur.y),
                fmt_xdot_num(nbb.ll.x),
                fmt_xdot_num(nbb.ll.y),
                fmt_xdot_num(nbb.ur.x),
                fmt_xdot_num(nbb.ll.y),
            );
        }
        _ => {
            let op = if is_filled { 'E' } else { 'e' };
            let _ = write!(
                out,
                "{op} {} {} {} {} ",
                fmt_xdot_num(node.pos.x),
                fmt_xdot_num(node.pos.y),
                fmt_xdot_num(rx),
                fmt_xdot_num(ry),
            );
        }
    }
    out.trim_end().to_string()
}

fn xdot_edge_ops(
    graph: &Graph,
    edge: &EdgeData,
    spline: &Spline,
    scheme: Option<&str>,
) -> (String, Option<String>, Option<String>) {
    let style = parse_style(edge.attrs.get("style").map(String::as_str));
    if style.invis {
        return (String::new(), None, None);
    }
    let color_spec = edge.attrs.get("color").map(String::as_str);
    let pen_hex = xdot_color_hex(color_spec, RgbaColor::BLACK, scheme);
    let fill_spec = edge
        .attrs
        .get("fillcolor")
        .map(String::as_str)
        .or(color_spec);
    let fill_hex = xdot_color_hex(fill_spec, RgbaColor::BLACK, scheme);
    let arrowsize = parse_f64_attr(&edge.attrs, "arrowsize", 1.0).max(0.0);
    let penwidth = parse_f64_attr(&edge.attrs, "penwidth", 1.0);

    let mut edraw = String::new();
    push_xdot_str(&mut edraw, "c ", &pen_hex);
    for seg in &spline.segments {
        if seg.points.is_empty() {
            continue;
        }
        let _ = write!(edraw, "B {} ", seg.points.len());
        for pt in &seg.points {
            let _ = write!(edraw, "{} {} ", fmt_xdot_num(pt.x), fmt_xdot_num(pt.y));
        }
    }

    let (want_s, want_e) = edge_arrow_directions(graph, edge);
    let mut hdraw = None;
    let mut tdraw = None;

    if let Some(seg) = spline.segments.last()
        && !seg.points.is_empty()
        && (seg.end_arrow.is_some() || want_e)
        && arrowsize > 0.0
    {
        let base = *seg.points.last().unwrap_or(&seg.points[0]);
        let tip = seg.end_arrow.unwrap_or(base);
        let dir = arrow_direction_at_end(&seg.points, tip);
        let tri = compute_normal_arrow_polygon(base, tip, dir, arrowsize, penwidth, false);
        let mut hd = String::new();
        push_xdot_str(&mut hd, "S ", "solid");
        push_xdot_str(&mut hd, "c ", &pen_hex);
        push_xdot_str(&mut hd, "C ", &fill_hex);
        let _ = write!(
            hd,
            "P 3 {} {} {} {} {} {} ",
            fmt_xdot_num(tri[0].x),
            fmt_xdot_num(tri[0].y),
            fmt_xdot_num(tri[1].x),
            fmt_xdot_num(tri[1].y),
            fmt_xdot_num(tri[2].x),
            fmt_xdot_num(tri[2].y),
        );
        hdraw = Some(hd.trim_end().to_string());
    }

    if let Some(seg) = spline.segments.first()
        && !seg.points.is_empty()
        && (seg.start_arrow.is_some() || want_s)
        && arrowsize > 0.0
    {
        let base = seg.points[0];
        let tip = seg.start_arrow.unwrap_or(base);
        let dir = arrow_direction_at_start(&seg.points, tip);
        let tri = compute_normal_arrow_polygon(base, tip, dir, arrowsize, penwidth, false);
        let mut td = String::new();
        push_xdot_str(&mut td, "S ", "solid");
        push_xdot_str(&mut td, "c ", &pen_hex);
        push_xdot_str(&mut td, "C ", &fill_hex);
        let _ = write!(
            td,
            "P 3 {} {} {} {} {} {} ",
            fmt_xdot_num(tri[0].x),
            fmt_xdot_num(tri[0].y),
            fmt_xdot_num(tri[1].x),
            fmt_xdot_num(tri[1].y),
            fmt_xdot_num(tri[2].x),
            fmt_xdot_num(tri[2].y),
        );
        tdraw = Some(td.trim_end().to_string());
    }

    (edraw.trim_end().to_string(), hdraw, tdraw)
}

fn xdot_label_ops(
    layout: &TextLabelLayout,
    pos: Point,
    font_color: &str,
    scheme: Option<&str>,
) -> String {
    if layout.lines.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let pen_hex = xdot_color_hex(Some(font_color), RgbaColor::BLACK, scheme);
    let font_size = layout.font_size;
    let mut py = pos.y + layout.height / 2.0 - font_size;

    for line in &layout.lines {
        let _ = write!(out, "F {} ", fmt_xdot_num(font_size));
        push_xdot_str(&mut out, "", &layout.font_name);
        push_xdot_str(&mut out, "c ", &pen_hex);

        let (j, px) = match line.justify {
            TextJustify::Left => (-1, pos.x - layout.width / 2.0),
            TextJustify::Right => (1, pos.x + layout.width / 2.0),
            TextJustify::Center => (0, pos.x),
        };
        let span_y = py + 0.1 * font_size;
        let _ = write!(
            out,
            "T {} {} {j} {} ",
            fmt_xdot_num(px),
            fmt_xdot_num(span_y),
            fmt_xdot_num(line.width)
        );
        push_xdot_str(&mut out, "", &line.text);
        py -= line.height;
    }

    out.trim_end().to_string()
}

/// Renders a positioned DOT (`xdot = false`) or extended XDOT (`xdot = true`) string.
pub fn render_dot_positioned(graph: &Graph, xdot: bool) -> String {
    let positioned = build_positioned_graph(graph, xdot);
    write_canon(&positioned)
}

/// Renders a [`Graph`] into Graphviz plain-text format (`-Tplain`), matching `write_plain`
/// in `lib/common/output.c`.
pub fn render_plain(graph: &Graph) -> String {
    let mut out = String::with_capacity(512);
    let bb = effective_graph_bb(graph);

    let _ = writeln!(
        out,
        "graph 1 {} {}",
        fmt_g5(bb.ur.x / POINTS_PER_INCH),
        fmt_g5(bb.ur.y / POINTS_PER_INCH)
    );

    for node in &graph.nodes {
        let name = canon_str(&node.name);
        let x_in = fmt_g5(node.pos.x / POINTS_PER_INCH);
        let y_in = fmt_g5(node.pos.y / POINTS_PER_INCH);
        let w_in = fmt_g5(node.width);
        let h_in = fmt_g5(node.height);

        let raw_label = node.attrs.get("label").map(String::as_str).unwrap_or("\\N");
        let label_str = if graph.html_strings.contains(raw_label) {
            canon_value(raw_label, &graph.html_strings)
        } else {
            let ctx = LabelSubstContext {
                node_name: &node.name,
                graph_name: &graph.name,
                is_directed: graph.kind == GraphKind::Directed,
                label_text: raw_label,
                ..Default::default()
            };
            canon_str(&substitute_object_escapes(raw_label, &ctx))
        };

        let style = node
            .attrs
            .get("style")
            .filter(|s| !s.is_empty())
            .map(String::as_str)
            .unwrap_or("solid");
        let shape = node
            .attrs
            .get("shape")
            .filter(|s| !s.is_empty())
            .map(String::as_str)
            .unwrap_or("ellipse");
        let color = node
            .attrs
            .get("color")
            .filter(|s| !s.is_empty())
            .map(String::as_str)
            .unwrap_or("black");
        let fillcolor = node
            .attrs
            .get("fillcolor")
            .filter(|s| !s.is_empty())
            .or_else(|| node.attrs.get("color").filter(|s| !s.is_empty()))
            .map(String::as_str)
            .unwrap_or("lightgrey");

        let _ = writeln!(
            out,
            "node {name} {x_in} {y_in} {w_in} {h_in} {label_str} {style} {shape} {color} {fillcolor}"
        );
    }

    for edge in &graph.edges {
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
        let tail_canon = canon_str(tail_name);
        let head_canon = canon_str(head_name);

        let fallback;
        let spline = if edge.spline.segments.is_empty() {
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
            fallback = straight_spline(p_tail, p_head);
            &fallback
        } else {
            &edge.spline
        };

        let total_pts: usize = spline.segments.iter().map(|s| s.points.len()).sum();
        let _ = write!(out, "edge {tail_canon} {head_canon} {total_pts}");
        for seg in &spline.segments {
            for pt in &seg.points {
                let _ = write!(
                    out,
                    " {} {}",
                    fmt_g5(pt.x / POINTS_PER_INCH),
                    fmt_g5(pt.y / POINTS_PER_INCH)
                );
            }
        }

        if let Some(raw_label) = edge.attrs.get("label")
            && !raw_label.is_empty()
        {
            let ctx = LabelSubstContext {
                node_name: "",
                graph_name: &graph.name,
                tail_name,
                head_name,
                is_edge: true,
                is_directed: graph.kind == GraphKind::Directed,
                tail_port: edge.tail_port.name.as_deref(),
                head_port: edge.head_port.name.as_deref(),
                label_text: raw_label,
            };
            let resolved = substitute_object_escapes(raw_label, &ctx);
            let lp = edge.label_pos.unwrap_or_else(|| {
                spline
                    .segments
                    .first()
                    .and_then(|s| s.points.get(s.points.len() / 2).copied())
                    .unwrap_or_default()
            });
            let _ = write!(
                out,
                " {} {} {}",
                canon_str(&resolved),
                fmt_g5(lp.x / POINTS_PER_INCH),
                fmt_g5(lp.y / POINTS_PER_INCH)
            );
        }

        let style = edge
            .attrs
            .get("style")
            .filter(|s| !s.is_empty())
            .map(String::as_str)
            .unwrap_or("solid");
        let color = edge
            .attrs
            .get("color")
            .filter(|s| !s.is_empty())
            .map(String::as_str)
            .unwrap_or("black");
        let _ = writeln!(out, " {style} {color}");
    }

    out.push_str("stop\n");
    let _ = LINESPACING;
    out
}
