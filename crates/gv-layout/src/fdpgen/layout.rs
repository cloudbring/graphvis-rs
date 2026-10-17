//! The fdp driver, ported from `lib/fdpgen/layout.c`: `fdp_layout`,
//! `fdpLayout`, `layout`, `deriveGraph`, `getEdgeList`, `genPorts`,
//! `expandCluster`, `finalCC`, `evalPositions`, `setBB`, `chkPos`,
//! `fdpSplines`.
//!
//! Each graph (the root or a cluster) is laid out through a *derived graph*
//! whose nodes are its own nodes, one node per child cluster and one node per
//! boundary port inherited from the parent. Derived graphs are kept as
//! [`DG`]; whenever shared neato code (overlap removal, packing, bounding
//! boxes) has to run on one, [`with_lgraph`] builds a matching [`LGraph`].
//!
//! cgraph orders derived nodes and edges by address; the port uses creation
//! order, which is what consecutive allocations give in practice.

use super::comp::find_ccomp;
use super::fdpinit::{get_inputscale, init_edges, initial_positions};
use super::tlayout::{TState, fdp_init_params, fdp_t_layout};
use super::xlayout::{XParams, fdp_x_layout};
use super::{ClusterTree, mk_clusters, subgraph_member_nodes};
use crate::common::postproc::gv_postprocess;
use crate::common::shapes::NodeShape;
use crate::common::types::*;
use crate::common::utils::{compute_bb, late_int};
use crate::cutil::{CRand, qsort_by};
use crate::neatogen::neatosplines::{neato_set_aspect, spline_edges_main};
use crate::osage::register_clusters;
use crate::pack::{PackInfo, PackMode, get_pack, get_pack_mode_info, put_graphs};
use gv_core::{AttrMap, BoundingBox, Graph, GraphKind, Point, Result};
use std::collections::HashMap;
use std::f64::consts::PI;

/// `CL_OFFSET`.
const CL_OFFSET: i64 = 8;
/// Maximum angular change between ports: 2 degrees.
const ANG: f64 = PI / 90.0;

/// What a derived node stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DKind {
    /// A child cluster (index into the [`ClusterTree`]).
    Clust(usize),
    /// A real node of the input graph.
    Real(usize),
    /// A boundary port.
    Port,
}

/// A node of a derived graph (`Agnodeinfo_t` + `dndata`).
#[derive(Debug, Clone)]
pub struct DNode {
    pub kind: DKind,
    /// `ND_pos`, inches.
    pub pos: [f64; 2],
    pub width: f64,
    pub height: f64,
    pub lw: f64,
    pub rw: f64,
    pub ht: f64,
    pub pinned: u8,
    pub deg: i32,
    pub wdeg: i32,
    pub disp: [f64; 2],
}

impl DNode {
    fn new(kind: DKind) -> Self {
        Self {
            kind,
            pos: [0.0, 0.0],
            width: 0.0,
            height: 0.0,
            lw: 0.0,
            rw: 0.0,
            ht: 0.0,
            pinned: 0,
            deg: 0,
            wdeg: 0,
            disp: [0.0, 0.0],
        }
    }

    /// `IS_PORT(n)`.
    pub fn is_port(&self) -> bool {
        self.kind == DKind::Port
    }
}

/// A derived edge; `reals` is the list kept in `ED_to_virt`/`ED_count`.
#[derive(Debug, Clone)]
pub struct DEdge {
    pub t: usize,
    pub h: usize,
    pub dist: f64,
    pub factor: f64,
    pub reals: Vec<usize>,
}

/// `bport_t` in a derived graph.
#[derive(Debug, Clone, Copy)]
pub struct DPort {
    pub n: usize,
    pub alpha: f64,
    pub e: usize,
}

/// `bport_t` attached to a cluster: a real edge and its real endpoint inside.
#[derive(Debug, Clone, Copy)]
pub struct BPort {
    pub e: usize,
    pub n: usize,
    pub alpha: f64,
}

/// A derived graph (strict, directed; edges run from the older node).
#[derive(Debug, Clone, Default)]
pub struct DG {
    pub uid: u32,
    pub nodes: Vec<DNode>,
    pub edges: Vec<DEdge>,
    pub out: Vec<Vec<usize>>,
    pub inn: Vec<Vec<usize>>,
    pub ports: Vec<DPort>,
    /// `overlap`, `sep` and `K` copied from the original graph (`copyAttr`).
    pub attrs: AttrMap,
    emap: HashMap<(usize, usize), usize>,
}

/// A component of a derived graph (node-induced subgraph).
#[derive(Debug, Clone)]
pub struct Comp {
    pub member: Vec<bool>,
    pub nodes: Vec<usize>,
    /// The component carries the derived graph's ports (`PORTS(cg)`).
    pub has_ports: bool,
}

impl Comp {
    pub fn new(n_total: usize, mut nodes: Vec<usize>, has_ports: bool) -> Self {
        nodes.sort_unstable();
        let mut member = vec![false; n_total];
        for &n in &nodes {
            member[n] = true;
        }
        Self {
            member,
            nodes,
            has_ports,
        }
    }

    /// `agdelete(cg, n)`.
    pub fn remove(&mut self, n: usize) {
        self.member[n] = false;
        self.nodes.retain(|&m| m != n);
    }
}

impl DG {
    fn add_node(&mut self, n: DNode) -> usize {
        self.nodes.push(n);
        self.out.push(Vec::new());
        self.inn.push(Vec::new());
        self.nodes.len() - 1
    }

    /// `agedge(dg, t, h, NULL, 1)` on a strict graph; returns the edge and
    /// whether it is new.
    fn edge(&mut self, t: usize, h: usize) -> (usize, bool) {
        if let Some(&e) = self.emap.get(&(t, h)) {
            return (e, false);
        }
        let e = self.edges.len();
        self.edges.push(DEdge {
            t,
            h,
            dist: 1.0,
            factor: 1.0,
            reals: Vec::new(),
        });
        self.out[t].push(e);
        self.inn[h].push(e);
        self.emap.insert((t, h), e);
        (e, true)
    }

    /// Orders adjacency like cgraph (`agedgeseqcmpf`: other end, then edge).
    fn sort_adjacency(&mut self) {
        let edges = &self.edges;
        for l in &mut self.out {
            l.sort_by_key(|&e| (edges[e].h, e));
        }
        for l in &mut self.inn {
            l.sort_by_key(|&e| (edges[e].t, e));
        }
    }

    /// `agfstout(cg, n)`..`agnxtout`.
    pub fn outs<'s>(&'s self, c: &'s Comp, n: usize) -> impl Iterator<Item = usize> + 's {
        self.out[n]
            .iter()
            .copied()
            .filter(move |&e| c.member[self.edges[e].h])
    }

    /// `agfstedge(cg, n)`..`agnxtedge`: out-edges, then in-edges.
    pub fn all_edges(&self, c: &Comp, n: usize) -> Vec<usize> {
        let mut v: Vec<usize> = self.outs(c, n).collect();
        v.extend(
            self.inn[n]
                .iter()
                .copied()
                .filter(|&e| c.member[self.edges[e].t] && self.edges[e].t != n),
        );
        v
    }
}

/// Runs `f` on an [`LGraph`] holding the derived nodes `keep` (in order) and
/// the derived edges among them, then copies node positions back. The third
/// argument maps derived node indices to [`LGraph`] indices.
pub fn with_lgraph<R>(
    dg: &mut DG,
    keep: &[usize],
    crand: &mut CRand,
    f: impl FnOnce(&mut LGraph<'_>, &mut Sub, &[usize]) -> R,
) -> R {
    let mut src = Graph::new("derived", GraphKind::Directed, true);
    src.graph_attrs = dg.attrs.clone();
    let mut map = vec![usize::MAX; dg.nodes.len()];
    for (i, &n) in keep.iter().enumerate() {
        map[n] = i;
        src.ensure_node(&format!("{i}"));
    }
    let mut edges = Vec::new();
    for (ei, e) in dg.edges.iter().enumerate() {
        if map[e.t] != usize::MAX && map[e.h] != usize::MAX {
            src.add_edge(
                gv_core::NodeId(map[e.t] as u32),
                gv_core::NodeId(map[e.h] as u32),
                AttrMap::new(),
            );
            edges.push(ei);
        }
    }
    let nodes: Vec<LNode> = keep
        .iter()
        .enumerate()
        .map(|(i, &n)| {
            let d = &dg.nodes[n];
            LNode {
                pos: vec![d.pos[0], d.pos[1]],
                coord: Point::default(),
                width: d.width,
                height: d.height,
                lw: d.lw,
                rw: d.rw,
                ht: d.ht,
                pinned: d.pinned,
                id: 0,
                heapindex: -1,
                hops: 0,
                dist: 0.0,
                mark: false,
                shape: NodeShape::new(&src.nodes[i]),
                xlabel: None,
                clust: None,
                lim: -1,
            }
        })
        .collect();
    let mut out = vec![Vec::new(); keep.len()];
    let mut inn = vec![Vec::new(); keep.len()];
    let ledges: Vec<LEdge> = edges
        .iter()
        .enumerate()
        .map(|(i, &ei)| {
            let e = &dg.edges[ei];
            let (t, h) = (map[e.t], map[e.h]);
            out[t].push(i);
            inn[h].push(i);
            LEdge {
                tail: t,
                head: h,
                dist: e.dist,
                factor: e.factor,
                count: 0,
                to_virt: None,
                spl: None,
                label: None,
                xlabel: None,
                head_label: None,
                tail_label: None,
                tail_port: PortInfo::default(),
                head_port: PortInfo::default(),
                path: Vec::new(),
            }
        })
        .collect();
    for l in out.iter_mut() {
        l.sort_by_key(|&e| (ledges[e].head, e));
    }
    for l in inn.iter_mut() {
        l.sort_by_key(|&e| (ledges[e].tail, e));
    }
    let mut lg = LGraph {
        src: &src,
        nodes,
        edges: ledges,
        out,
        inn,
        ndim: 2,
        nodesep: 18,
        bb: BoundingBox::default(),
        label: None,
        label_pos: 0,
        drawing: Drawing::default(),
        clusters: Vec::new(),
        root_clusters: Vec::new(),
        edge_type: EdgeType::Line,
        concentrate: false,
        drand: Default::default(),
        crand: *crand,
        has_labelangle: false,
        has_labeldistance: false,
    };
    let mut sub = lg.root_sub();
    let r = f(&mut lg, &mut sub, &map);
    for (i, &n) in keep.iter().enumerate() {
        dg.nodes[n].pos = [lg.nodes[i].pos[0], lg.nodes[i].pos[1]];
    }
    *crand = lg.crand;
    r
}

/// `erec`.
#[derive(Debug, Clone, Copy)]
struct ERec {
    e: usize,
    alpha: f64,
    dist2: f64,
}

struct Fdp<'g, 'a> {
    rg: &'g mut LGraph<'a>,
    tree: ClusterTree,
    st: TState,
    /// `PORTS(sg)` of each cluster.
    ports: Vec<Vec<BPort>>,
    /// `BB(g)` (inches) of each cluster and of the root.
    gbb: Vec<BoundingBox>,
    root_bb: BoundingBox,
    members: Vec<Vec<usize>>,
    /// Edge membership of each cluster subgraph.
    cedges: Vec<Vec<bool>>,
    /// `PARENT(n)`: smallest containing graph (`Some(None)` = root).
    parent: Vec<Option<Option<usize>>>,
    /// `DNODE(n)`: derived graph id and node.
    dnode: Vec<Option<(u32, usize)>>,
    next_uid: u32,
    pack: PackInfo,
    coords_declared: bool,
    inputscale: f64,
}

impl Fdp<'_, '_> {
    fn gattrs(&self, g: Option<usize>) -> &AttrMap {
        match g {
            None => &self.rg.src.graph_attrs,
            Some(c) => &self.rg.src.subgraphs[self.tree.subgraph[c]].graph_attrs,
        }
    }

    fn members_of(&self, g: Option<usize>) -> Vec<usize> {
        match g {
            None => (0..self.rg.nodes.len()).collect(),
            Some(c) => self.members[c].clone(),
        }
    }

    /// `GPARENT(g)` as a `PARENT` value (`None` = NULL for the root).
    fn gparent(&self, g: Option<usize>) -> Option<Option<usize>> {
        g.map(|c| self.tree.parent[c])
    }

    fn bb_of(&self, g: Option<usize>) -> BoundingBox {
        match g {
            None => self.root_bb,
            Some(c) => self.gbb[c],
        }
    }

    fn set_bb(&mut self, g: Option<usize>, bb: BoundingBox) {
        match g {
            None => self.root_bb = bb,
            Some(c) => self.gbb[c] = bb,
        }
    }

    /// `chkPos(g, n, infop, bbp)`: a cluster's `coords` attribute.
    fn chk_pos(&self, c: usize, dn: &mut DNode, bbp: &mut BoundingBox) {
        let attrs = self.gattrs(Some(c));
        let Some(p) = attrs.get("coords").filter(|s| !s.is_empty()) else {
            return;
        };
        let parent = self.gattrs(self.tree.parent[c]);
        if parent.get("coords").map(String::as_str).unwrap_or("") == p.as_str() {
            return;
        }
        let mut vals = Vec::new();
        let mut rest = p.as_str();
        while vals.len() < 4 {
            let Some((v, r)) = crate::common::utils::parse_f64_rest(rest) else {
                break;
            };
            vals.push(v);
            rest = r;
            if vals.len() < 4 {
                match rest.strip_prefix(',') {
                    Some(r) => rest = r,
                    None => break,
                }
            }
        }
        if vals.len() < 4 {
            return;
        }
        let c = rest.chars().next();
        let mut bb = BoundingBox::new(vals[0], vals[1], vals[2], vals[3]);
        if self.inputscale > 0.0 {
            let s = self.inputscale;
            bb = BoundingBox::new(bb.ll.x / s, bb.ll.y / s, bb.ur.x / s, bb.ur.y / s);
        }
        dn.pinned = match c {
            Some('!') => P_PIN,
            Some('?') => P_FIX,
            _ => P_SET,
        };
        *bbp = bb;
    }

    /// `deriveGraph(g, infop)`; `None` when a node lies in two
    /// non-comparable clusters.
    fn derive_graph(&mut self, g: Option<usize>) -> Option<DG> {
        let mut dg = DG {
            uid: self.next_uid,
            ..DG::default()
        };
        self.next_uid += 1;
        let uid = dg.uid;
        let src = self.rg.src;
        for key in ["overlap", "sep", "K"] {
            let declared = src.graph_attrs.contains_key(key)
                || src
                    .subgraphs
                    .iter()
                    .any(|s| s.graph_attrs.contains_key(key));
            if declared {
                let v = self.gattrs(g).get(key).cloned().unwrap_or_default();
                dg.attrs.insert(key.to_string(), v);
            }
        }

        for c in self.tree.kids(g).to_vec() {
            let m = f64::MAX;
            let mut fix_bb = BoundingBox::new(m, m, -m, -m);
            let mut dn = DNode::new(DKind::Clust(c));
            if self.coords_declared {
                self.chk_pos(c, &mut dn, &mut fix_bb);
            }
            if dn.pinned != 0 {
                dn.pos = [
                    (fix_bb.ll.x + fix_bb.ur.x) / 2.0,
                    (fix_bb.ll.y + fix_bb.ur.y) / 2.0,
                ];
            }
            let id = dg.add_node(dn);
            for &n in &self.members[c] {
                self.dnode[n] = Some((uid, id));
            }
        }

        let members = self.members_of(g);
        for &n in &members {
            if self.dnode[n].is_some() {
                continue;
            }
            if self.parent[n].is_some() && self.parent[n] != self.gparent(g) {
                return None;
            }
            self.parent[n] = Some(g);
            let rn = &self.rg.nodes[n];
            let mut dn = DNode::new(DKind::Real(n));
            dn.width = rn.width;
            dn.height = rn.height;
            dn.lw = rn.lw;
            dn.rw = rn.rw;
            dn.ht = rn.ht;
            if rn.pinned != 0 {
                dn.pos = [rn.pos[0], rn.pos[1]];
                dn.pinned = rn.pinned;
            }
            let id = dg.add_node(dn);
            self.dnode[n] = Some((uid, id));
        }

        let root = self.rg.root_sub();
        for &n in &members {
            let tl = self.dnode[n].expect("derived node").1;
            let outs: Vec<usize> = self.rg.outs(&root, n).collect();
            for e in outs {
                if let Some(c) = g
                    && !self.cedges[c][e]
                {
                    continue;
                }
                let hd = self.dnode[self.rg.edges[e].head].expect("derived node").1;
                if hd == tl {
                    continue;
                }
                let (de, new) = if hd > tl {
                    dg.edge(tl, hd)
                } else {
                    dg.edge(hd, tl)
                };
                dg.edges[de].dist = self.rg.edges[e].dist;
                dg.edges[de].factor = self.rg.edges[e].factor;
                dg.nodes[hd].wdeg += 1;
                dg.nodes[tl].wdeg += 1;
                if new {
                    dg.nodes[hd].deg += 1;
                    dg.nodes[tl].deg += 1;
                }
                dg.edges[de].reals.push(e);
            }
        }

        if let Some(c) = g {
            for pp in self.ports[c].clone() {
                let Some((u, m)) = self.dnode[pp.n] else {
                    continue;
                };
                if u != uid {
                    continue;
                }
                let dn = dg.add_node(DNode::new(DKind::Port));
                let (de, _) = dg.edge(m, dn);
                dg.edges[de].dist = self.rg.edges[pp.e].dist;
                dg.edges[de].factor = self.rg.edges[pp.e].factor;
                dg.edges[de].reals.push(pp.e);
                dg.nodes[dn].wdeg += 1;
                dg.nodes[m].wdeg += 1;
                dg.nodes[dn].deg += 1;
                dg.nodes[m].deg += 1;
                dg.ports.push(DPort {
                    n: dn,
                    alpha: pp.alpha,
                    e: de,
                });
            }
        }
        dg.sort_adjacency();
        Some(dg)
    }

    /// `getEdgeList(n, g)`: edges of `n` sorted counterclockwise.
    fn get_edge_list(dg: &DG, comp: &Comp, n: usize) -> Vec<ERec> {
        let mut erecs: Vec<ERec> = dg
            .all_edges(comp, n)
            .into_iter()
            .map(|e| {
                let ed = &dg.edges[e];
                let m = if ed.h == n { ed.t } else { ed.h };
                let dx = dg.nodes[m].pos[0] - dg.nodes[n].pos[0];
                let dy = dg.nodes[m].pos[1] - dg.nodes[n].pos[1];
                ERec {
                    e,
                    alpha: dy.atan2(dx),
                    dist2: dx.mul_add(dx, dy * dy),
                }
            })
            .collect();
        qsort_by(&mut erecs, |a, b| {
            if a.alpha > b.alpha {
                1
            } else if a.alpha < b.alpha {
                -1
            } else if a.dist2 > b.dist2 {
                1
            } else if a.dist2 < b.dist2 {
                -1
            } else {
                0
            }
        });
        let deg = erecs.len();
        if deg >= 2 {
            let mut i = 0;
            while i < deg - 1 {
                let a = erecs[i].alpha;
                let mut j = i + 1;
                while j < deg && erecs[j].alpha == a {
                    j += 1;
                }
                if j == i + 1 {
                    i = j;
                } else {
                    let bnd = if j == deg { PI } else { erecs[j].alpha };
                    let delta = ((bnd - a) / (j - i) as f64).min(ANG);
                    let mut inc = 0.0;
                    while i < j {
                        erecs[i].alpha += inc;
                        inc += delta;
                        i += 1;
                    }
                }
            }
        }
        erecs
    }

    /// `genPorts(n, er, pp, idx, bnd)`.
    fn gen_ports(
        &self,
        dg: &DG,
        n: usize,
        er: &ERec,
        pp: &mut [BPort],
        idx: usize,
        bnd: f64,
    ) -> usize {
        let e = &dg.edges[er.e];
        let cnt = e.reals.len();
        let other = if e.h == n { e.t } else { e.h };
        let mut delta = ((bnd - er.alpha) / cnt as f64).min(ANG);
        let mut angle = er.alpha;
        let (mut i, inc): (isize, isize) = if n < other {
            (idx as isize, 1)
        } else {
            angle = delta.mul_add((cnt - 1) as f64, angle);
            delta = -delta;
            ((idx + cnt - 1) as isize, -1)
        };
        for &el in &e.reals {
            let t = self.rg.edges[el].tail;
            let h = self.rg.edges[el].head;
            let pn = if self.dnode[t] == Some((dg.uid, n)) {
                t
            } else {
                h
            };
            pp[i as usize] = BPort {
                e: el,
                n: pn,
                alpha: angle,
            };
            i += inc;
            angle += delta;
        }
        idx + cnt
    }

    /// `expandCluster(n, cg)`: ports of cluster `c` from the layout of `cg`.
    fn expand_cluster(&mut self, dg: &DG, comp: &Comp, n: usize, c: usize) {
        let sz = dg.nodes[n].wdeg as usize;
        if sz == 0 {
            return;
        }
        let mut pp = vec![
            BPort {
                e: 0,
                n: 0,
                alpha: 0.0
            };
            sz
        ];
        let es = Self::get_edge_list(dg, comp, n);
        let mut idx = 0;
        for k in 0..es.len() {
            let bnd = if k + 1 < es.len() {
                es[k + 1].alpha
            } else {
                2.0 * PI + es[0].alpha
            };
            idx = self.gen_ports(dg, n, &es[k], &mut pp, idx, bnd);
        }
        self.ports[c] = pp;
    }

    /// `finalCC(g, cc, pts, rg, infop)`.
    fn final_cc(
        &self,
        dg: &mut DG,
        cc: &[Comp],
        bbs: &[BoundingBox],
        pts: Option<&[Point]>,
        g: Option<usize>,
    ) -> BoundingBox {
        let is_root = g.is_none();
        let mut is_empty = false;
        let mut bb;
        if !cc.is_empty() {
            bb = bbs[0];
            if cc.len() > 1 {
                let pts = pts.expect("packing positions");
                let pt = pts[0];
                bb = BoundingBox::new(
                    bb.ll.x + pt.x,
                    bb.ll.y + pt.y,
                    bb.ur.x + pt.x,
                    bb.ur.y + pt.y,
                );
                for i in 1..cc.len() {
                    let b = bbs[i];
                    let pt = pts[i];
                    let b = BoundingBox::new(
                        b.ll.x + pt.x,
                        b.ll.y + pt.y,
                        b.ur.x + pt.x,
                        b.ur.y + pt.y,
                    );
                    bb.ll.x = bb.ll.x.min(b.ll.x);
                    bb.ll.y = bb.ll.y.min(b.ll.y);
                    bb.ur.x = bb.ur.x.max(b.ur.x);
                    bb.ur.y = bb.ur.y.max(b.ur.y);
                }
            }
        } else {
            let a = self.gattrs(g);
            bb = BoundingBox::new(
                0.0,
                0.0,
                late_int(a, "width", 54, 3) as f64,
                late_int(a, "height", 36, 3) as f64,
            );
            is_empty = true;
        }
        let (label, border) = match g {
            None => (self.rg.label.as_ref(), [Point::default(); 4]),
            Some(c) => (
                self.rg.clusters[c].label.as_ref(),
                self.rg.clusters[c].border,
            ),
        };
        if let Some(l) = label {
            is_empty = false;
            let mut d = l.dimen.x.round() - (bb.ur.x - bb.ll.x);
            if d > 0.0 {
                d /= 2.0;
                bb.ll.x -= d;
                bb.ur.x += d;
            }
        }
        let margin = if is_root || is_empty {
            0
        } else {
            late_int(self.gattrs(g), "margin", CL_OFFSET, 0)
        } as f64;
        let pt = Point::new(-bb.ll.x + margin, -bb.ll.y + margin + border[0].y);
        bb.ll = Point::new(0.0, 0.0);
        bb.ur.x += pt.x + margin;
        bb.ur.y += pt.y + margin + border[2].y;

        for (i, comp) in cc.iter().enumerate() {
            let p = match pts {
                Some(pts) => Point::new(pts[i].x + pt.x, pts[i].y + pt.y),
                None => pt,
            };
            let del = Point::new(p.x / 72.0, p.y / 72.0);
            for &n in &comp.nodes {
                dg.nodes[n].pos[0] += del.x;
                dg.nodes[n].pos[1] += del.y;
            }
        }
        BoundingBox::new(
            bb.ll.x / 72.0,
            bb.ll.y / 72.0,
            bb.ur.x / 72.0,
            bb.ur.y / 72.0,
        )
    }

    /// `layout(g, infop, counter)`; `Err` on non-comparable clusters.
    fn layout(&mut self, g: Option<usize>) -> std::result::Result<(), ()> {
        for n in self.members_of(g) {
            self.dnode[n] = None;
        }
        let mut dg = self.derive_graph(g).ok_or(())?;
        let (mut cc, pinned) = find_ccomp(&dg);

        for ci in 0..cc.len() {
            let mut xpms = XParams::default();
            fdp_t_layout(&mut self.st, &mut dg, &cc[ci], &mut xpms);
            for n in cc[ci].nodes.clone() {
                match dg.nodes[n].kind {
                    DKind::Clust(c) => {
                        self.expand_cluster(&dg, &cc[ci], n, c);
                        self.layout(Some(c))?;
                        let bb = self.gbb[c];
                        let dn = &mut dg.nodes[n];
                        dn.width = bb.ur.x;
                        dn.height = bb.ur.y;
                        let pt = Point::new(72.0 * bb.ur.x, 72.0 * bb.ur.y);
                        dn.lw = pt.x / 2.0;
                        dn.rw = pt.x / 2.0;
                        dn.ht = pt.y;
                    }
                    DKind::Port => cc[ci].remove(n),
                    DKind::Real(_) => {}
                }
            }
            if cc[ci].nodes.len() >= 2 {
                // normalize(cg) reads `normalize` on the derived graph, which
                // never declares it.
                fdp_x_layout(&mut self.st, &mut dg, &cc[ci], &xpms);
            }
        }

        let keep: Vec<usize> = (0..dg.nodes.len())
            .filter(|&n| cc.iter().any(|c| c.member[n]))
            .collect();
        let mut pinfo = self.pack.clone();
        let (bbs, pts) = with_lgraph(&mut dg, &keep, &mut self.st.crand, |lg, _, map| {
            let mut subs: Vec<Sub> = cc
                .iter()
                .map(|c| {
                    let nodes = c.nodes.iter().map(|&n| map[n]).collect();
                    Sub::new(lg.nodes.len(), nodes)
                })
                .collect();
            let pts = if subs.len() > 1 {
                if pinned {
                    let mut bp = vec![false; subs.len()];
                    bp[0] = true;
                    pinfo.fixed = Some(bp);
                } else {
                    pinfo.fixed = None;
                }
                put_graphs(lg, &mut subs, &mut pinfo)
            } else {
                if let Some(s) = subs.first_mut() {
                    compute_bb(lg, s, false);
                }
                None
            };
            (subs.iter().map(|s| s.bb).collect::<Vec<_>>(), pts)
        });
        let bb = self.final_cc(&mut dg, &cc, &bbs, pts.as_deref(), g);

        for dn in &dg.nodes {
            match dn.kind {
                DKind::Clust(c) => {
                    let llx = dn.pos[0] - dn.width / 2.0;
                    let lly = dn.pos[1] - dn.height / 2.0;
                    self.gbb[c] = BoundingBox::new(llx, lly, llx + dn.width, lly + dn.height);
                }
                DKind::Real(n) => {
                    self.rg.nodes[n].pos[0] = dn.pos[0];
                    self.rg.nodes[n].pos[1] = dn.pos[1];
                }
                DKind::Port => {}
            }
        }
        self.set_bb(g, bb);
        Ok(())
    }

    /// `evalPositions(g, rootg)`.
    fn eval_positions(&mut self, g: Option<usize>) {
        let bb = self.bb_of(g);
        if g.is_some() {
            for n in self.members_of(g) {
                if self.parent[n] != Some(g) {
                    continue;
                }
                self.rg.nodes[n].pos[0] += bb.ll.x;
                self.rg.nodes[n].pos[1] += bb.ll.y;
            }
        }
        for c in self.tree.kids(g).to_vec() {
            if g.is_some() {
                let s = &mut self.gbb[c];
                s.ll.x += bb.ll.x;
                s.ll.y += bb.ll.y;
                s.ur.x += bb.ll.x;
                s.ur.y += bb.ll.y;
            }
            self.eval_positions(Some(c));
        }
    }
}

/// `fdpSplines(g)`.
fn fdp_splines(g: &mut LGraph<'_>) {
    let mut et = g.edge_type;
    let root = g.root_sub();
    if et == EdgeType::Ortho || et == EdgeType::CompoundEdge {
        // lib/ortho and compoundEdges are not ported; fall back as
        // spline_edges0 does.
        et = EdgeType::Pline;
    }
    spline_edges_main(g, &root, et);
}

/// `fdp_layout(g)`.
pub fn fdp_layout(graph: &mut Graph) -> Result<()> {
    let src = graph.clone();
    let attrs = &src.graph_attrs;
    let ndim = late_int(attrs, "dim", 2, 2).min(MAXDIM as i64) as usize;
    let mut g = LGraph::new(&src, ndim, EdgeType::Line);
    let tree = mk_clusters(&src);
    register_clusters(&mut g, &tree);
    let st = fdp_init_params(attrs);
    init_edges(&mut g, st.d.k);
    let inputscale = get_inputscale(&g);
    initial_positions(&mut g, inputscale);

    let mut pack = PackInfo {
        margin: get_pack(g.gattr("pack"), CL_OFFSET / 2, CL_OFFSET / 2) as u32,
        ..PackInfo::default()
    };
    pack.mode = get_pack_mode_info(g.gattr("packmode"), PackMode::Node, &mut pack);

    let nc = tree.len();
    let members: Vec<Vec<usize>> = tree
        .subgraph
        .iter()
        .map(|&si| subgraph_member_nodes(&src, si))
        .collect();
    let cedges: Vec<Vec<bool>> = tree
        .subgraph
        .iter()
        .map(|&si| {
            let mut v = vec![false; src.edges.len()];
            for e in crate::neatogen::neatoinit::subgraph_edges(&src, si) {
                v[e] = true;
            }
            v
        })
        .collect();
    let coords_declared = src.graph_attrs.contains_key("coords")
        || src
            .subgraphs
            .iter()
            .any(|s| s.graph_attrs.contains_key("coords"));
    let n = g.nodes.len();
    let mut f = Fdp {
        rg: &mut g,
        tree,
        st,
        ports: vec![Vec::new(); nc],
        gbb: vec![BoundingBox::default(); nc],
        root_bb: BoundingBox::default(),
        members,
        cedges,
        parent: vec![None; n],
        dnode: vec![None; n],
        next_uid: 0,
        pack,
        coords_declared,
        inputscale,
    };
    if f.layout(None).is_err() {
        // C reports the error and returns before positioning anything.
        g.write_back(graph);
        return Ok(());
    }
    f.eval_positions(None);
    // setBB
    let root_bb = f.root_bb;
    let gbb = f.gbb.clone();
    let sc = |b: BoundingBox| {
        BoundingBox::new(72.0 * b.ll.x, 72.0 * b.ll.y, 72.0 * b.ur.x, 72.0 * b.ur.y)
    };
    g.bb = sc(root_bb);
    for (c, b) in gbb.into_iter().enumerate() {
        g.clusters[c].bb = sc(b);
    }

    let root = g.root_sub();
    neato_set_aspect(&mut g, &root, true);
    if g.edge_type != EdgeType::None {
        fdp_splines(&mut g);
    }
    gv_postprocess(&mut g, false);
    g.write_back(graph);
    Ok(())
}
