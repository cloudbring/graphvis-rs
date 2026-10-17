//! Layout attributes written back to the graph: port of `attach_attrs_and_arrows`,
//! `set_record_rects` and `rec_attach_bb` in `lib/common/output.c`, and `write_plain`.

use super::model::Model;
use super::num::fmt_g;
use gv_core::{Graph, Point};
use gv_geom::shapes::{RecordField, ShapeKind};
use std::collections::BTreeSet;
use std::fmt::Write;

fn g5(v: f64) -> String {
    fmt_g(v, 5)
}

fn pt(p: Point) -> String {
    format!("{},{}", g5(p.x), g5(p.y))
}

fn record_rects(f: &RecordField, c: Point, out: &mut String) {
    if f.children.is_empty() {
        let _ = write!(
            out,
            "{},{},{},{} ",
            g5(f.bbox.ll.x + c.x),
            g5(f.bbox.ll.y + c.y),
            g5(f.bbox.ur.x + c.x),
            g5(f.bbox.ur.y + c.y)
        );
    }
    for sub in &f.children {
        record_rects(sub, c, out);
    }
}

/// `attach_attrs_and_arrows`: returns a copy of the graph with `pos`, `width`, `height`,
/// `rects`, `bb`, `lp`, `lwidth`, `lheight`, `xlp`, `head_lp`, `tail_lp` set, and whether any
/// edge has start or end arrows.
pub(crate) fn attach_attrs(m: &Model<'_>) -> (Graph, bool, bool, Vec<BTreeSet<String>>) {
    let mut g = m.g.clone();
    g.graph_attrs.retain(|k, _| !k.starts_with('\u{1}'));
    for sg in &mut g.subgraphs {
        sg.graph_attrs.remove(super::model::LABEL_POS_KEY);
    }
    let (mut s_arrows, mut e_arrows) = (false, false);
    for (i, n) in m.nodes.iter().enumerate() {
        let a = &mut g.nodes[i].attrs;
        if a.remove(super::model::CLUST_NODE_KEY).is_some() {
            continue;
        }
        a.insert("pos".into(), pt(n.coord));
        a.insert("height".into(), g5(n.ht / 72.0));
        a.insert("width".into(), g5((n.lw + n.rw) / 72.0));
        if let Some(xl) = n.xlabel.as_ref().filter(|l| l.set) {
            a.insert("xlp".into(), pt(xl.pos));
        }
        if n.geom.shape == ShapeKind::Record
            && let Some(f) = &n.geom.record
        {
            let mut s = String::new();
            record_rects(f, n.coord, &mut s);
            s.pop();
            a.insert("rects".into(), s);
        }
    }
    for (i, e) in m.edges.iter().enumerate() {
        if e.bz.is_empty() {
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
        a.insert("pos".into(), s);
        if let Some(l) = &e.label {
            a.insert("lp".into(), pt(l.pos));
        }
        if let Some(l) = e.xlabel.as_ref().filter(|l| l.set) {
            a.insert("xlp".into(), pt(l.pos));
        }
        if let Some(l) = &e.head_label {
            a.insert("head_lp".into(), pt(l.pos));
        }
        if let Some(l) = &e.tail_label {
            a.insert("tail_lp".into(), pt(l.pos));
        }
    }
    // rec_attach_bb
    let bbs = |b: gv_core::BoundingBox| {
        format!(
            "{},{},{},{}",
            g5(b.ll.x),
            g5(b.ll.y),
            g5(b.ur.x),
            g5(b.ur.y)
        )
    };
    g.graph_attrs.insert("bb".into(), bbs(m.bb));
    if let Some(l) = m.label.as_ref().filter(|l| !l.text.is_empty()) {
        g.graph_attrs.insert("lp".into(), pt(l.pos));
        g.graph_attrs
            .insert("lwidth".into(), format!("{:.2}", l.dimen.x / 72.0));
        g.graph_attrs
            .insert("lheight".into(), format!("{:.2}", l.dimen.y / 72.0));
    }
    let mut explicit: Vec<BTreeSet<String>> = vec![BTreeSet::new(); g.subgraphs.len()];
    for c in &m.clusters {
        let a = &mut g.subgraphs[c.sub].graph_attrs;
        a.insert("bb".into(), bbs(c.bb));
        explicit[c.sub].insert("bb".into());
        if let Some(l) = c.label.as_ref().filter(|l| !l.text.is_empty()) {
            a.insert("lp".into(), pt(l.pos));
            a.insert("lwidth".into(), format!("{:.2}", l.dimen.x / 72.0));
            a.insert("lheight".into(), format!("{:.2}", l.dimen.y / 72.0));
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
                let pfinal = match parent {
                    None => g.graph_attrs.get(k).cloned(),
                    Some(p) => g.subgraphs[p].graph_attrs.get(k).cloned(),
                };
                match pfinal {
                    Some(v) => {
                        g.subgraphs[s].graph_attrs.insert(k.into(), v);
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
    use gv_parser::{canon_str, canon_value};
    let g = m.g;
    let mut out = String::new();
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
            canon_value(a.get("label").unwrap_or(""), &g.html_strings)
        } else {
            canon_str(&n.label.text)
        };
        let fill = a
            .nonempty("fillcolor")
            .unwrap_or_else(|| a.late_nnstring("color", "lightgrey"));
        let _ = writeln!(
            out,
            "node {} {} {} {} {} {} {} {} {} {}",
            canon_str(n.name),
            g5(n.coord.x / 72.0),
            g5(n.coord.y / 72.0),
            g5(n.geom.width),
            g5(n.geom.height),
            lbl,
            a.late_nnstring("style", "solid"),
            {
                let sh = a.late_nnstring("shape", "ellipse");
                if sh != "epsf" && a.nonempty("shapefile").is_some() {
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
                for (node, port) in [(e.tail, &tport), (e.head, &hport)] {
                    out.push(' ');
                    out.push_str(&canon_str(m.nodes[node].name));
                    if let Some(p) = port.as_deref().filter(|p| !p.is_empty()) {
                        out.push(':');
                        out.push_str(&canon_str(p));
                    }
                }
                let npts: usize = e.bz.iter().map(|b| b.list.len()).sum();
                let _ = write!(out, " {npts}");
                for b in &e.bz {
                    for p in &b.list {
                        let _ = write!(out, " {} {}", g5(p.x / 72.0), g5(p.y / 72.0));
                    }
                }
            }
            if let Some(l) = &e.label {
                let _ = write!(
                    out,
                    " {} {} {}",
                    canon_str(&l.text),
                    g5(l.pos.x / 72.0),
                    g5(l.pos.y / 72.0)
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

fn port_string(p: &gv_core::Port) -> Option<String> {
    match (&p.name, &p.compass) {
        (Some(n), Some(c)) => Some(format!("{n}:{c}")),
        (Some(n), None) => Some(n.clone()),
        (None, Some(c)) => Some(c.clone()),
        (None, None) => None,
    }
}
