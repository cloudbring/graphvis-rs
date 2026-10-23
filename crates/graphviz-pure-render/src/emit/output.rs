//! Layout attributes written back to the graph: port of `attach_attrs_and_arrows`,
//! `set_record_rects` and `rec_attach_bb` in `lib/common/output.c`, and `write_plain`.

use super::model::Model;
use super::num::fmt_g;
use super::{y_dir, y_offset};
use graphviz_pure_core::{Graph, HTML_VALUE_PREFIX, Point, attr_is_html, set_attr};
use graphviz_pure_geom::shapes::{RecordField, ShapeKind};
use std::collections::BTreeSet;
use std::fmt::Write;

fn g5(v: f64) -> String {
    fmt_g(v, 5)
}

/// `"%.5g,%.5g"` of `p.x, yDir(p.y, y_off)`.
fn pt(p: Point, y_off: f64) -> String {
    format!("{},{}", g5(p.x), g5(y_dir(p.y, y_off)))
}

fn record_rects(f: &RecordField, c: Point, y_off: f64, out: &mut String) {
    if f.children.is_empty() {
        let _ = write!(
            out,
            "{},{},{},{} ",
            g5(f.bbox.ll.x + c.x),
            g5(y_dir(f.bbox.ll.y + c.y, y_off)),
            g5(f.bbox.ur.x + c.x),
            g5(y_dir(f.bbox.ur.y + c.y, y_off))
        );
    }
    for sub in &f.children {
        record_rects(sub, c, y_off, out);
    }
}

/// `attach_attrs_and_arrows`: returns a copy of the graph with `pos`, `width`, `height`,
/// `rects`, `bb`, `lp`, `lwidth`, `lheight`, `xlp`, `head_lp`, `tail_lp` set, and whether any
/// edge has start or end arrows.
pub(crate) fn attach_attrs(m: &Model<'_>) -> (Graph, bool, bool, Vec<BTreeSet<String>>) {
    let mut g = m.g.clone();
    // (the writers read the HTML marks)
    let private = |k: &str| k.starts_with('\u{1}') && !k.starts_with(HTML_VALUE_PREFIX);
    g.graph_attrs.retain(|k, _| !private(k));
    for sg in &mut g.subgraphs {
        sg.graph_attrs.retain(|k, _| !private(k));
    }
    for e in &mut g.edges {
        e.attrs.retain(|k, _| !private(k));
    }
    let (mut s_arrows, mut e_arrows) = (false, false);
    let y_off = y_offset(m.bb);
    let pt = |p: Point| pt(p, y_off);
    for (i, n) in m.nodes.iter().enumerate() {
        let a = &mut g.nodes[i].attrs;
        let clust_node = a.contains_key(super::model::CLUST_NODE_KEY);
        a.retain(|k, _| !private(k));
        if clust_node {
            continue;
        }
        set_attr(a, "pos", pt(n.coord), false);
        set_attr(a, "height", g5(n.ht / 72.0), false);
        set_attr(a, "width", g5((n.lw + n.rw) / 72.0), false);
        if let Some(xl) = n.xlabel.as_ref().filter(|l| l.set) {
            set_attr(a, "xlp", pt(xl.pos), false);
        }
        if n.geom.shape == ShapeKind::Record
            && let Some(f) = &n.geom.record
        {
            let mut s = String::new();
            record_rects(f, n.coord, y_off, &mut s);
            s.pop();
            set_attr(a, "rects", s, false);
        }
    }
    for (i, e) in m.edges.iter().enumerate() {
        if e.bz.is_empty() {
            continue;
        }
        // `if (ED_edge_type(e) == IGNORED) continue;`: the layout's label positions,
        // written for the renderer, give way to the input values.
        let src = &m.g.edges[i].attrs;
        if src.contains_key(graphviz_pure_core::IGNORED_EDGE_KEY) {
            let a = &mut g.edges[i].attrs;
            for (k, ik) in graphviz_pure_core::IGNORED_EDGE_INPUT_KEYS {
                match src.get(ik) {
                    Some(v) => set_attr(a, k, v.clone(), attr_is_html(src, ik)),
                    None => {
                        a.remove(k);
                    }
                };
            }
            continue;
        }
        let mut s = String::new();
        for (k, b) in e.bz.iter().enumerate() {
            if k > 0 {
                s.push(';');
            }
            if b.sflag != 0 {
                s_arrows = true;
                let _ = write!(s, "s,{} ", pt(b.sp));
            }
            if b.eflag != 0 {
                e_arrows = true;
                let _ = write!(s, "e,{} ", pt(b.ep));
            }
            for (j, p) in b.list.iter().enumerate() {
                if j > 0 {
                    s.push(' ');
                }
                s.push_str(&pt(*p));
            }
        }
        let a = &mut g.edges[i].attrs;
        set_attr(a, "pos", s, false);
        if let Some(l) = &e.label {
            set_attr(a, "lp", pt(l.pos), false);
        }
        if let Some(l) = e.xlabel.as_ref().filter(|l| l.set) {
            set_attr(a, "xlp", pt(l.pos), false);
        }
        if let Some(l) = &e.head_label {
            set_attr(a, "head_lp", pt(l.pos), false);
        }
        if let Some(l) = &e.tail_label {
            set_attr(a, "tail_lp", pt(l.pos), false);
        }
    }
    // rec_attach_bb
    let bbs = |b: graphviz_pure_core::BoundingBox| {
        format!(
            "{},{},{},{}",
            g5(b.ll.x),
            g5(y_dir(b.ll.y, y_off)),
            g5(b.ur.x),
            g5(y_dir(b.ur.y, y_off))
        )
    };
    set_attr(&mut g.graph_attrs, "bb", bbs(m.bb), false);
    if let Some(l) = m.label.as_ref().filter(|l| !l.text.is_empty()) {
        set_attr(&mut g.graph_attrs, "lp", pt(l.pos), false);
        set_attr(
            &mut g.graph_attrs,
            "lwidth",
            graphviz_pure_core::cnum::fmt_f(l.dimen.x / 72.0, 2),
            false,
        );
        set_attr(
            &mut g.graph_attrs,
            "lheight",
            graphviz_pure_core::cnum::fmt_f(l.dimen.y / 72.0, 2),
            false,
        );
    }
    let mut explicit: Vec<BTreeSet<String>> = vec![BTreeSet::new(); g.subgraphs.len()];
    for c in &m.clusters {
        let a = &mut g.subgraphs[c.sub].graph_attrs;
        set_attr(a, "bb", bbs(c.bb), false);
        explicit[c.sub].insert("bb".into());
        if let Some(l) = c.label.as_ref().filter(|l| !l.text.is_empty()) {
            set_attr(a, "lp", pt(l.pos), false);
            set_attr(
                a,
                "lwidth",
                graphviz_pure_core::cnum::fmt_f(l.dimen.x / 72.0, 2),
                false,
            );
            set_attr(
                a,
                "lheight",
                graphviz_pure_core::cnum::fmt_f(l.dimen.y / 72.0, 2),
                false,
            );
            for k in ["lp", "lwidth", "lheight"] {
                explicit[c.sub].insert(k.into());
            }
        }
    }
    (g, s_arrows, e_arrows, explicit)
}

/// For `agwrite`: subgraphs that only inherited `bb`, `lp`, `lwidth` or `lheight` follow
/// their parent's new value (cgraph keeps graph attribute values in per-graph dictionaries,
/// so a value set on the root is not a local value of the subgraph).
pub(crate) fn propagate_inherited(g: &mut Graph, orig: &Graph, explicit: &[BTreeSet<String>]) {
    // Subgraphs that only inherited these attributes keep following their parent: cgraph
    // stores graph attribute values in per-graph dictionaries, so a changed root value is
    // not a local value of the subgraph.
    let mut is_child = vec![false; g.subgraphs.len()];
    for sg in &orig.subgraphs {
        for c in &sg.subgraphs {
            is_child[c.0 as usize] = true;
        }
    }
    fn walk(
        g: &mut Graph,
        orig: &Graph,
        s: usize,
        parent: Option<usize>,
        explicit: &[BTreeSet<String>],
    ) {
        for k in ["bb", "lp", "lwidth", "lheight"] {
            if explicit[s].contains(k) {
                continue;
            }
            let porig = match parent {
                None => orig.graph_attrs.get(k),
                Some(p) => orig.subgraphs[p].graph_attrs.get(k),
            };
            if orig.subgraphs[s].graph_attrs.get(k) == porig {
                let pattrs = match parent {
                    None => &g.graph_attrs,
                    Some(p) => &g.subgraphs[p].graph_attrs,
                };
                let pfinal = pattrs.get(k).cloned();
                let html = attr_is_html(pattrs, k);
                match pfinal {
                    Some(v) => {
                        set_attr(&mut g.subgraphs[s].graph_attrs, k, v, html);
                    }
                    None => {
                        g.subgraphs[s].graph_attrs.remove(k);
                    }
                }
            }
        }
        let kids: Vec<usize> = orig.subgraphs[s]
            .subgraphs
            .iter()
            .map(|c| c.0 as usize)
            .collect();
        for c in kids {
            walk(g, orig, c, Some(s), explicit);
        }
    }
    for (s, child) in is_child.iter().enumerate() {
        if !child {
            walk(g, orig, s, None, explicit);
        }
    }
}

/// `write_plain` (`-Tplain`, `-Tplain-ext`).
pub(crate) fn write_plain(m: &Model<'_>, zoom: f64, extend: bool) -> String {
    use graphviz_pure_parser::{canon_str, canon_value};
    let g = m.g;
    let mut out = String::new();
    let y_off = y_offset(m.bb);
    let _ = writeln!(
        out,
        "graph {} {} {}",
        g5(zoom),
        g5(m.bb.ur.x / 72.0),
        g5(m.bb.ur.y / 72.0)
    );
    for (i, n) in m.nodes.iter().enumerate() {
        let a = n.attrs;
        if a.get(super::model::CLUST_NODE_KEY).is_some() {
            continue;
        }
        let lbl = if n.label.is_html {
            canon_value(a.get("label").unwrap_or(""), true)
        } else {
            canon_str(&n.label.text)
        };
        let fill = a
            .nonempty("fillcolor")
            .unwrap_or_else(|| a.late_nnstring("color", "lightgrey"));
        let _ = writeln!(
            out,
            "node {} {} {} {} {} {} {} {} {} {}",
            canon_value(n.name, g.nodes[i].html_name),
            g5(n.coord.x / 72.0),
            g5(y_dir(n.coord.y, y_off) / 72.0),
            g5(n.geom.width),
            g5(n.geom.height),
            lbl,
            a.late_nnstring("style", "solid"),
            {
                let sh = a.late_nnstring("shape", "ellipse");
                if graphviz_pure_core::usershape::binds_custom(sh, a.nonempty("shapefile")) {
                    "custom"
                } else {
                    sh
                }
            },
            a.late_nnstring("color", "black"),
            fill
        );
        let _ = i;
    }
    for n in 0..m.nodes.len() {
        for &ei in &m.out_edges[n] {
            let e = &m.edges[ei];
            let ge = &g.edges[ei];
            let (tport, hport) = if extend {
                (
                    e.attrs
                        .get("tailport")
                        .map(str::to_string)
                        .or_else(|| port_string(&ge.tail_port)),
                    e.attrs
                        .get("headport")
                        .map(str::to_string)
                        .or_else(|| port_string(&ge.head_port)),
                )
            } else {
                (None, None)
            };
            if !e.bz.is_empty() {
                out.push_str("edge");
                // `writenodeandport`: `agcanonStr` of the node name and the port.
                let ports = [
                    (e.tail, &tport, attr_is_html(&ge.attrs, "tailport")),
                    (e.head, &hport, attr_is_html(&ge.attrs, "headport")),
                ];
                for (node, port, html) in ports {
                    out.push(' ');
                    out.push_str(&canon_value(m.nodes[node].name, g.nodes[node].html_name));
                    if let Some(p) = port.as_deref().filter(|p| !p.is_empty()) {
                        out.push(':');
                        out.push_str(&canon_value(p, html));
                    }
                }
                let npts: usize = e.bz.iter().map(|b| b.list.len()).sum();
                let _ = write!(out, " {npts}");
                for b in &e.bz {
                    for p in &b.list {
                        let _ = write!(out, " {} {}", g5(p.x / 72.0), g5(y_dir(p.y, y_off) / 72.0));
                    }
                }
            }
            if let Some(l) = &e.label {
                let _ = write!(
                    out,
                    " {} {} {}",
                    canon_str(&l.text),
                    g5(l.pos.x / 72.0),
                    g5(y_dir(l.pos.y, y_off) / 72.0)
                );
            }
            let _ = writeln!(
                out,
                " {} {}",
                e.attrs.late_nnstring("style", "solid"),
                e.attrs.late_nnstring("color", "black")
            );
        }
    }
    out.push_str("stop\n");
    out
}

fn port_string(p: &graphviz_pure_core::Port) -> Option<String> {
    match (&p.name, &p.compass) {
        (Some(n), Some(c)) => Some(format!("{n}:{c}")),
        (Some(n), None) => Some(n.clone()),
        (None, Some(c)) => Some(c.clone()),
        (None, None) => None,
    }
}
