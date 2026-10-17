//! Port of the drawing traversal in `lib/common/emit.c` (`emit_graph`, `emit_page`,
//! `emit_background`, `emit_view`, `emit_clusters`, `emit_node`, `emit_edge`,
//! `emit_edge_graphics`, `emit_begin_edge`, `emit_end_edge`, `emit_edge_label`,
//! `initObjMapData`, `getObjId`, `parseSegs`, `findStopColor`, `stripedBox`) and of
//! `emit_label` in `lib/common/labels.c`.

use super::attrs::{Attrs, StyleItem, default_line_style, mapbool, parse_style};
use super::job::{
    EMIT_CLUSTERS_LAST, EMIT_EDGE_SORTED, EMIT_SORTED, EmitState, Emitter, Engine, FILL,
    GRADIENT, GVDEVICE_DOES_TRUECOLOR, GVRENDER_DOES_LABELS, GVRENDER_DOES_MAPS,
    GVRENDER_DOES_TARGETS, GVRENDER_DOES_TOOLTIPS, GVRENDER_NO_WHITE_BG, ObjKind, RGRADIENT,
};
use super::model::{Bz, TextLabel};
use gv_core::font::{LabelSubstContext, html_entity_utf8, strdup_and_subst_obj};
use gv_core::{BoundingBox, Point};

pub(crate) const DEFAULT_COLOR: &str = "black";
pub(crate) const DEFAULT_FILL: &str = "lightgrey";

/// A color segment of a `color` list (`colorseg_t`).
#[derive(Debug, Clone)]
pub(crate) struct ColorSeg {
    pub color: Option<String>,
    pub t: f64,
    pub has_fraction: bool,
}

fn aeq0(x: f64) -> bool {
    x < 1e-5 && x > -1e-5
}

/// `parseSegs`: `Err(1|2)` on error, `Ok((segs, rval))` with `rval` 0 or 3.
pub(crate) fn parse_segs(clrs: &str) -> Result<(Vec<ColorSeg>, i32), i32> {
    let mut segs: Vec<ColorSeg> = Vec::new();
    let mut left = 1.0f64;
    let mut rval = 0;
    for tok in clrs.split(':').filter(|t| !t.is_empty()) {
        let (color, v) = match tok.find(';') {
            None => (tok, 0.0),
            Some(i) => {
                let rest = &tok[i + 1..];
                match super::attrs::strtod(rest) {
                    Some((v, _)) if v >= 0.0 => (&tok[..i], v),
                    _ => {
                        eprintln!(
                            "Error: Illegal value in \"{clrs}\" color attribute; float expected after ';'"
                        );
                        return Err(2);
                    }
                }
            }
        };
        let mut v = v;
        let del = v - left;
        if del > 0.0 {
            if !aeq0(del) {
                eprintln!("Warning: Total size > 1 in \"{clrs}\" color spec ");
                rval = 3;
            }
            v = left;
        }
        left -= v;
        segs.push(ColorSeg {
            color: (!color.is_empty()).then(|| color.to_string()),
            t: v,
            has_fraction: v > 0.0,
        });
        if aeq0(left) {
            left = 0.0;
            break;
        }
    }
    if left > 0.0 {
        let nseg = segs.iter().filter(|s| s.t <= 0.0).count();
        if nseg > 0 {
            let delta = left / nseg as f64;
            for s in segs.iter_mut().filter(|s| s.t <= 0.0) {
                s.t = delta;
            }
        } else if let Some(last) = segs.last_mut() {
            last.t += left;
        }
    }
    while segs.last().is_some_and(|s| s.t <= 0.0) {
        segs.pop();
    }
    Ok((segs, rval))
}

/// `findStopColor`: `(clr0, clr1, frac)` for a two-color gradient spec.
pub(crate) fn find_stop_color(colorlist: &str) -> Option<(String, Option<String>, f64)> {
    let (segs, rv) = parse_segs(colorlist).ok()?;
    if rv != 0 || segs.len() < 2 || segs[0].color.is_none() {
        return None;
    }
    if segs.len() > 2 {
        eprintln!("Warning: More than 2 colors specified for a gradient - ignoring remaining");
    }
    let frac = if segs[0].has_fraction {
        segs[0].t
    } else if segs[1].has_fraction {
        1.0 - segs[1].t
    } else {
        0.0
    };
    Some((segs[0].color.clone().unwrap(), segs[1].color.clone(), frac))
}

fn overlap(a: BoundingBox, b: BoundingBox) -> bool {
    a.ll.x <= b.ur.x && a.ur.x >= b.ll.x && a.ll.y <= b.ur.y && a.ur.y >= b.ll.y
}

fn expand(bb: &mut BoundingBox, p: Point) {
    bb.ll.x = bb.ll.x.min(p.x);
    bb.ll.y = bb.ll.y.min(p.y);
    bb.ur.x = bb.ur.x.max(p.x);
    bb.ur.y = bb.ur.y.max(p.y);
}

/// `arrow_bb`.
fn arrow_bb(p: Point, u: Point, arrowsize: f64) -> BoundingBox {
    let mut u = Point::new(u.x - p.x, u.y - p.y);
    let s = 10.0 * arrowsize / (u.x.hypot(u.y) + 0.0001);
    u.x += if u.x >= 0.0 { 0.0001 } else { -0.0001 };
    u.y += if u.y >= 0.0 { 0.0001 } else { -0.0001 };
    u.x *= s;
    u.y *= s;
    let (ux2, uy2) = (u.x / 2.0, u.y / 2.0);
    let (ax, ay) = (p.x - uy2, p.y - ux2);
    let (bx, by) = (p.x + uy2, p.y + ux2);
    let (cx, cy) = (ax + u.x, ay + u.y);
    let (dx, dy) = (bx + u.x, by + u.y);
    BoundingBox::new(
        ax.min(bx.min(cx.min(dx))),
        ay.min(by.min(cy.min(dy))),
        ax.max(bx.max(cx.max(dx))),
        ay.max(by.max(cy.max(dy))),
    )
}

/// `init_splines_bb`.
fn splines_bb(bz: &[Bz]) -> Option<BoundingBox> {
    let first = bz.first()?;
    let bezier_bb = |b: &Bz| {
        let mut bb = BoundingBox {
            ll: b.list[0],
            ur: b.list[0],
        };
        let mut i = 1;
        while i + 2 < b.list.len() + 1 && i + 1 < b.list.len() {
            let (p1, p2) = (b.list[i], b.list[i + 1]);
            expand(&mut bb, Point::new((p1.x + p2.x) / 2.0, (p1.y + p2.y) / 2.0));
            if let Some(p) = b.list.get(i + 2) {
                expand(&mut bb, *p);
            }
            i += 3;
        }
        bb
    };
    if first.list.is_empty() {
        return None;
    }
    let mut bb = bezier_bb(first);
    for (i, b) in bz.iter().enumerate() {
        if b.list.is_empty() {
            continue;
        }
        if i > 0 {
            let x = bezier_bb(b);
            expand(&mut bb, x.ll);
            expand(&mut bb, x.ur);
        }
        if b.sflag != 0 {
            let x = arrow_bb(b.sp, b.list[0], 1.0);
            expand(&mut bb, x.ll);
            expand(&mut bb, x.ur);
        }
        if b.eflag != 0 {
            let x = arrow_bb(b.ep, *b.list.last().unwrap(), 1.0);
            expand(&mut bb, x.ll);
            expand(&mut bb, x.ur);
        }
    }
    Some(bb)
}

fn label_overlaps(l: &TextLabel, b: BoundingBox) -> bool {
    let s = Point::new(l.dimen.x / 2.0, l.dimen.y / 2.0);
    overlap(
        BoundingBox::new(l.pos.x - s.x, l.pos.y - s.y, l.pos.x + s.x, l.pos.y + s.y),
        b,
    )
}

/// `interpretCRNL`.
fn interpret_crnl(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut bs = false;
    for c in s.chars() {
        if bs {
            out.push(match c {
                'n' | 'l' => '\n',
                'r' => '\r',
                c => c,
            });
            bs = false;
        } else if c == '\\' {
            bs = true;
        } else {
            out.push(c);
        }
    }
    out
}

impl<'a, E: Engine> Emitter<'a, E> {
    /// Substitution context for an object.
    pub(crate) fn ctx(&self, kind: ObjKind) -> LabelSubstContext<'a> {
        let m = self.job.m;
        match kind {
            ObjKind::Root => LabelSubstContext {
                graph_name: &m.g.name,
                label_text: m.label.as_ref().map_or("", |l| l.text.as_str()),
                ..Default::default()
            },
            ObjKind::Cluster(c) => LabelSubstContext {
                graph_name: m.clusters[c].name,
                label_text: m.clusters[c].label.as_ref().map_or("", |l| l.text.as_str()),
                ..Default::default()
            },
            ObjKind::Node(n) => LabelSubstContext {
                graph_name: &m.g.name,
                node_name: m.nodes[n].name,
                label_text: &m.nodes[n].label.text,
                ..Default::default()
            },
            ObjKind::Edge(e) => {
                let ed = &m.edges[e];
                LabelSubstContext {
                    graph_name: &m.g.name,
                    tail_name: m.nodes[ed.tail].name,
                    head_name: m.nodes[ed.head].name,
                    is_edge: true,
                    is_directed: m.directed,
                    tail_port: ed.tail_port,
                    head_port: ed.head_port,
                    label_text: ed.label.as_ref().map_or("", |l| l.text.as_str()),
                    ..Default::default()
                }
            }
        }
    }

    pub(crate) fn obj_attrs(&self, kind: ObjKind) -> Attrs<'a> {
        let m = self.job.m;
        match kind {
            ObjKind::Root => m.root_attrs(),
            ObjKind::Cluster(c) => m.clusters[c].attrs,
            ObjKind::Node(n) => m.nodes[n].attrs,
            ObjKind::Edge(e) => m.edges[e].attrs,
        }
    }

    /// `getObjId`.
    fn obj_id(&self, kind: ObjKind) -> String {
        let a = self.obj_attrs(kind);
        if let Some(id) = a.nonempty("id") {
            return id.to_string();
        }
        let mut s = String::new();
        if kind != ObjKind::Root
            && let Some(gid) = self.job.m.root_attrs().nonempty("id")
        {
            s.push_str(gid);
            s.push('_');
        }
        match kind {
            ObjKind::Root => s.push_str("graph0"),
            ObjKind::Cluster(c) => {
                s.push_str(&format!("clust{}", self.job.m.clusters[c].sub + 1));
            }
            ObjKind::Node(n) => s.push_str(&format!("node{}", n + 1)),
            ObjKind::Edge(e) => s.push_str(&format!("edge{}", e + 1)),
        }
        s
    }

    fn preprocess_tooltip(&self, s: &str) -> String {
        interpret_crnl(&html_entity_utf8(s))
    }

    /// `initObjMapData`.
    fn init_obj_map_data(&mut self, label: Option<String>, kind: ObjKind) {
        let a = self.obj_attrs(kind);
        let ctx = self.ctx(kind);
        let url = a.nonempty("href").or_else(|| a.nonempty("URL"));
        let tooltip = a.get("tooltip").map(|t| self.preprocess_tooltip(t));
        let target = a.get("target");
        let id = self.obj_id(kind);
        let flags = self.job.flags;
        let o = self.job.obj_mut();
        if flags & GVRENDER_DOES_LABELS != 0 {
            o.label = label;
        }
        if flags & GVRENDER_DOES_MAPS != 0 {
            o.id = Some(strdup_and_subst_obj(&id, &ctx));
            if let Some(u) = url {
                o.url = Some(strdup_and_subst_obj(u, &ctx));
            }
        }
        if flags & GVRENDER_DOES_TOOLTIPS != 0 {
            if let Some(t) = tooltip.filter(|t| !t.is_empty()) {
                o.tooltip = Some(strdup_and_subst_obj(&t, &ctx));
                o.explicit_tooltip = true;
            } else if let Some(l) = &o.label {
                o.tooltip = Some(l.clone());
            }
        }
        if flags & GVRENDER_DOES_TARGETS != 0
            && let Some(t) = target.filter(|t| !t.is_empty())
        {
            o.target = Some(strdup_and_subst_obj(t, &ctx));
        }
    }

    fn set_scheme(&mut self, s: Option<&str>) -> Option<String> {
        std::mem::replace(&mut self.job.color_scheme, s.map(str::to_string))
    }

    /// `emit_graph`.
    pub(crate) fn emit_graph(&mut self) {
        let m = self.job.m;
        self.eng.begin_job(&self.job);
        let comment = m.root_attrs().get_or_empty("comment");
        // gvrender_comment needs an object state only for engines that look at it
        self.job.push_obj(ObjKind::Root);
        self.job.pop_obj();
        if !comment.is_empty() {
            self.job.push_obj(ObjKind::Root);
            self.eng.comment(&self.job, comment);
            self.job.pop_obj();
        }
        // emit_begin_graph
        self.job.push_obj(ObjKind::Root);
        self.job.obj_mut().emit_state = EmitState::GDraw;
        let label = m.label.as_ref().map(|l| l.text.clone());
        self.init_obj_map_data(label, ObjKind::Root);
        self.eng.begin_graph(&self.job);
        self.emit_page();
        self.eng.end_graph(&self.job);
        self.job.pop_obj();
    }

    fn emit_page(&mut self) {
        let m = self.job.m;
        let prev = self.set_scheme(m.root_attrs().get("colorscheme"));
        self.eng.begin_page(&self.job);
        self.set_pencolor(DEFAULT_COLOR);
        self.set_fillcolor(DEFAULT_FILL);
        let flags = self.job.flags;
        let anchored = {
            let o = self.job.obj();
            o.url.is_some() || o.explicit_tooltip
        };
        if flags & GVRENDER_DOES_LABELS != 0
            && let Some(l) = &m.label
        {
            self.job.obj_mut().label = Some(l.text.clone());
        }
        if flags & EMIT_CLUSTERS_LAST == 0 && anchored {
            self.begin_obj_anchor();
        }
        self.emit_background();
        if let Some(l) = &m.label {
            self.emit_label(EmitState::GLabel, l);
        }
        if flags & EMIT_CLUSTERS_LAST == 0 && anchored {
            self.end_anchor();
        }
        self.emit_view();
        self.eng.end_page(&self.job);
        self.set_scheme(prev.as_deref());
    }

    fn emit_background(&mut self) {
        let m = self.job.m;
        let a = m.root_attrs();
        let (mut s, mut dflt) = match a.nonempty("bgcolor") {
            Some(s) => (s, false),
            None => ("white", true),
        };
        let flags = self.job.flags;
        if flags & GVDEVICE_DOES_TRUECOLOR == 0 && s == "transparent" {
            s = "white";
            dflt = true;
        }
        if !((flags & GVDEVICE_DOES_TRUECOLOR != 0 && s == "transparent")
            || (flags & GVRENDER_NO_WHITE_BG != 0 && dflt))
        {
            let clip = self.job.clip;
            if let Some((c0, c1, frac)) = find_stop_color(s) {
                self.set_fillcolor(&c0);
                self.set_pencolor("transparent");
                let radial = a
                    .nonempty("style")
                    .is_some_and(|st| parse_style(st).iter().any(|i| i.name == "radial"));
                let angle = a.late_int("gradientangle", 0, 0);
                self.set_gradient_vals(c1.as_deref().unwrap_or(DEFAULT_COLOR), angle, frac);
                self.boxf(clip, if radial { RGRADIENT } else { GRADIENT });
            } else {
                self.set_fillcolor(s);
                self.set_pencolor("transparent");
                self.boxf(clip, FILL);
            }
        }
    }

    fn emit_view(&mut self) {
        let m = self.job.m;
        let flags = self.job.flags;
        let mut drawn = vec![false; m.nodes.len()];
        if flags & EMIT_CLUSTERS_LAST == 0 {
            self.emit_clusters(&m.root_clusters);
        }
        if flags & EMIT_SORTED != 0 {
            for n in 0..m.nodes.len() {
                self.emit_node(n, &mut drawn);
            }
            for n in 0..m.nodes.len() {
                for &e in &m.out_edges[n] {
                    self.emit_edge(e);
                }
            }
        } else if flags & EMIT_EDGE_SORTED != 0 {
            for n in 0..m.nodes.len() {
                for &e in &m.out_edges[n] {
                    self.emit_edge(e);
                }
            }
            for n in 0..m.nodes.len() {
                self.emit_node(n, &mut drawn);
            }
        } else {
            for n in 0..m.nodes.len() {
                self.emit_node(n, &mut drawn);
                for &e in &m.out_edges[n] {
                    self.emit_node(m.edges[e].head, &mut drawn);
                    self.emit_edge(e);
                }
            }
        }
    }

    fn emit_clusters(&mut self, list: &[usize]) {
        let m = self.job.m;
        for &c in list {
            let cl = &m.clusters[c];
            let a = cl.attrs;
            self.job.push_obj(ObjKind::Cluster(c));
            self.job.obj_mut().emit_state = EmitState::CDraw;
            self.init_obj_map_data(cl.label.as_ref().map(|l| l.text.clone()), ObjKind::Cluster(c));
            self.eng.begin_cluster(&self.job);
            let anchored = {
                let o = self.job.obj();
                o.url.is_some() || o.explicit_tooltip
            };
            let prev = self.set_scheme(a.get("colorscheme"));
            if anchored {
                self.begin_obj_anchor();
            }
            let mut filled = 0;
            let mut rounded = false;
            let mut striped = false;
            let mut radial = false;
            if let Some(st) = a.nonempty("style") {
                let mut items = parse_style(st);
                let mut kept = Vec::new();
                for it in items.drain(..) {
                    match it.name.as_str() {
                        "filled" => {
                            filled = FILL;
                            kept.push(it);
                        }
                        "radial" => {
                            filled = FILL;
                            radial = true;
                        }
                        "striped" => striped = true,
                        "rounded" => rounded = true,
                        _ => kept.push(it),
                    }
                }
                self.set_style(&kept);
            }
            let mut pencolor: Option<&str> = None;
            let mut fillcolor: Option<&str> = None;
            if let Some(c) = a.nonempty("color") {
                pencolor = Some(c);
                fillcolor = Some(c);
            }
            if let Some(c) = a.nonempty("pencolor") {
                pencolor = Some(c);
            }
            if let Some(c) = a.nonempty("fillcolor") {
                fillcolor = Some(c);
            }
            if (filled == 0 || fillcolor.is_none())
                && let Some(c) = a.nonempty("bgcolor")
            {
                fillcolor = Some(c);
                filled = FILL;
            }
            let pencolor = pencolor.unwrap_or(DEFAULT_COLOR);
            let fillcolor = fillcolor.unwrap_or(DEFAULT_FILL);
            if filled != 0 {
                if let Some((c0, c1, frac)) = find_stop_color(fillcolor) {
                    self.set_fillcolor(&c0);
                    let angle = a.late_int("gradientangle", 0, 0);
                    self.set_gradient_vals(c1.as_deref().unwrap_or(DEFAULT_COLOR), angle, frac);
                    filled = if radial { RGRADIENT } else { GRADIENT };
                } else {
                    self.set_fillcolor(fillcolor);
                }
            }
            if a.nonempty("penwidth").is_some() {
                let pw = a.late_double("penwidth", 1.0, 0.0);
                self.set_penwidth(pw);
            }
            let bb = cl.bb;
            let peripheries = a.late_int("peripheries", 1, 0);
            let af = [
                bb.ll,
                Point::new(bb.ur.x, bb.ll.y),
                bb.ur,
                Point::new(bb.ll.x, bb.ur.y),
            ];
            if rounded {
                if peripheries != 0 || filled != 0 {
                    if peripheries != 0 {
                        self.set_pencolor(pencolor);
                    } else {
                        self.set_pencolor("transparent");
                    }
                    let style = super::shapes::PolyStyle {
                        rounded: true,
                        filled: filled != 0,
                        radial,
                        striped,
                        ..Default::default()
                    };
                    self.round_corners(&af, style, filled);
                }
            } else if striped {
                if peripheries == 0 {
                    self.set_pencolor("transparent");
                } else {
                    self.set_pencolor(pencolor);
                }
                self.striped_box(&af, fillcolor, false);
                self.boxf(bb, 0);
            } else if peripheries != 0 {
                self.set_pencolor(pencolor);
                self.boxf(bb, filled);
            } else if filled != 0 {
                self.set_pencolor("transparent");
                self.boxf(bb, filled);
            }
            if let Some(l) = &cl.label {
                self.emit_label(EmitState::CLabel, l);
            }
            if anchored {
                self.end_anchor();
            }
            self.eng.end_cluster(&self.job);
            self.job.pop_obj();
            self.emit_clusters(&cl.children);
            self.set_scheme(prev.as_deref());
        }
    }

    /// `stripedBox`.
    pub(crate) fn striped_box(&mut self, af: &[Point], clrs: &str, rotate: bool) -> i32 {
        let (segs, rv) = match parse_segs(clrs) {
            Ok(x) => x,
            Err(e) => return e,
        };
        let mut pts = if rotate {
            [af[2], af[3], af[0], af[1]]
        } else {
            [af[0], af[1], af[2], af[3]]
        };
        let lastx = pts[1].x;
        let xdelta = pts[1].x - pts[0].x;
        pts[1].x = pts[0].x;
        pts[2].x = pts[0].x;
        let save = self.job.obj().penwidth;
        if save > 0.5 {
            self.set_penwidth(0.5);
        }
        let n = segs.len();
        for (i, s) in segs.iter().enumerate() {
            let Some(c) = &s.color else { break };
            if s.t <= 0.0 {
                continue;
            }
            self.set_fillcolor(c);
            let x = if i + 1 == n {
                lastx
            } else {
                pts[0].x + xdelta * s.t
            };
            pts[1].x = x;
            pts[2].x = x;
            self.polygon(&pts, FILL);
            pts[0].x = pts[1].x;
            pts[3].x = pts[1].x;
        }
        if save > 0.5 {
            self.set_penwidth(save);
        }
        rv
    }

    fn emit_node(&mut self, n: usize, drawn: &mut [bool]) {
        let m = self.job.m;
        let nd = &m.nodes[n];
        if drawn[n] {
            return;
        }
        let half = Point::new(nd.lw, nd.ht / 2.0);
        let nbb = BoundingBox::new(
            nd.coord.x - half.x,
            nd.coord.y - half.y,
            nd.coord.x + nd.rw,
            nd.coord.y + half.y,
        );
        if !overlap(nbb, self.job.clip) {
            return;
        }
        drawn[n] = true;
        self.job.push_obj(ObjKind::Node(n));
        self.comment(nd.name);
        let c = nd.attrs.get_or_empty("comment");
        self.comment(c);
        self.job.pop_obj();
        if let Some(st) = nd.attrs.nonempty("style")
            && parse_style(st).iter().any(|i| i.name == "invis")
        {
            return;
        }
        // emit_begin_node
        self.job.push_obj(ObjKind::Node(n));
        self.job.obj_mut().emit_state = EmitState::NDraw;
        self.init_obj_map_data(Some(nd.label.text.clone()), ObjKind::Node(n));
        let prev = self.set_scheme(nd.attrs.get("colorscheme"));
        self.eng.begin_node(&self.job);
        self.node_gencode(n);
        if let Some(xl) = &nd.xlabel
            && xl.set
        {
            self.emit_label(EmitState::NLabel, xl);
        }
        self.eng.end_node(&self.job);
        self.set_scheme(prev.as_deref());
        self.job.pop_obj();
    }

    /// `emit_label`.
    pub(crate) fn emit_label(&mut self, state: EmitState, lp: &TextLabel) {
        let old = self.job.obj().emit_state;
        self.job.obj_mut().emit_state = state;
        if lp.is_html {
            self.emit_html_label(lp);
            self.job.obj_mut().emit_state = old;
            return;
        }
        if lp.spans.is_empty() {
            return;
        }
        self.set_pencolor(&lp.fontcolor);
        let mut p = Point::default();
        p.y = match lp.valign {
            't' => lp.pos.y + lp.space.y / 2.0 - lp.fontsize,
            'b' => lp.pos.y - lp.space.y / 2.0 + lp.dimen.y - lp.fontsize,
            _ => lp.pos.y + lp.dimen.y / 2.0 - lp.fontsize,
        };
        if self.job.obj().labeledgealigned {
            p.y -= lp.pos.y;
        }
        for span in &lp.spans {
            p.x = match span.just {
                'l' => lp.pos.x - lp.space.x / 2.0,
                'r' => lp.pos.x + lp.space.x / 2.0,
                _ => lp.pos.x,
            };
            self.textspan(p, span);
            p.y -= span.height;
        }
        self.job.obj_mut().emit_state = old;
    }

    fn emit_edge(&mut self, e: usize) {
        let m = self.job.m;
        let ed = &m.edges[e];
        let clip = self.job.clip;
        let in_box = splines_bb(&ed.bz).is_some_and(|b| overlap(b, clip))
            || ed.label.as_ref().is_some_and(|l| label_overlaps(l, clip))
            || ed
                .xlabel
                .as_ref()
                .is_some_and(|l| l.set && label_overlaps(l, clip));
        if !in_box {
            return;
        }
        let name = format!(
            "{}{}{}",
            m.nodes[ed.tail].name,
            if m.directed { "->" } else { "--" },
            m.nodes[ed.head].name
        );
        self.job.push_obj(ObjKind::Edge(e));
        self.comment(&name);
        let c = ed.attrs.get_or_empty("comment");
        self.comment(c);
        self.job.pop_obj();
        let styles = ed.attrs.nonempty("style").map(parse_style);
        if styles
            .as_ref()
            .is_some_and(|s| s.iter().any(|i| i.name == "invis"))
        {
            return;
        }
        let prev = self.set_scheme(ed.attrs.get("colorscheme"));
        self.emit_begin_edge(e, styles.as_deref());
        self.emit_edge_graphics(e, styles.as_deref());
        self.emit_end_edge(e);
        self.set_scheme(prev.as_deref());
    }

    fn emit_begin_edge(&mut self, e: usize, styles: Option<&[StyleItem]>) {
        let m = self.job.m;
        let ed = &m.edges[e];
        let a = ed.attrs;
        let flags = self.job.flags;
        self.job.push_obj(ObjKind::Edge(e));
        self.job.obj_mut().emit_state = EmitState::EDraw;
        if ed.label.as_ref().is_some_and(|l| !l.is_html) && a.mapbool("labelaligned") {
            self.job.obj_mut().labeledgealigned = true;
        }
        if let Some(s) = styles
            && !ed.bz.is_empty()
        {
            self.set_style(s);
        }
        if a.nonempty("penwidth").is_some() {
            let pw = a.late_double("penwidth", 1.0, 0.0);
            self.set_penwidth(pw);
        }
        let ctx = self.ctx(ObjKind::Edge(e));
        let sub = |s: &str| strdup_and_subst_obj(s, &ctx);
        let o = self.job.obj_mut();
        if flags & GVRENDER_DOES_LABELS != 0 {
            o.label = ed.label.as_ref().map(|l| l.text.clone());
            o.taillabel = o.label.clone();
            o.headlabel = o.label.clone();
            if let Some(t) = &ed.tail_label {
                o.taillabel = Some(t.text.clone());
            }
            if let Some(h) = &ed.head_label {
                o.headlabel = Some(h.text.clone());
            }
        }
        let pick = |k1: &str, k2: &str| a.nonempty(k1).or_else(|| a.nonempty(k2));
        if flags & GVRENDER_DOES_MAPS != 0 {
            let id = {
                if let Some(id) = a.nonempty("id") {
                    id.to_string()
                } else {
                    let mut s = String::new();
                    if let Some(gid) = m.root_attrs().nonempty("id") {
                        s.push_str(gid);
                        s.push('_');
                    }
                    s.push_str(&format!("edge{}", e + 1));
                    s
                }
            };
            o.id = Some(sub(&id));
            let dflt_url = pick("href", "URL").map(sub);
            o.url = pick("edgehref", "edgeURL").map(sub).or(dflt_url.clone());
            o.labelurl = pick("labelhref", "labelURL").map(sub).or(dflt_url.clone());
            match pick("tailhref", "tailURL") {
                Some(s) => {
                    o.tailurl = Some(sub(s));
                    o.explicit_tailurl = true;
                }
                None => o.tailurl = dflt_url.clone(),
            }
            match pick("headhref", "headURL") {
                Some(s) => {
                    o.headurl = Some(sub(s));
                    o.explicit_headurl = true;
                }
                None => o.headurl = dflt_url,
            }
        }
        if flags & GVRENDER_DOES_TARGETS != 0 {
            let dflt = a.nonempty("target").map(sub);
            o.target = a.nonempty("edgetarget").map(sub).or(dflt.clone());
            o.labeltarget = a.nonempty("labeltarget").map(sub).or(dflt.clone());
            o.tailtarget = a.nonempty("tailtarget").map(sub).or(dflt.clone());
            o.headtarget = a.nonempty("headtarget").map(sub).or(dflt);
        }
        if flags & GVRENDER_DOES_TOOLTIPS != 0 {
            let tt = |s: &str| sub(&interpret_crnl(&html_entity_utf8(s)));
            if let Some(s) = pick("tooltip", "edgetooltip") {
                o.tooltip = Some(tt(s));
                o.explicit_tooltip = true;
            } else if let Some(l) = &o.label {
                o.tooltip = Some(l.clone());
            }
            if let Some(s) = a.nonempty("labeltooltip") {
                o.labeltooltip = Some(tt(s));
                o.explicit_labeltooltip = true;
            } else if let Some(l) = &o.label {
                o.labeltooltip = Some(l.clone());
            }
            if let Some(s) = a.nonempty("tailtooltip") {
                o.tailtooltip = Some(tt(s));
                o.explicit_tailtooltip = true;
            } else if let Some(l) = &o.taillabel {
                o.tailtooltip = Some(l.clone());
            }
            if let Some(s) = a.nonempty("headtooltip") {
                o.headtooltip = Some(tt(s));
                o.explicit_headtooltip = true;
            } else if let Some(l) = &o.headlabel {
                o.headtooltip = Some(l.clone());
            }
        }
        self.eng.begin_edge(&self.job);
        let anchored = {
            let o = self.job.obj();
            o.url.is_some() || o.explicit_tooltip
        };
        if anchored {
            self.begin_obj_anchor();
        }
    }

    fn emit_edge_label(
        &mut self,
        lbl: Option<&TextLabel>,
        lkind: EmitState,
        explicit: bool,
        url: Option<String>,
        tooltip: Option<String>,
        target: Option<String>,
        id: Option<String>,
        decorate: Option<&[Bz]>,
    ) {
        let Some(lbl) = lbl.filter(|l| l.set) else {
            return;
        };
        let newid = id.map(|id| {
            let ty = match lkind {
                EmitState::HLabel => "headlabel",
                EmitState::TLabel => "taillabel",
                _ => "label",
            };
            format!("{id}-{ty}")
        });
        let old = self.job.obj().emit_state;
        self.job.obj_mut().emit_state = lkind;
        let anchored = url.is_some() || explicit;
        if anchored {
            self.begin_anchor(
                url.as_deref(),
                tooltip.as_deref(),
                target.as_deref(),
                newid.as_deref(),
            );
        }
        self.emit_label(lkind, lbl);
        if let Some(spl) = decorate {
            self.emit_attachment(lbl, spl);
        }
        if anchored {
            self.end_anchor();
        }
        self.job.obj_mut().emit_state = old;
    }

    /// `emit_attachment`.
    fn emit_attachment(&mut self, lp: &TextLabel, spl: &[Bz]) {
        if lp.text.trim_start().is_empty() {
            return;
        }
        let sz = lp.dimen;
        let a0 = Point::new(lp.pos.x + sz.x / 2.0, lp.pos.y - sz.y / 2.0);
        let a1 = Point::new(a0.x - sz.x, a0.y);
        let a2 = super::geom::dotneato_closest(spl, lp.pos);
        self.set_style(&default_line_style());
        self.set_pencolor(&lp.fontcolor.clone());
        self.polyline(&[a0, a1, a2]);
    }

    fn emit_end_edge(&mut self, e: usize) {
        let m = self.job.m;
        let ed = &m.edges[e];
        let anchored = {
            let o = self.job.obj();
            o.url.is_some() || o.explicit_tooltip
        };
        if anchored {
            self.end_anchor();
        }
        let o = self.job.obj().clone();
        let decorate = (mapbool(Some(ed.attrs.late_string("decorate", "false")))
            && !ed.bz.is_empty())
        .then_some(ed.bz.as_slice());
        self.emit_edge_label(
            ed.label.as_ref(),
            EmitState::ELabel,
            o.explicit_labeltooltip,
            o.labelurl.clone(),
            o.labeltooltip.clone(),
            o.labeltarget.clone(),
            o.id.clone(),
            decorate,
        );
        self.emit_edge_label(
            ed.xlabel.as_ref(),
            EmitState::ELabel,
            o.explicit_labeltooltip,
            o.labelurl.clone(),
            o.labeltooltip.clone(),
            o.labeltarget.clone(),
            o.id.clone(),
            decorate,
        );
        self.emit_edge_label(
            ed.head_label.as_ref(),
            EmitState::HLabel,
            o.explicit_headtooltip,
            o.headurl.clone(),
            o.headtooltip.clone(),
            o.headtarget.clone(),
            o.id.clone(),
            None,
        );
        self.emit_edge_label(
            ed.tail_label.as_ref(),
            EmitState::TLabel,
            o.explicit_tailtooltip,
            o.tailurl.clone(),
            o.tailtooltip.clone(),
            o.tailtarget.clone(),
            o.id.clone(),
            None,
        );
        self.eng.end_edge(&self.job);
        self.job.pop_obj();
    }

    fn emit_edge_graphics(&mut self, e: usize, styles: Option<&[StyleItem]>) {
        let m = self.job.m;
        let ed = &m.edges[e];
        if ed.bz.is_empty() {
            return;
        }
        let a = ed.attrs;
        let penwidth = self.job.obj().penwidth;
        let arrowsize = a.late_double("arrowsize", 1.0, 0.0);
        let color = a.late_string("color", "");
        let tapered = styles.is_some_and(|s| s.iter().any(|i| i.name == "tapered"));
        let numc = color.matches(':').count();
        let numsemi = color.matches(';').count();
        let mut color = color;
        if numsemi > 0 && numc > 0 {
            if self.multicolor(e, styles, color, arrowsize, penwidth) {
                color = DEFAULT_COLOR;
            } else {
                return;
            }
        }
        let fillcolor = a.late_nnstring("fillcolor", color);
        if fillcolor != color {
            self.set_fillcolor(fillcolor);
        }
        if tapered {
            let color = if color.is_empty() { DEFAULT_COLOR } else { color };
            let fillcolor = if fillcolor.is_empty() {
                DEFAULT_COLOR
            } else {
                fillcolor
            };
            self.set_pencolor("transparent");
            self.set_fillcolor(color);
            let bz = &ed.bz[0];
            let dir = super::geom::taper_dir(a, m.directed);
            let stroke = super::geom::taper(&bz.list, dir, penwidth);
            self.polygon(&stroke, 1);
            self.set_pencolor(color);
            if fillcolor != color {
                self.set_fillcolor(fillcolor);
            }
            if bz.sflag != 0 {
                self.arrow_gen(EmitState::TDraw, bz.sp, bz.list[0], arrowsize, penwidth, bz.sflag);
            }
            if bz.eflag != 0 {
                self.arrow_gen(
                    EmitState::HDraw,
                    bz.ep,
                    *bz.list.last().unwrap(),
                    arrowsize,
                    penwidth,
                    bz.eflag,
                );
            }
        } else if numc > 0 {
            self.parallel_colors(e, color, arrowsize, penwidth);
        } else {
            if !color.is_empty() {
                self.set_pencolor(color);
                self.set_fillcolor(fillcolor);
            } else {
                self.set_pencolor(DEFAULT_COLOR);
                if !fillcolor.is_empty() {
                    self.set_fillcolor(fillcolor);
                } else {
                    self.set_fillcolor(DEFAULT_COLOR);
                }
            }
            let n = ed.bz.len();
            for bz in &ed.bz {
                self.bezier(&bz.list, 0);
                if bz.sflag != 0 {
                    self.arrow_gen(EmitState::TDraw, bz.sp, bz.list[0], arrowsize, penwidth, bz.sflag);
                }
                if bz.eflag != 0 {
                    self.arrow_gen(
                        EmitState::HDraw,
                        bz.ep,
                        *bz.list.last().unwrap(),
                        arrowsize,
                        penwidth,
                        bz.eflag,
                    );
                }
                if n > 1
                    && (bz.sflag != 0 || bz.eflag != 0)
                    && let Some(s) = styles
                {
                    self.set_style(s);
                }
            }
        }
    }

    /// The multi-color (`a:b:c`) branch of `emit_edge_graphics`: parallel offset Béziers.
    fn parallel_colors(&mut self, e: usize, color: &str, arrowsize: f64, penwidth: f64) {
        const SEP: f64 = 2.0;
        let m = self.job.m;
        let ed = &m.edges[e];
        let numc = color.matches(':').count();
        let numc2 = (2.0 + numc as f64) / 2.0;
        let mut offs: Vec<Vec<Point>> = Vec::new();
        let mut tmps: Vec<Vec<Point>> = Vec::new();
        for bz in &ed.bz {
            let n = bz.list.len();
            let mut off = vec![Point::default(); n];
            let mut tmp = vec![Point::default(); n];
            let mut pf3 = bz.list[0];
            let mut pf2 = Point::default();
            let mut j = 0;
            while j + 1 < n {
                let pf0 = pf3;
                let pf1 = bz.list[j + 1];
                off[j] = if j == 0 {
                    super::geom::computeoffset_p(pf0, pf1, SEP)
                } else {
                    super::geom::computeoffset_p(pf2, pf1, SEP)
                };
                pf2 = bz.list[j + 2];
                pf3 = bz.list[j + 3];
                let q = super::geom::computeoffset_qr(pf0, pf1, pf2, pf3, SEP);
                off[j + 1] = q;
                off[j + 2] = q;
                tmp[j] = Point::new(pf0.x - numc2 * off[j].x, pf0.y - numc2 * off[j].y);
                tmp[j + 1] = Point::new(pf1.x - numc2 * off[j + 1].x, pf1.y - numc2 * off[j + 1].y);
                tmp[j + 2] = Point::new(pf2.x - numc2 * off[j + 2].x, pf2.y - numc2 * off[j + 2].y);
                j += 3;
            }
            off[j] = super::geom::computeoffset_p(pf2, pf3, SEP);
            tmp[j] = Point::new(pf3.x - numc2 * off[j].x, pf3.y - numc2 * off[j].y);
            offs.push(off);
            tmps.push(tmp);
        }
        let mut lastcolor = color.to_string();
        let mut headcolor = lastcolor.clone();
        let mut tailcolor = lastcolor.clone();
        let mut cur = lastcolor.clone();
        for (cnum, c) in color.split(':').filter(|c| !c.is_empty()).enumerate() {
            let c = if c.is_empty() { DEFAULT_COLOR } else { c };
            cur = c.to_string();
            if cur != lastcolor {
                self.set_pencolor(c);
                self.set_fillcolor(c);
                lastcolor = cur.clone();
            }
            if cnum == 0 {
                headcolor = cur.clone();
                tailcolor = cur.clone();
            }
            if cnum == 1 {
                tailcolor = cur.clone();
            }
            for (i, tmp) in tmps.iter_mut().enumerate() {
                for (t, o) in tmp.iter_mut().zip(&offs[i]) {
                    t.x += o.x;
                    t.y += o.y;
                }
                let pts = tmp.clone();
                self.bezier(&pts, 0);
            }
        }
        let bz = ed.bz.last().unwrap();
        if bz.sflag != 0 {
            if cur != tailcolor {
                cur = tailcolor.clone();
                self.set_pencolor(&cur);
                self.set_fillcolor(&cur);
            }
            self.arrow_gen(EmitState::TDraw, bz.sp, bz.list[0], arrowsize, penwidth, bz.sflag);
        }
        if bz.eflag != 0 {
            if cur != headcolor {
                cur = headcolor.clone();
                self.set_pencolor(&cur);
                self.set_fillcolor(&cur);
            }
            self.arrow_gen(
                EmitState::HDraw,
                bz.ep,
                *bz.list.last().unwrap(),
                arrowsize,
                penwidth,
                bz.eflag,
            );
        }
    }

    /// `multicolor`: an edge drawn as consecutive colored pieces. Returns `true` on error.
    fn multicolor(
        &mut self,
        e: usize,
        styles: Option<&[StyleItem]>,
        colors: &str,
        arrowsize: f64,
        penwidth: f64,
    ) -> bool {
        let m = self.job.m;
        let ed = &m.edges[e];
        let segs = match parse_segs(colors) {
            Ok((s, _)) => s,
            Err(_) => return true,
        };
        let n = ed.bz.len();
        for bz in &ed.bz {
            let mut left = 1.0;
            let mut first = true;
            let mut endcolor: Option<String> = None;
            let mut right: Vec<Point> = Vec::new();
            for s in &segs {
                let Some(c) = &s.color else { break };
                if aeq0(s.t) {
                    continue;
                }
                self.set_pencolor(c);
                left -= s.t;
                endcolor = Some(c.clone());
                if first {
                    first = false;
                    let (l, r) = super::geom::split_bspline(&bz.list, s.t);
                    self.bezier(&l, 0);
                    right = r;
                    if aeq0(left) {
                        break;
                    }
                } else if aeq0(left) {
                    let r = right.clone();
                    self.bezier(&r, 0);
                    break;
                } else {
                    let (l, r) = super::geom::split_bspline(&right, s.t / (left + s.t));
                    self.bezier(&l, 0);
                    right = r;
                }
            }
            if bz.sflag != 0 {
                if let Some(c) = segs.first().and_then(|s| s.color.clone()) {
                    self.set_pencolor(&c);
                    self.set_fillcolor(&c);
                }
                self.arrow_gen(EmitState::TDraw, bz.sp, bz.list[0], arrowsize, penwidth, bz.sflag);
            }
            if bz.eflag != 0 {
                if let Some(c) = &endcolor {
                    self.set_pencolor(c);
                    self.set_fillcolor(c);
                }
                self.arrow_gen(
                    EmitState::HDraw,
                    bz.ep,
                    *bz.list.last().unwrap(),
                    arrowsize,
                    penwidth,
                    bz.eflag,
                );
            }
            if n > 1
                && (bz.sflag != 0 || bz.eflag != 0)
                && let Some(s) = styles
            {
                self.set_style(s);
            }
        }
        false
    }
}
