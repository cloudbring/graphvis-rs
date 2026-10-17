//! Construction of the dot state from the input graph, plus ports of
//! `graph_init`/`do_graph_label` (lib/common/input.c), `dot_init_subg` and
//! `dot_init_node_edge` (lib/dotgen/dotinit.c), `common_init_node`,
//! `common_init_edge` (lib/common/utils.c) and the port resolution of
//! `poly_port`/`record_port`/`compassPort` (lib/common/shapes.c).

use super::Dot;
use super::types::*;
use super::utils::*;
use gv_core::{Graph, GraphKind, Point};
use gv_geom::{ShapeKind, is_html_label};
use std::f64::consts::PI;

const GAP: f64 = 4.0;

/// `PAD(d)`: `d.x += 4*GAP; d.y += 2*GAP`.
pub fn pad(d: Point) -> Point {
    Point::new(d.x + 4.0 * GAP, d.y + 2.0 * GAP)
}

pub fn cwrotatepf(p: Point, cwrot: i32) -> Point {
    match cwrot {
        90 => Point::new(p.y, -p.x),
        180 => Point::new(p.x, -p.y),
        270 => Point::new(p.y, p.x),
        _ => p,
    }
}

pub fn ccwrotatepf(p: Point, ccwrot: i32) -> Point {
    match ccwrot {
        90 => Point::new(-p.y, p.x),
        180 => Point::new(p.x, -p.y),
        270 => Point::new(p.y, p.x),
        _ => p,
    }
}

fn invflip_side(side: u8, rankdir: i32) -> u8 {
    match rankdir {
        RANKDIR_BT => match side {
            TOP => BOTTOM,
            BOTTOM => TOP,
            s => s,
        },
        RANKDIR_LR => match side {
            TOP => RIGHT,
            BOTTOM => LEFT,
            LEFT => TOP,
            RIGHT => BOTTOM,
            s => s,
        },
        RANKDIR_RL => match side {
            TOP => RIGHT,
            BOTTOM => LEFT,
            LEFT => BOTTOM,
            RIGHT => TOP,
            s => s,
        },
        _ => side,
    }
}

fn invflip_angle(angle: f64, rankdir: i32) -> f64 {
    match rankdir {
        RANKDIR_BT => -angle,
        RANKDIR_LR => angle - PI * 0.5,
        RANKDIR_RL => {
            if angle == PI {
                -0.5 * PI
            } else if angle == PI * 0.75 {
                -0.25 * PI
            } else if angle == PI * 0.5 {
                0.0
            } else if angle == 0.0 {
                PI * 0.5
            } else if angle == PI * -0.25 {
                PI * 0.75
            } else if angle == PI * -0.5 {
                PI
            } else {
                angle
            }
        }
        _ => angle,
    }
}

impl<'a> Dot<'a> {
    /// Builds the arenas from the (already node-sized) input graph.
    pub fn new(input: &'a Graph) -> Self {
        let mut d = Dot::empty(input);
        let nn = input.nodes.len();
        let ne = input.edges.len();
        // Root graph.
        d.graphs.push(GraphInfo {
            name: input.name.clone(),
            root: 0,
            attrs: input.graph_attrs.clone(),
            bound: true,
            ..GraphInfo::default()
        });
        for (i, n) in input.nodes.iter().enumerate() {
            d.nodes.push(NodeInfo {
                real: Some(i),
                name: n.name.clone(),
                cg_root: Some(0),
                ..NodeInfo::default()
            });
            d.graphs[0].nodes.insert(i);
        }
        for (i, e) in input.edges.iter().enumerate() {
            let (t, h) = (e.tail.0 as usize, e.head.0 as usize);
            d.edges.push(EdgeInfo {
                tail: t,
                head: h,
                real: Some(i),
                seq: (i + 1) as u64,
                ..EdgeInfo::default()
            });
            d.nodes[t].cg_out.push(i);
            d.nodes[h].cg_in.push(i);
            d.graphs[0].edges.insert(i);
        }
        d.edge_seq = ne as u64;
        d.n_real_nodes = nn;
        // Subgraphs: create in input order, then wire the hierarchy.
        let base = d.graphs.len();
        let mut is_child = vec![false; input.subgraphs.len()];
        for s in &input.subgraphs {
            for c in &s.subgraphs {
                is_child[c.0 as usize] = true;
            }
        }
        for (i, s) in input.subgraphs.iter().enumerate() {
            let mut gi = GraphInfo {
                name: s.name.clone(),
                root: 0,
                attrs: s.graph_attrs.clone(),
                sub_index: Some(i),
                ..GraphInfo::default()
            };
            for nid in &s.nodes {
                gi.nodes.insert(nid.0 as usize);
            }
            for eid in &s.edges {
                gi.edges.insert(eid.0 as usize);
            }
            d.graphs.push(gi);
        }
        for (i, s) in input.subgraphs.iter().enumerate() {
            for c in &s.subgraphs {
                let ci = base + c.0 as usize;
                if d.graphs[ci].cg_parent.is_none() {
                    d.graphs[ci].cg_parent = Some(base + i);
                    d.graphs[base + i].subgs.push(ci);
                }
            }
        }
        for (i, child) in is_child.iter().enumerate() {
            if !child {
                d.graphs[base + i].cg_parent = Some(0);
                d.graphs[0].subgs.push(base + i);
            }
        }
        // Subgraph membership implies membership in every ancestor.
        for g in base..d.graphs.len() {
            let nodes: Vec<N> = d.graphs[g].nodes.iter().copied().collect();
            let edges: Vec<E> = d.graphs[g].edges.iter().copied().collect();
            let mut p = d.graphs[g].cg_parent;
            while let Some(pg) = p {
                if pg == 0 {
                    break;
                }
                d.graphs[pg].nodes.extend(nodes.iter().copied());
                d.graphs[pg].edges.extend(edges.iter().copied());
                p = d.graphs[pg].cg_parent;
            }
        }
        d
    }

    /// `graph_init` (the parts dot uses).
    pub fn graph_init(&mut self) {
        let g = 0;
        let mut rankdir = RANKDIR_TB;
        if let Some(p) = self.agget(g, "rankdir") {
            rankdir = match p {
                "LR" => RANKDIR_LR,
                "BT" => RANKDIR_BT,
                "RL" => RANKDIR_RL,
                _ => RANKDIR_TB,
            };
        }
        self.graphs[g].rankdir = rankdir;
        let xf = late_double(self.agget(g, "nodesep"), 0.25, 0.02);
        self.graphs[g].nodesep = points(xf);
        let mut xf = 0.5;
        if let Some(p) = self.agget(g, "ranksep") {
            match strtod_prefix(p) {
                None => xf = 0.5,
                Some(v) => xf = if v < 0.02 { 0.02 } else { v },
            }
            if p.contains("equally") {
                self.graphs[g].exact_ranksep = true;
            }
        }
        self.graphs[g].ranksep = points(xf);
        // setRatio
        if let Some(p) = self.agget(g, "ratio").map(str::to_string) {
            let dr = &mut self.graphs[g].drawing;
            match p.as_str() {
                "auto" => dr.ratio_kind = RatioKind::Auto,
                "compress" => dr.ratio_kind = RatioKind::Compress,
                "expand" => dr.ratio_kind = RatioKind::Expand,
                "fill" => dr.ratio_kind = RatioKind::Fill,
                _ => {
                    let r = atof(&p);
                    if r > 0.0 {
                        dr.ratio_kind = RatioKind::Value;
                        dr.ratio = r;
                    }
                }
            }
        }
        let (size, filled) = getdoubles2ptf(self.agget(g, "size"));
        if let Some(s) = size {
            self.graphs[g].drawing.size = s;
        }
        self.graphs[g].drawing.filled = filled;
        if let (Some(p), _) = getdoubles2ptf(self.agget(g, "page")) {
            self.graphs[g].drawing.page = p;
        }
        let p = self.agget(g, "clusterrank");
        self.cl_type = match p {
            Some("local") => LOCAL,
            Some("global") => GLOBAL,
            Some("none") => NOCLUST,
            _ => LOCAL,
        };
        self.concentrate = mapbool(self.agget(g, "concentrate"));
        self.do_graph_label(g);
    }

    /// `do_graph_label`
    pub fn do_graph_label(&mut self, sg: G) {
        let flip = self.graphs[0].flip();
        let name = self.graphs[sg].name.clone();
        let Some(gl) = gv_geom::graph_label_geometry(
            &name,
            &self.graphs[sg].attrs,
            sg == 0,
            flip,
            self.env_latin1(),
        ) else {
            return;
        };
        self.graphs[0].has_labels |= GRAPH_LABEL;
        let l = self.push_label(&gl.label);
        self.graphs[sg].label = Some(l);
        self.graphs[sg].label_pos = gl.pos_flag;
        if let Some((side, d)) = gl.border {
            self.graphs[sg].border[side as usize] = d;
        }
    }

    fn env_latin1(&self) -> bool {
        gv_geom::NodeSizeEnv::from_graph(self.input).charset_latin1
    }

    fn push_label(&mut self, lg: &gv_geom::LabelGeometry) -> L {
        let l = self.labels.len();
        self.labels.push(TextLabel {
            fontname: lg.font_name.clone(),
            fontsize: lg.font_size,
            dimen: lg.dimen,
            space: lg.space,
            html: lg.html,
            ..TextLabel::default()
        });
        l
    }

    /// `dot_init_subg(g, g)`: every subgraph gets an Agraphinfo_t record.
    pub fn dot_init_subg(&mut self) {
        for g in 0..self.graphs.len() {
            self.graphs[g].bound = true;
        }
    }

    /// `dot_init_node_edge`
    pub fn dot_init_node_edge(&mut self) {
        let flip = self.graphs[0].flip();
        for n in self.nodes_of(0) {
            self.dot_init_node(n, flip);
        }
        for n in self.nodes_of(0) {
            for e in self.out_edges(0, n) {
                self.dot_init_edge(e);
            }
        }
    }

    fn dot_init_node(&mut self, n: N, flip: bool) {
        let r = self.nodes[n].real.unwrap();
        let nd = &self.input.nodes[r];
        let env = gv_geom::NodeSizeEnv::from_graph(self.input);
        let geom = gv_geom::node_geometry(nd, &env);
        let (lw, rw, ht) = geom.nodesize(flip);
        let info = &mut self.nodes[n];
        info.width = geom.width;
        info.height = geom.height;
        info.is_record = geom.record.is_some();
        info.lw = lw;
        info.rw = rw;
        info.ht = ht;
        info.uf_size = 1;
        if geom.xlabel.is_some() {
            self.graphs[0].has_labels |= NODE_XLABEL;
        }
    }

    fn dot_init_edge(&mut self, e: E) {
        self.common_init_edge(e);
        let mut weight = late_int(self.edge_attr(e, "weight"), 1, 0);
        let (t, h) = (self.edges[e].tail, self.edges[e].head);
        let tg = self.node_attr(t, "group").unwrap_or("");
        let hg = self.node_attr(h, "group").unwrap_or("");
        let mut xpenalty = 1;
        if !tg.is_empty() && tg == hg {
            xpenalty = CL_CROSS;
            weight *= 100;
        }
        if self.nonconstraint_edge(e) {
            xpenalty = 0;
            weight = 0;
        }
        let minlen = late_int(self.edge_attr(e, "minlen"), 1, 0);
        let ed = &mut self.edges[e];
        ed.count = 1;
        ed.xpenalty = xpenalty;
        ed.weight = weight;
        ed.minlen = minlen;
    }

    fn common_init_edge(&mut self, e: E) {
        let r = self.edges[e].real.unwrap();
        let (t, h) = (self.edges[e].tail, self.edges[e].head);
        let env = gv_geom::NodeSizeEnv::from_graph(self.input);
        let directed = self.input.kind == GraphKind::Directed;
        let ie = &self.input.edges[r];
        let geo = gv_geom::edge_label_geometry(
            &ie.attrs,
            &self.nodes[t].name,
            &self.nodes[h].name,
            directed,
            &env,
        );
        if let Some(lg) = &geo.label {
            let l = self.push_label(lg);
            self.edges[e].label = Some(l);
            self.graphs[0].has_labels |= EDGE_LABEL;
            self.edges[e].label_ontop = geo.label_float;
        }
        if let Some(lg) = &geo.xlabel {
            let l = self.push_label(lg);
            self.edges[e].xlabel = Some(l);
            self.graphs[0].has_labels |= EDGE_XLABEL;
        }
        if let Some(lg) = &geo.head_label {
            let l = self.push_label(lg);
            self.edges[e].head_label = Some(l);
            self.graphs[0].has_labels |= HEAD_LABEL;
        }
        if let Some(lg) = &geo.tail_label {
            let l = self.push_label(lg);
            self.edges[e].tail_label = Some(l);
            self.graphs[0].has_labels |= TAIL_LABEL;
        }
        let ie = &self.input.edges[r];
        let tp = ie.tail_port.clone();
        let hp = ie.head_port.clone();
        let tail_str = port_string(&tp);
        if !tail_str.is_empty() {
            self.nodes[t].has_port = true;
        }
        let mut port = self.chk_port(t, &tp);
        if no_clip(self.edge_attr(e, "tailclip")) {
            port.clip = false;
        }
        self.edges[e].tail_port = port;
        let head_str = port_string(&hp);
        if !head_str.is_empty() {
            self.nodes[h].has_port = true;
        }
        let mut port = self.chk_port(h, &hp);
        if no_clip(self.edge_attr(e, "headclip")) {
            port.clip = false;
        }
        self.edges[e].head_port = port;
    }

    /// `chkPort(ND_shape(n)->fns->portfn, n, s)`
    fn chk_port(&self, n: N, p: &gv_core::Port) -> Port {
        let name = p.name.clone().unwrap_or_default();
        let compass = p.compass.clone();
        let mut pt = self.shape_port(n, &name, compass.as_deref());
        pt.name = if name.is_empty() {
            None
        } else if let Some(c) = &compass {
            Some(c.clone())
        } else {
            Some(name)
        };
        pt
    }

    /// `poly_port` / `record_port`
    fn shape_port(&self, n: N, portname: &str, compass: Option<&str>) -> Port {
        if portname.is_empty() {
            return Port::center();
        }
        let compass = compass.unwrap_or("_");
        let all = BOTTOM | RIGHT | TOP | LEFT;
        let r = self.nodes[n].real.unwrap();
        let nd = &self.input.nodes[r];
        let shape = ShapeKind::from_node(nd);
        let raw_label = nd.attrs.get("label").map_or("\\N", |s| s.as_str());
        let html = is_html_label(raw_label, shape);
        let rankdir_s = match self.graphs[0].rankdir {
            RANKDIR_LR => "LR",
            RANKDIR_BT => "BT",
            RANKDIR_RL => "RL",
            _ => "TB",
        };
        let mut rv = Port::default();
        let _ = rankdir_s;
        if shape.is_record() && !html {
            let env = gv_geom::NodeSizeEnv::from_graph(self.input);
            if let Some(rec) = gv_geom::node_geometry(nd, &env).record {
                let whole = BoxF {
                    ll: rec.bbox.ll,
                    ur: rec.bbox.ur,
                };
                if let Some(f) = rec.find_port(portname) {
                    let b = BoxF {
                        ll: f.bbox.ll,
                        ur: f.bbox.ur,
                    };
                    let sides = box_sides(&b, &whole);
                    self.compass_port(n, Some(b), &mut rv, compass, sides, false);
                } else {
                    self.compass_port(n, Some(whole), &mut rv, portname, all, false);
                }
                return rv;
            }
        }
        if html
            && let Some(tbl) = gv_geom::html_table_layout(nd, &self.input.name)
            && let Some(pb) = tbl.find_port_box(portname)
        {
            let b = BoxF {
                ll: pb.ll,
                ur: pb.ur,
            };
            let whole = BoxF {
                ll: tbl.bbox.ll,
                ur: tbl.bbox.ur,
            };
            let sides = box_sides(&b, &whole);
            self.compass_port(n, Some(b), &mut rv, compass, sides, false);
            rv.name = None;
            return rv;
        }
        let ictxt = shape != ShapeKind::Box && !shape.is_record();
        self.compass_port(n, None, &mut rv, portname, all, ictxt);
        rv.name = None;
        rv
    }

    /// `compassPoint`: boundary point on the ray from the centre toward `(x, y)`.
    fn compass_point(&self, n: N, y: f64, x: f64) -> Point {
        let r = self.nodes[n].real.unwrap();
        let mut nd = self.input.nodes[r].clone();
        nd.pos = Point::new(0.0, 0.0);
        gv_geom::clip_ray_to_node_boundary(&nd, &gv_core::Port::default(), Point::new(x, y))
    }

    /// `compassPort`
    fn compass_port(
        &self,
        n: N,
        bp: Option<BoxF>,
        pp: &mut Port,
        compass: &str,
        sides: u8,
        ictxt: bool,
    ) -> i32 {
        let mut b;
        let mut p;
        let mut rv = 0;
        let mut theta = 0.0;
        let mut constrain = false;
        let mut dyna = false;
        let mut side = 0u8;
        let mut clip = true;
        let mut defined;
        if let Some(bb) = bp {
            b = bb;
            p = Point::new((b.ll.x + b.ur.x) / 2.0, (b.ll.y + b.ur.y) / 2.0);
            defined = true;
        } else {
            p = Point::new(0.0, 0.0);
            b = BoxF::default();
            let nd = &self.nodes[n];
            if self.graphs[0].flip() {
                b.ur.x = nd.ht / 2.0;
                b.ll.x = -b.ur.x;
                b.ur.y = nd.lw;
                b.ll.y = -b.ur.y;
            } else {
                b.ur.y = nd.ht / 2.0;
                b.ll.y = -b.ur.y;
                b.ur.x = nd.lw;
                b.ll.x = -b.ur.x;
            }
            defined = false;
        }
        let maxv = b.ur.x.max(b.ur.y) * 4.0;
        let ctr = p;
        let cb = compass.as_bytes();
        if !cb.is_empty() {
            let rest = &cb[1..];
            match cb[0] {
                b'e' => {
                    if !rest.is_empty() {
                        rv = 1;
                    } else {
                        if ictxt {
                            p = self.compass_point(n, ctr.y, maxv);
                        } else {
                            p.x = b.ur.x;
                        }
                        theta = 0.0;
                        constrain = true;
                        defined = true;
                        clip = false;
                        side = sides & RIGHT;
                    }
                }
                b's' => {
                    p.y = b.ll.y;
                    constrain = true;
                    clip = false;
                    match rest.first() {
                        None => {
                            theta = -PI * 0.5;
                            defined = true;
                            if ictxt {
                                p = self.compass_point(n, -maxv, ctr.x);
                            } else {
                                p.x = ctr.x;
                            }
                            side = sides & BOTTOM;
                        }
                        Some(b'e') => {
                            theta = -PI * 0.25;
                            defined = true;
                            if ictxt {
                                p = self.compass_point(n, -maxv, maxv);
                            } else {
                                p.x = b.ur.x;
                            }
                            side = sides & (BOTTOM | RIGHT);
                        }
                        Some(b'w') => {
                            theta = -PI * 0.75;
                            defined = true;
                            if ictxt {
                                p = self.compass_point(n, -maxv, -maxv);
                            } else {
                                p.x = b.ll.x;
                            }
                            side = sides & (BOTTOM | LEFT);
                        }
                        _ => {
                            p.y = ctr.y;
                            constrain = false;
                            clip = true;
                            rv = 1;
                        }
                    }
                }
                b'w' => {
                    if !rest.is_empty() {
                        rv = 1;
                    } else {
                        if ictxt {
                            p = self.compass_point(n, ctr.y, -maxv);
                        } else {
                            p.x = b.ll.x;
                        }
                        theta = PI;
                        constrain = true;
                        defined = true;
                        clip = false;
                        side = sides & LEFT;
                    }
                }
                b'n' => {
                    p.y = b.ur.y;
                    constrain = true;
                    clip = false;
                    match rest.first() {
                        None => {
                            defined = true;
                            theta = PI * 0.5;
                            if ictxt {
                                p = self.compass_point(n, maxv, ctr.x);
                            } else {
                                p.x = ctr.x;
                            }
                            side = sides & TOP;
                        }
                        Some(b'e') => {
                            defined = true;
                            theta = PI * 0.25;
                            if ictxt {
                                p = self.compass_point(n, maxv, maxv);
                            } else {
                                p.x = b.ur.x;
                            }
                            side = sides & (TOP | RIGHT);
                        }
                        Some(b'w') => {
                            defined = true;
                            theta = PI * 0.75;
                            if ictxt {
                                p = self.compass_point(n, maxv, -maxv);
                            } else {
                                p.x = b.ll.x;
                            }
                            side = sides & (TOP | LEFT);
                        }
                        _ => {
                            p.y = ctr.y;
                            constrain = false;
                            clip = true;
                            rv = 1;
                        }
                    }
                }
                b'_' => {
                    dyna = true;
                    side = sides;
                }
                b'c' => {}
                _ => rv = 1,
            }
        }
        let rankdir = self.graphs[0].rankdir;
        p = cwrotatepf(p, 90 * rankdir);
        pp.side = if dyna {
            side
        } else {
            invflip_side(side, rankdir)
        };
        pp.bp = bp;
        pp.p = p;
        pp.theta = invflip_angle(theta, rankdir);
        if p.x == 0.0 && p.y == 0.0 {
            pp.order = (MC_SCALE / 2) as u8;
        } else {
            let mut angle = p.y.atan2(p.x) + 1.5 * PI;
            if angle >= 2.0 * PI {
                angle -= 2.0 * PI;
            }
            pp.order = ((MC_SCALE as f64 * angle) / (2.0 * PI)) as i32 as u8;
        }
        pp.constrained = constrain;
        pp.defined = defined;
        pp.clip = clip;
        pp.dyna = dyna;
        rv
    }
}

/// Sides of the enclosing box that a field box touches.
fn box_sides(b: &BoxF, whole: &BoxF) -> u8 {
    let eps = 0.5;
    let mut s = 0;
    if (b.ll.y - whole.ll.y).abs() < eps {
        s |= BOTTOM;
    }
    if (b.ur.x - whole.ur.x).abs() < eps {
        s |= RIGHT;
    }
    if (b.ur.y - whole.ur.y).abs() < eps {
        s |= TOP;
    }
    if (b.ll.x - whole.ll.x).abs() < eps {
        s |= LEFT;
    }
    s
}

fn port_string(p: &gv_core::Port) -> String {
    match (&p.name, &p.compass) {
        (Some(n), Some(c)) => format!("{n}:{c}"),
        (Some(n), None) => n.clone(),
        (None, Some(c)) => c.clone(),
        (None, None) => String::new(),
    }
}

/// `noClip`: "" means clip.
fn no_clip(v: Option<&str>) -> bool {
    match v {
        Some(s) if !s.is_empty() => !mapbool(Some(s)),
        _ => false,
    }
}

/// `getdoubles2ptf`
fn getdoubles2ptf(p: Option<&str>) -> (Option<Point>, bool) {
    let Some(p) = p else { return (None, false) };
    // sscanf("%lf,%lf%c")
    let t = p.trim_start();
    if let Some(xf) = strtod_prefix(t) {
        let consumed = float_len(t);
        let rest = &t[consumed..];
        if let Some(r2) = rest.strip_prefix(',')
            && let Some(yf) = strtod_prefix(r2)
        {
            let r3 = &r2.trim_start()[float_len(r2.trim_start())..];
            if xf > 0.0 && yf > 0.0 {
                return (
                    Some(Point::new(points(xf) as f64, points(yf) as f64)),
                    r3.starts_with('!'),
                );
            }
        }
        if xf > 0.0 {
            return (
                Some(Point::new(points(xf) as f64, points(xf) as f64)),
                rest.starts_with('!'),
            );
        }
    }
    (None, false)
}

/// Length of the float prefix accepted by `strtod`.
fn float_len(s: &str) -> usize {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
        i += 1;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let ds = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > ds {
            i = j;
        }
    }
    i
}
