//! `-Tjson` and `-Tjson0`: port of `plugin/core/gvrender_core_json.c`, including the xdot
//! parser of `lib/xdot/xdot.c` used to expand `_draw_` attributes.

use gv_core::{AttrMap, Graph, GraphKind};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

/// `stoj`.
fn stoj(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '/' => out.push_str("\\/"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn indent(out: &mut String, level: i32) {
    for _ in 0..level.max(0) {
        out.push_str("  ");
    }
}

fn is_xdot(name: &str) -> bool {
    matches!(
        name,
        "_draw_" | "_ldraw_" | "_hdraw_" | "_tdraw_" | "_hldraw_" | "_tldraw_"
    )
}

// ---------------------------------------------------------------------------------------------
// xdot parsing (`parseXDot`)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Grad {
    None(String),
    Linear(f64, f64, f64, f64, Vec<(f64, String)>),
    Radial(f64, f64, f64, f64, f64, f64, Vec<(f64, String)>),
}

#[derive(Debug, Clone)]
enum Op {
    Ellipse(bool, [f64; 4]),
    Polygon(bool, Vec<(f64, f64)>),
    Bezier(bool, Vec<(f64, f64)>),
    Polyline(Vec<(f64, f64)>),
    Text(f64, f64, i64, f64, String),
    Color(bool, Grad),
    Font(f64, String),
    Style(String),
    Image,
    FontChar(u64),
}

struct P<'a> {
    s: &'a [u8],
    i: usize,
}

impl P<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }
    fn real(&mut self) -> Option<f64> {
        let rest = std::str::from_utf8(&self.s[self.i..]).ok()?;
        let (v, r) = crate::emit::attrs::strtod(rest)?;
        self.i = self.s.len() - r.len();
        Some(v)
    }
    fn int(&mut self) -> Option<i64> {
        let rest = std::str::from_utf8(&self.s[self.i..]).ok()?;
        let (v, r) = crate::emit::attrs::strtol(rest)?;
        self.i = self.s.len() - r.len();
        Some(v)
    }
    fn string(&mut self) -> Option<String> {
        let n = self.int()?;
        if n <= 0 {
            return None;
        }
        while self.i < self.s.len() && self.s[self.i] != b'-' {
            self.i += 1;
        }
        if self.i >= self.s.len() {
            return None;
        }
        self.i += 1;
        let start = self.i;
        let mut accounted = 0;
        let mut j = 0usize;
        while accounted < n {
            let c = *self.s.get(start + j)?;
            if c != b'\\' || (j > 0 && self.s[start + j - 1] == b'\\') {
                accounted += 1;
            }
            j += 1;
        }
        // count characters, not bytes: extend over UTF-8 continuation bytes
        let mut end = start + j;
        while end < self.s.len() && (self.s[end] & 0xC0) == 0x80 {
            end += 1;
        }
        self.i = end;
        Some(String::from_utf8_lossy(&self.s[start..end]).into_owned())
    }
    fn polyline(&mut self) -> Option<Vec<(f64, f64)>> {
        let n = self.int()?;
        if n < 0 {
            return None;
        }
        let mut v = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let x = self.real()?;
            let y = self.real()?;
            v.push((x, y));
        }
        Some(v)
    }
}

fn parse_color(s: &str) -> Option<Grad> {
    let b = s.as_bytes();
    let stops = |p: &mut P| -> Option<Vec<(f64, String)>> {
        let n = p.int()?;
        let mut v = Vec::new();
        for _ in 0..n {
            let f = p.real()?;
            let c = p.string()?;
            v.push((f, c));
        }
        Some(v)
    };
    match b.first() {
        Some(b'[') => {
            let mut p = P { s: b, i: 1 };
            let (x0, y0, x1, y1) = (p.real()?, p.real()?, p.real()?, p.real()?);
            Some(Grad::Linear(x0, y0, x1, y1, stops(&mut p)?))
        }
        Some(b'(') => {
            let mut p = P { s: b, i: 1 };
            let (x0, y0, r0, x1, y1, r1) = (
                p.real()?,
                p.real()?,
                p.real()?,
                p.real()?,
                p.real()?,
                p.real()?,
            );
            Some(Grad::Radial(x0, y0, r0, x1, y1, r1, stops(&mut p)?))
        }
        _ => Some(Grad::None(s.to_string())),
    }
}

fn parse_xdot(s: &str) -> Vec<Op> {
    let mut p = P {
        s: s.as_bytes(),
        i: 0,
    };
    let mut ops = Vec::new();
    loop {
        p.ws();
        let Some(&c) = p.s.get(p.i) else { break };
        p.i += 1;
        let op = match c {
            b'E' | b'e' => (|| Some([p.real()?, p.real()?, p.real()?, p.real()?]))()
                .map(|r| Op::Ellipse(c == b'E', r)),
            b'P' | b'p' => p.polyline().map(|v| Op::Polygon(c == b'P', v)),
            b'b' | b'B' => p.polyline().map(|v| Op::Bezier(c == b'b', v)),
            b'L' => p.polyline().map(Op::Polyline),
            b'c' | b'C' => p
                .string()
                .and_then(|cs| parse_color(&cs))
                .map(|g| Op::Color(c == b'C', g)),
            b'T' => (|| {
                let x = p.real()?;
                let y = p.real()?;
                let a = p.int()?;
                let w = p.real()?;
                let t = p.string()?;
                Some(Op::Text(x, y, a, w, t))
            })(),
            b'F' => (|| Some(Op::Font(p.real()?, p.string()?)))(),
            b'S' => p.string().map(Op::Style),
            b'I' => (|| {
                p.real()?;
                p.real()?;
                p.real()?;
                p.real()?;
                p.string()?;
                Some(Op::Image)
            })(),
            b't' => p.int().map(|v| Op::FontChar(v as u64)),
            _ => None,
        };
        match op {
            Some(o) => ops.push(o),
            None => break,
        }
    }
    ops
}

fn write_polyline(out: &mut String, pts: &[(f64, f64)]) {
    out.push_str("\"points\": [");
    for (i, (x, y)) in pts.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "[{x:.3},{y:.3}]");
    }
    out.push_str("]\n");
}

fn write_stops(out: &mut String, stops: &[(f64, String)]) {
    out.push_str("\"stops\": [");
    for (i, (f, c)) in stops.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let _ = write!(out, "{{\"frac\": {f:.3}, \"color\": ");
        stoj(c, out);
        out.push('}');
    }
    out.push_str("]\n");
}

fn write_op(out: &mut String, op: &Op, level: &mut i32) {
    indent(out, *level);
    *level += 1;
    out.push_str("{\n");
    indent(out, *level);
    let l = *level;
    match op {
        Op::Ellipse(f, r) => {
            let _ = writeln!(out, "\"op\": \"{}\",", if *f { 'E' } else { 'e' });
            indent(out, l);
            let _ = writeln!(
                out,
                "\"rect\": [{:.3},{:.3},{:.3},{:.3}]",
                r[0], r[1], r[2], r[3]
            );
        }
        Op::Polygon(f, v) => {
            let _ = writeln!(out, "\"op\": \"{}\",", if *f { 'P' } else { 'p' });
            indent(out, l);
            write_polyline(out, v);
        }
        Op::Bezier(f, v) => {
            let _ = writeln!(out, "\"op\": \"{}\",", if *f { 'B' } else { 'b' });
            indent(out, l);
            write_polyline(out, v);
        }
        Op::Polyline(v) => {
            out.push_str("\"op\": \"L\",\n");
            indent(out, l);
            write_polyline(out, v);
        }
        Op::Text(x, y, a, w, t) => {
            out.push_str("\"op\": \"T\",\n");
            indent(out, l);
            let _ = writeln!(out, "\"pt\": [{x:.3},{y:.3}],");
            indent(out, l);
            let al = if *a < 0 {
                'l'
            } else if *a > 0 {
                'r'
            } else {
                'c'
            };
            let _ = writeln!(out, "\"align\": \"{al}\",");
            indent(out, l);
            let _ = writeln!(out, "\"width\": {w:.3},");
            indent(out, l);
            out.push_str("\"text\": ");
            stoj(t, out);
            out.push('\n');
        }
        Op::Color(fill, g) => {
            let _ = writeln!(out, "\"op\": \"{}\",", if *fill { 'C' } else { 'c' });
            indent(out, l);
            match g {
                Grad::None(c) => {
                    out.push_str("\"grad\": \"none\",\n");
                    indent(out, l);
                    out.push_str("\"color\": ");
                    stoj(c, out);
                    out.push('\n');
                }
                Grad::Linear(x0, y0, x1, y1, st) => {
                    out.push_str("\"grad\": \"linear\",\n");
                    indent(out, l);
                    let _ = writeln!(out, "\"p0\": [{x0:.3},{y0:.3}],");
                    indent(out, l);
                    let _ = writeln!(out, "\"p1\": [{x1:.3},{y1:.3}],");
                    indent(out, l);
                    write_stops(out, st);
                }
                Grad::Radial(x0, y0, r0, x1, y1, r1, st) => {
                    out.push_str("\"grad\": \"radial\",\n");
                    indent(out, l);
                    let _ = writeln!(out, "\"p0\": [{x0:.3},{y0:.3},{r0:.3}],");
                    indent(out, l);
                    let _ = writeln!(out, "\"p1\": [{x1:.3},{y1:.3},{r1:.3}],");
                    indent(out, l);
                    write_stops(out, st);
                }
            }
        }
        Op::Font(sz, f) => {
            out.push_str("\"op\": \"F\",\n");
            indent(out, l);
            let _ = writeln!(out, "\"size\": {sz:.3},");
            indent(out, l);
            out.push_str("\"face\": ");
            stoj(f, out);
            out.push('\n');
        }
        Op::Style(s) => {
            out.push_str("\"op\": \"S\",\n");
            indent(out, l);
            out.push_str("\"style\": ");
            stoj(s, out);
            out.push('\n');
        }
        Op::Image => {}
        Op::FontChar(v) => {
            out.push_str("\"op\": \"t\",\n");
            indent(out, l);
            let _ = writeln!(out, "\"fontchar\": {v}");
        }
    }
    *level -= 1;
    indent(out, *level);
    out.push('}');
}

fn write_xdots(out: &mut String, val: &str, level: &mut i32) {
    let ops = parse_xdot(val);
    if ops.is_empty() {
        out.push('"');
        out.push('"');
        return;
    }
    out.push('\n');
    indent(out, *level);
    *level += 1;
    out.push_str("[\n");
    for (i, op) in ops.iter().enumerate() {
        if i > 0 {
            out.push_str(",\n");
        }
        write_op(out, op, level);
    }
    *level -= 1;
    out.push('\n');
    indent(out, *level);
    out.push(']');
}

// ---------------------------------------------------------------------------------------------
// graph writer
// ---------------------------------------------------------------------------------------------

struct W<'g> {
    g: &'g Graph,
    do_xdot: bool,
    level: i32,
    out: String,
    graph_dict: Vec<String>,
    node_dict: Vec<String>,
    edge_dict: Vec<String>,
    edge_attrs: Vec<AttrMap>,
    sub_gid: Vec<usize>,
    children: Vec<Vec<usize>>,
    top: Vec<usize>,
    node_gid: Vec<usize>,
    edge_gid: Vec<usize>,
}

impl W<'_> {
    fn write_attrs(&mut self, attrs: &AttrMap, dict_kind: u8) {
        let dict = match dict_kind {
            0 => self.graph_dict.clone(),
            1 => self.node_dict.clone(),
            _ => self.edge_dict.clone(),
        };
        for name in &dict {
            let v = attrs.get(name).map(String::as_str).unwrap_or("");
            if v.is_empty() && name != "label" {
                continue;
            }
            self.out.push_str(",\n");
            indent(&mut self.out, self.level);
            stoj(name, &mut self.out);
            self.out.push_str(": ");
            if self.do_xdot && is_xdot(name) {
                let mut level = self.level;
                write_xdots(&mut self.out, v, &mut level);
            } else {
                stoj(v, &mut self.out);
            }
        }
    }

    fn write_graph(&mut self, sub: Option<usize>) {
        let g = self.g;
        let top = sub.is_none();
        indent(&mut self.out, self.level);
        self.level += 1;
        self.out.push_str("{\n");
        indent(&mut self.out, self.level);
        self.out.push_str("\"name\": ");
        let name = sub.map_or(g.name.as_str(), |s| g.subgraphs[s].name.as_str());
        stoj(name, &mut self.out);
        if top {
            self.out.push_str(",\n");
            indent(&mut self.out, self.level);
            let _ = writeln!(self.out, "\"directed\": {},", g.kind == GraphKind::Directed);
            indent(&mut self.out, self.level);
            let _ = write!(self.out, "\"strict\": {}", g.strict);
        }
        let attrs = sub.map_or(&g.graph_attrs, |s| &g.subgraphs[s].graph_attrs);
        self.write_attrs(attrs, 0);
        self.out.push_str(",\n");
        indent(&mut self.out, self.level);
        if top {
            let _ = write!(self.out, "\"_subgraph_cnt\": {}", g.subgraphs.len());
        } else {
            let _ = write!(self.out, "\"_gvid\": {}", self.sub_gid[sub.unwrap()]);
        }
        // write_subgs
        let subs: Vec<usize> = match sub {
            None => self.top.clone(),
            Some(s) => self.children[s].clone(),
        };
        let has_subgs = !subs.is_empty();
        if has_subgs {
            self.out.push_str(",\n");
            indent(&mut self.out, self.level);
            self.level += 1;
            if top {
                self.out.push_str("\"objects\": [\n");
            } else {
                self.out.push_str("\"subgraphs\": [\n");
                indent(&mut self.out, self.level);
            }
            for (i, s) in subs.iter().enumerate() {
                if i > 0 {
                    self.out.push_str(",\n");
                }
                if top {
                    self.write_subg(*s);
                } else {
                    let _ = write!(self.out, "{}", self.sub_gid[*s]);
                }
            }
            if !top {
                self.level -= 1;
                self.out.push('\n');
                indent(&mut self.out, self.level);
                self.out.push(']');
            }
        }
        // write_nodes
        let nodes: Vec<usize> = match sub {
            None => (0..g.nodes.len()).collect(),
            Some(s) => {
                let mut v: Vec<usize> = g.subgraphs[s].nodes.iter().map(|n| n.0 as usize).collect();
                v.sort_unstable();
                v.dedup();
                v
            }
        };
        if nodes.is_empty() {
            if has_subgs && top {
                self.level -= 1;
                self.out.push('\n');
                indent(&mut self.out, self.level);
                self.out.push(']');
            }
        } else {
            self.out.push_str(",\n");
            if top {
                if !has_subgs {
                    indent(&mut self.out, self.level);
                    self.level += 1;
                    self.out.push_str("\"objects\": [\n");
                }
            } else {
                indent(&mut self.out, self.level);
                self.level += 1;
                self.out.push_str("\"nodes\": [\n");
                indent(&mut self.out, self.level);
            }
            for (i, &n) in nodes.iter().enumerate() {
                if i > 0 {
                    self.out.push_str(if top { ",\n" } else { "," });
                }
                if top {
                    indent(&mut self.out, self.level);
                    self.level += 1;
                    self.out.push_str("{\n");
                    indent(&mut self.out, self.level);
                    let _ = writeln!(self.out, "\"_gvid\": {},", self.node_gid[n]);
                    indent(&mut self.out, self.level);
                    self.out.push_str("\"name\": ");
                    stoj(&g.nodes[n].name, &mut self.out);
                    self.write_attrs(&g.nodes[n].attrs, 1);
                    self.out.push('\n');
                    self.level -= 1;
                    indent(&mut self.out, self.level);
                    self.out.push('}');
                } else {
                    let _ = write!(self.out, "{}", self.node_gid[n]);
                }
            }
            self.level -= 1;
            self.out.push('\n');
            indent(&mut self.out, self.level);
            self.out.push(']');
        }
        // write_edges
        let mut edges: Vec<usize> = match sub {
            None => (0..g.edges.len()).collect(),
            Some(s) => g.subgraphs[s].edges.iter().map(|e| e.0 as usize).collect(),
        };
        edges.sort_unstable();
        edges.dedup();
        if !edges.is_empty() {
            self.out.push_str(",\n");
            indent(&mut self.out, self.level);
            self.level += 1;
            self.out.push_str("\"edges\": [\n");
            if !top {
                indent(&mut self.out, self.level);
            }
            for (i, &e) in edges.iter().enumerate() {
                if i > 0 {
                    self.out.push_str(if top { ",\n" } else { "," });
                }
                if top {
                    indent(&mut self.out, self.level);
                    self.level += 1;
                    self.out.push_str("{\n");
                    indent(&mut self.out, self.level);
                    let _ = writeln!(self.out, "\"_gvid\": {},", self.edge_gid[e]);
                    indent(&mut self.out, self.level);
                    let _ = writeln!(
                        self.out,
                        "\"tail\": {},",
                        self.node_gid[g.edges[e].tail.0 as usize]
                    );
                    indent(&mut self.out, self.level);
                    let _ = write!(
                        self.out,
                        "\"head\": {}",
                        self.node_gid[g.edges[e].head.0 as usize]
                    );
                    let a = self.edge_attrs[e].clone();
                    self.write_attrs(&a, 2);
                    self.out.push('\n');
                    self.level -= 1;
                    indent(&mut self.out, self.level);
                    self.out.push('}');
                } else {
                    let _ = write!(self.out, "{}", self.edge_gid[e]);
                }
            }
            self.level -= 1;
            self.out.push('\n');
            indent(&mut self.out, self.level);
            self.out.push(']');
        }
        self.out.push('\n');
        self.level -= 1;
        indent(&mut self.out, self.level);
        self.out.push_str(if top { "}\n" } else { "}" });
    }

    fn write_subg(&mut self, s: usize) {
        self.write_graph(Some(s));
        for c in self.children[s].clone() {
            self.out.push_str(",\n");
            self.write_subg(c);
        }
    }
}

/// Edge attributes including `tailport`/`headport` from `a:p -> b` syntax.
fn edge_attrs_with_ports(g: &Graph) -> Vec<AttrMap> {
    g.edges
        .iter()
        .map(|e| {
            let mut a = e.attrs.clone();
            let port = |p: &gv_core::Port| -> Option<String> {
                match (&p.name, &p.compass) {
                    (Some(n), Some(c)) => Some(format!("{n}:{c}")),
                    (Some(n), None) => Some(n.clone()),
                    (None, Some(c)) => Some(c.clone()),
                    (None, None) => None,
                }
            };
            if let Some(t) = port(&e.tail_port) {
                a.entry("tailport".into()).or_insert(t);
            }
            if let Some(h) = port(&e.head_port) {
                a.entry("headport".into()).or_insert(h);
            }
            a
        })
        .collect()
}

/// Writes `g` (with its layout attributes already attached) as JSON. `edge_order` gives the
/// `agfstnode`/`agfstout` iteration order used to number edges.
pub(crate) fn write_json(g: &Graph, do_xdot: bool, edge_order: &[usize]) -> String {
    let edge_attrs = edge_attrs_with_ports(g);
    let mut gd: BTreeSet<String> = g.graph_attrs.keys().cloned().collect();
    for s in &g.subgraphs {
        gd.extend(s.graph_attrs.keys().cloned());
    }
    let mut nd: BTreeSet<String> = g.node_defaults.keys().cloned().collect();
    for n in &g.nodes {
        nd.extend(n.attrs.keys().cloned());
    }
    let mut ed: BTreeSet<String> = g.edge_defaults.keys().cloned().collect();
    for a in &edge_attrs {
        ed.extend(a.keys().cloned());
    }
    let mut is_child = vec![false; g.subgraphs.len()];
    for s in &g.subgraphs {
        for c in &s.subgraphs {
            is_child[c.0 as usize] = true;
        }
    }
    let top: Vec<usize> = (0..g.subgraphs.len()).filter(|&i| !is_child[i]).collect();
    let children: Vec<Vec<usize>> = g
        .subgraphs
        .iter()
        .map(|s| s.subgraphs.iter().map(|c| c.0 as usize).collect())
        .collect();
    // label_subgs: preorder numbering
    let mut sub_gid = vec![0usize; g.subgraphs.len()];
    let mut cluster_gid: BTreeMap<&str, usize> = BTreeMap::new();
    let mut cnt = 0;
    fn label(
        s: usize,
        cnt: &mut usize,
        gid: &mut [usize],
        children: &[Vec<usize>],
        g: &Graph,
        cl: &mut BTreeMap<String, usize>,
    ) {
        gid[s] = *cnt;
        let name = &g.subgraphs[s].name;
        if name.len() >= 7 && name[..7].eq_ignore_ascii_case("cluster") {
            cl.entry(name.clone()).or_insert(*cnt);
        }
        *cnt += 1;
        for &c in &children[s] {
            label(c, cnt, gid, children, g, cl);
        }
    }
    let mut cl: BTreeMap<String, usize> = BTreeMap::new();
    for &s in &top {
        label(s, &mut cnt, &mut sub_gid, &children, g, &mut cl);
    }
    let _ = &mut cluster_gid;
    let node_gid: Vec<usize> = (0..g.nodes.len()).map(|i| cnt + i).collect();
    let mut edge_gid = vec![0usize; g.edges.len()];
    for (k, &e) in edge_order.iter().enumerate() {
        edge_gid[e] = k;
    }
    let mut w = W {
        g,
        do_xdot,
        level: 0,
        out: String::new(),
        graph_dict: gd.into_iter().collect(),
        node_dict: nd.into_iter().collect(),
        edge_dict: ed.into_iter().collect(),
        edge_attrs,
        sub_gid,
        children,
        top,
        node_gid,
        edge_gid,
    };
    w.write_graph(None);
    w.out
}
