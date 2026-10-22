//! Port of Graphviz `lib/ortho` (`splines=ortho`): orthogonal edge routing.
//!
//! - [`maze`] — `maze.c`: node cells, free cells from [`partition`], and the
//!   search graph over cell sides
//! - [`partition`] / [`trapezoid`] — `partition.c`, `trapezoid.c`: free-space
//!   decomposition by Seidel trapezoidation
//! - [`sgraph`] — `sgraph.c`, `fPQ.c`: search graph and Dijkstra
//! - [`rawgraph`] — `rawgraph.c`: track ordering graphs
//! - this module — `ortho.c`: routing, channel and track assignment
//!
//! The router works on any layout implementing [`OrthoGraph`].

// Faithful ports of C loops index several arrays in lockstep.
#![allow(
    clippy::needless_range_loop,
    clippy::too_many_arguments,
    clippy::neg_multiply,
    clippy::needless_late_init
)]

pub mod maze;
pub mod partition;
pub mod rawgraph;
pub mod sgraph;
pub mod trapezoid;

use crate::cdt::Cdt;
use crate::common::splines::{SplineGraph, SplineInfo, clip_and_install_info};
use graphviz_pure_core::Point;
use maze::{Cell, M_BOTTOM, M_LEFT, M_RIGHT, M_TOP, MZ_HSCAN, MZ_VSCAN, Maze, mk_maze, update_wts};
use rawgraph::RawGraph;
use sgraph::{Pq, SGraph};
use std::cmp::Ordering;

/// The layout state `orthoEdges` needs beyond [`SplineGraph`].
pub trait OrthoGraph: SplineGraph {
    /// Nodes in `agfstnode` order.
    fn ortho_nodes(&self) -> Vec<usize>;
    /// Out-edges of `n` in `agfstout` order.
    fn ortho_out_edges(&self, n: usize) -> Vec<usize>;
    /// The `Concentrate` global.
    fn ortho_concentrate(&self) -> bool;
    /// `AGSEQ(n)`.
    fn node_seq(&self, n: usize) -> usize;
    /// `agnameof(n)` (debug dumps).
    fn xl_name(&self, n: usize) -> String;
}

/// `bend`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Bend {
    #[default]
    Node,
    Up,
    Left,
    Down,
    Right,
}

/// `segment`.
#[derive(Debug, Clone, Copy, Default)]
struct Segment {
    is_vert: bool,
    comm_coord: f64,
    p1: f64,
    p2: f64,
    l1: Bend,
    l2: Bend,
    ind_no: Option<usize>,
    track_no: Option<i32>,
    prev: Option<usize>,
    next: Option<usize>,
}

/// A segment of a route: (route index, segment index).
type SegRef = (usize, usize);

/// `channel`.
#[derive(Debug, Clone, Default)]
struct Channel {
    p1: f64,
    p2: f64,
    seg_list: Vec<SegRef>,
    g: RawGraph,
    cp: usize,
}

/// `chanItem`.
#[derive(Debug, Clone)]
struct ChanItem {
    v: f64,
    chans: Cdt<Channel>,
}

fn fcmp(a: f64, b: f64) -> Ordering {
    if a < b {
        Ordering::Less
    } else if a > b {
        Ordering::Greater
    } else {
        Ordering::Equal
    }
}

/// `chancmpid(k1, k2)`: intervals compare equal when one contains the other.
fn chancmpid(k1: (f64, f64), k2: (f64, f64)) -> Ordering {
    if k1.0 > k2.0 {
        if k1.1 <= k2.1 {
            return Ordering::Equal;
        }
        return Ordering::Greater;
    }
    if k1.0 < k2.0 {
        if k1.1 >= k2.1 {
            return Ordering::Equal;
        }
        return Ordering::Less;
    }
    Ordering::Equal
}

/// A channel dictionary (`Dt_t*` of `chanItem`s, each with a `Dt_t*` of
/// channels), with cdt's splay-tree semantics.
#[derive(Default)]
struct Chans {
    items: Cdt<ChanItem>,
}

/// (chanItem node, channel node) in a [`Chans`].
type ChanKey = (usize, usize);

impl Chans {
    /// `addChan(chdict, cp, j)`.
    fn add(&mut self, chp: Channel, j: f64) {
        let ii = match self.items.find(&|it: &ChanItem| fcmp(j, it.v)) {
            Some(i) => i,
            None => {
                self.items
                    .insert(
                        ChanItem {
                            v: j,
                            chans: Cdt::new(),
                        },
                        &|it: &ChanItem| fcmp(j, it.v),
                    )
                    .0
            }
        };
        let key = (chp.p1, chp.p2);
        self.items
            .get_mut(ii)
            .chans
            .insert(chp, &|c: &Channel| chancmpid(key, (c.p1, c.p2)));
    }

    /// `chanSearch(chans, seg)`, with `None` where C's asserts fail.
    fn find(&mut self, seg: &Segment) -> Option<ChanKey> {
        let v = seg.comm_coord;
        let i = self.items.find(&|it: &ChanItem| fcmp(v, it.v))?;
        let key = (seg.p1, seg.p2);
        let c = self
            .items
            .get_mut(i)
            .chans
            .find(&|c: &Channel| chancmpid(key, (c.p1, c.p2)))?;
        Some((i, c))
    }

    /// `chanSearch(chans, seg)` for a segment that [`Ortho::assign_segs`]
    /// has already placed in a channel. It cannot miss: segments are not moved
    /// after `assign_segs`, and splaying reorders the dictionaries without
    /// changing which channels they hold.
    fn search(&mut self, seg: &Segment) -> ChanKey {
        self.find(seg).expect("chanSearch: no channel")
    }

    fn chan(&self, k: ChanKey) -> &Channel {
        self.items.get(k.0).chans.get(k.1)
    }

    fn chan_mut(&mut self, k: ChanKey) -> &mut Channel {
        self.items.get_mut(k.0).chans.get_mut(k.1)
    }
}

struct Ortho {
    mp: Maze,
    routes: Vec<Vec<Segment>>,
    hchans: Chans,
    vchans: Chans,
}

/// `cellOf(p, q)`: the cell shared by two snodes.
fn cell_of(sg: &SGraph, p: usize, q: usize) -> usize {
    let cp = sg.nodes[p].cells[0];
    if cp == sg.nodes[q].cells[0] || cp == sg.nodes[q].cells[1] {
        return cp.unwrap();
    }
    sg.nodes[p].cells[1].unwrap()
}

fn mid(a: f64, b: f64) -> f64 {
    (a + b) / 2.0
}

fn mid_pt(cp: &Cell) -> Point {
    Point::new(mid(cp.bb.ll.x, cp.bb.ur.x), mid(cp.bb.ll.y, cp.bb.ur.y))
}

/// `sidePt(ptr, cp)`.
fn side_pt(sg: &SGraph, ptr: usize, c: usize, cp: &Cell) -> Point {
    let n = &sg.nodes[ptr];
    if Some(c) == n.cells[1] {
        if n.is_vert {
            return Point::new(cp.bb.ll.x, mid(cp.bb.ll.y, cp.bb.ur.y));
        }
        return Point::new(mid(cp.bb.ll.x, cp.bb.ur.x), cp.bb.ll.y);
    }
    if n.is_vert {
        return Point::new(cp.bb.ur.x, mid(cp.bb.ll.y, cp.bb.ur.y));
    }
    Point::new(mid(cp.bb.ll.x, cp.bb.ur.x), cp.bb.ur.y)
}

/// `setSeg(dir, fix, b1, b2, l1, l2)`.
fn set_seg(dir: bool, fix: f64, b1: f64, b2: f64, l1: Bend, l2: Bend) -> Segment {
    let mut sp = Segment {
        is_vert: dir,
        comm_coord: fix,
        ..Segment::default()
    };
    if b1 < b2 {
        sp.p1 = b1;
        sp.p2 = b2;
        sp.l1 = l1;
        sp.l2 = l2;
    } else {
        sp.p2 = b1;
        sp.p1 = b2;
        sp.l2 = l1;
        sp.l1 = l2;
    }
    sp
}

/// `convertSPtoRoute(g, fst, lst)`.
fn convert_sp_to_route(mp: &mut Maze, fst: usize, lst: usize) -> Vec<Segment> {
    let dad = |sg: &SGraph, n: usize| sg.nodes[n].n_dad;
    let mut rte = Vec::new();
    let mut ptr = dad(&mp.sg, fst).unwrap();
    let mut prev = ptr;
    let mut next = dad(&mp.sg, ptr).unwrap();
    let c0 = mp.sg.nodes[ptr].cells[0].unwrap();
    let mut cp = if mp.cells[c0].is_node() {
        mp.sg.nodes[ptr].cells[1].unwrap()
    } else {
        c0
    };
    let mut bp1 = side_pt(&mp.sg, ptr, cp, &mp.cells[cp]);
    let mut prevbp = Point::new(0.0, 0.0);
    let first_dad = ptr;
    while let Some(nn) = dad(&mp.sg, next) {
        let ncp = cell_of(&mp.sg, prev, next);
        let pe = mp.sg.nodes[ptr].n_edge.unwrap();
        let cell = mp.cells[ncp].clone();
        update_wts(&mut mp.sg, &cell, pe);
        let pv = mp.sg.nodes[ptr].is_vert;
        let nv = mp.sg.nodes[next].is_vert;
        if pv != nv || nn == lst {
            let bp2 = if pv != nv {
                mid_pt(&mp.cells[ncp])
            } else {
                side_pt(&mp.sg, next, ncp, &mp.cells[ncp])
            };
            let (l1, l2, fix, b1, b2);
            if pv {
                l1 = if ptr == first_dad {
                    Bend::Node
                } else if prevbp.y > bp1.y {
                    Bend::Up
                } else {
                    Bend::Down
                };
                l2 = if pv != nv {
                    if mp.sg.nodes[next].cells[0] == Some(ncp) {
                        Bend::Up
                    } else {
                        Bend::Down
                    }
                } else {
                    Bend::Node
                };
                fix = mp.cells[cp].bb.ll.y;
                b1 = mp.cells[cp].bb.ll.x;
                b2 = mp.cells[ncp].bb.ll.x;
            } else {
                l1 = if ptr == first_dad {
                    Bend::Node
                } else if prevbp.x > bp1.x {
                    Bend::Right
                } else {
                    Bend::Left
                };
                l2 = if pv != nv {
                    if mp.sg.nodes[next].cells[0] == Some(ncp) {
                        Bend::Right
                    } else {
                        Bend::Left
                    }
                } else {
                    Bend::Node
                };
                fix = mp.cells[cp].bb.ll.x;
                b1 = mp.cells[cp].bb.ll.y;
                b2 = mp.cells[ncp].bb.ll.y;
            }
            rte.push(set_seg(!pv, fix, b1, b2, l1, l2));
            cp = ncp;
            prevbp = bp1;
            bp1 = bp2;
            if pv != nv && nn == lst {
                let (l1, fix, b1, b2);
                if nv {
                    l1 = if prevbp.y > bp1.y {
                        Bend::Up
                    } else {
                        Bend::Down
                    };
                    fix = mp.cells[cp].bb.ll.y;
                    b1 = mp.cells[cp].bb.ll.x;
                    b2 = mp.cells[ncp].bb.ll.x;
                } else {
                    l1 = if prevbp.x > bp1.x {
                        Bend::Right
                    } else {
                        Bend::Left
                    };
                    fix = mp.cells[cp].bb.ll.x;
                    b1 = mp.cells[cp].bb.ll.y;
                    b2 = mp.cells[ncp].bb.ll.y;
                }
                rte.push(set_seg(!nv, fix, b1, b2, l1, Bend::Node));
            }
            ptr = next;
        }
        prev = next;
        next = nn;
    }
    let n = rte.len();
    for i in 0..n {
        if i > 0 {
            rte[i].prev = Some(i - 1);
        }
        if i + 1 < n {
            rte[i].next = Some(i + 1);
        }
    }
    rte
}

/// `extractHChans(mp)` / `extractVChans(mp)`.
fn extract_chans(mp: &mut Maze, horizontal: bool) -> Chans {
    let mut chans = Chans::default();
    let (back, fwd, flag) = if horizontal {
        (M_LEFT, M_RIGHT, MZ_HSCAN)
    } else {
        (M_BOTTOM, M_TOP, MZ_VSCAN)
    };
    for i in mp.ngcells..mp.cells.len() {
        let mut cp = i;
        if mp.cells[cp].flags & flag != 0 {
            continue;
        }
        while let Some(np) = mp.cells[cp].sides[back] {
            match mp.sg.nodes[np].cells[0] {
                Some(nc) if !mp.cells[nc].is_node() => cp = nc,
                _ => break,
            }
        }
        let start = cp;
        let p1 = if horizontal {
            mp.cells[cp].bb.ll.x
        } else {
            mp.cells[cp].bb.ll.y
        };
        mp.cells[cp].flags |= flag;
        while let Some(np) = mp.cells[cp].sides[fwd] {
            match mp.sg.nodes[np].cells[1] {
                Some(nc) if !mp.cells[nc].is_node() => {
                    cp = nc;
                    mp.cells[cp].flags |= flag;
                }
                _ => break,
            }
        }
        let p2 = if horizontal {
            mp.cells[cp].bb.ur.x
        } else {
            mp.cells[cp].bb.ur.y
        };
        let key = if horizontal {
            mp.cells[start].bb.ll.y
        } else {
            mp.cells[start].bb.ll.x
        };
        chans.add(
            Channel {
                p1,
                p2,
                cp: start,
                ..Channel::default()
            },
            key,
        );
    }
    chans
}

fn eq_end_seg(s1l2: Bend, s2l2: Bend, t1: Bend, t2: Bend) -> i32 {
    if (s1l2 == t2 && s2l2 != t2) || (s1l2 == Bend::Node && s2l2 == t1) {
        0
    } else {
        -1
    }
}

fn overlap_seg(s1: &Segment, s2: &Segment, t1: Bend, t2: Bend) -> i32 {
    if s1.p2 < s2.p2 {
        if s1.l2 == t1 && s2.l1 == t2 {
            return -1;
        }
        if s1.l2 == t2 && s2.l1 == t1 {
            return 1;
        }
        return 0;
    }
    if s1.p2 > s2.p2 {
        if s2.l1 == t2 && s2.l2 == t2 {
            return -1;
        }
        if s2.l1 == t1 && s2.l2 == t1 {
            return 1;
        }
        return 0;
    }
    if s2.l1 == t2 {
        return eq_end_seg(s1.l2, s2.l2, t1, t2);
    }
    -eq_end_seg(s2.l2, s1.l2, t1, t2)
}

fn ell_seg(s1l1: Bend, s1l2: Bend, t: Bend) -> i32 {
    if s1l1 == t {
        if s1l2 == t {
            return -1;
        }
        return 0;
    }
    1
}

fn seg_cmp_(s1: &Segment, s2: &Segment, t1: Bend, t2: Bend) -> i32 {
    if s1.p2 < s2.p1 || s1.p1 > s2.p2 {
        return 0;
    }
    if s1.p1 < s2.p1 && s2.p1 < s1.p2 {
        return overlap_seg(s1, s2, t1, t2);
    }
    if s2.p1 < s1.p1 && s1.p1 < s2.p2 {
        return -overlap_seg(s2, s1, t1, t2);
    }
    if s1.p1 == s2.p1 {
        if s1.p2 < s2.p2 {
            if s1.l2 == t1 {
                return eq_end_seg(s2.l1, s1.l1, t1, t2);
            }
            return -eq_end_seg(s2.l1, s1.l1, t1, t2);
        }
        if s1.p2 > s2.p2 {
            if s2.l2 == t2 {
                return eq_end_seg(s1.l1, s2.l1, t1, t2);
            }
            return -eq_end_seg(s1.l1, s2.l1, t1, t2);
        }
        if s1.l1 == s2.l1 && s1.l2 == s2.l2 {
            return 0;
        }
        if s2.l1 == s2.l2 {
            if s2.l1 == t1 {
                return 1;
            }
            if s2.l1 == t2 {
                return -1;
            }
            if s1.l1 != t1 && s1.l2 != t1 {
                return 1;
            }
            if s1.l1 != t2 && s1.l2 != t2 {
                return -1;
            }
            return 0;
        }
        if s2.l1 == t1 && s2.l2 == t2 {
            if s1.l1 != t1 && s1.l2 == t2 {
                return 1;
            }
            if s1.l1 == t1 && s1.l2 != t2 {
                return -1;
            }
            return 0;
        }
        if s2.l2 == t1 && s2.l1 == t2 {
            if s1.l2 != t1 && s1.l1 == t2 {
                return 1;
            }
            if s1.l2 == t1 && s1.l1 != t2 {
                return -1;
            }
            return 0;
        }
        if s2.l1 == Bend::Node && s2.l2 == t1 {
            return ell_seg(s1.l1, s1.l2, t1);
        }
        if s2.l1 == Bend::Node && s2.l2 == t2 {
            return -ell_seg(s1.l1, s1.l2, t2);
        }
        if s2.l1 == t1 && s2.l2 == Bend::Node {
            return ell_seg(s1.l2, s1.l1, t1);
        }
        return -ell_seg(s1.l2, s1.l1, t2);
    }
    if s1.p2 == s2.p1 {
        if s1.l2 == s2.l1 {
            return 0;
        }
        if s1.l2 == t2 {
            return 1;
        }
        return -1;
    }
    if s1.l1 == s2.l2 {
        return 0;
    }
    if s1.l1 == t2 {
        return 1;
    }
    -1
}

/// `seg_cmp(S1, S2)`; -2 when incomparable.
fn seg_cmp(s1: &Segment, s2: &Segment) -> i32 {
    if s1.is_vert != s2.is_vert || s1.comm_coord != s2.comm_coord {
        return -2;
    }
    if s1.is_vert {
        seg_cmp_(s1, s2, Bend::Right, Bend::Left)
    } else {
        seg_cmp_(s1, s2, Bend::Down, Bend::Up)
    }
}

fn is_parallel(s1: &Segment, s2: &Segment) -> bool {
    s1.p1 == s2.p1 && s1.p2 == s2.p2 && s1.l1 == s2.l1 && s1.l2 == s2.l2
}

impl Ortho {
    fn seg(&self, r: SegRef) -> &Segment {
        &self.routes[r.0][r.1]
    }

    fn next_seg(&self, r: SegRef, dir: i32) -> Option<SegRef> {
        let s = self.seg(r);
        let n = if dir == 0 { s.prev } else { s.next };
        n.map(|i| (r.0, i))
    }

    fn chans_mut(&mut self, vert: bool) -> &mut Chans {
        if vert {
            &mut self.vchans
        } else {
            &mut self.hchans
        }
    }

    fn chan_key(&mut self, r: SegRef) -> (bool, ChanKey) {
        let s = *self.seg(r);
        (s.is_vert, self.chans_mut(s.is_vert).search(&s))
    }

    /// The nested `dtflatten`/`dtlink` loop over a channel dictionary. The
    /// `dtlink` pointers are read after each body, which may have searched
    /// (and so restored and splayed) the same dictionaries, as in C.
    fn for_each_chan(
        &mut self,
        vert: bool,
        f: &mut dyn FnMut(&mut Ortho, ChanKey) -> bool,
    ) -> bool {
        let mut l1 = self.chans_mut(vert).items.flatten();
        while let Some(i) = l1 {
            let mut l2 = self.chans_mut(vert).items.get_mut(i).chans.flatten();
            while let Some(j) = l2 {
                if !f(self, (i, j)) {
                    return false;
                }
                l2 = self.chans_for_kind(vert).items.get(i).chans.link(j);
            }
            l1 = self.chans_for_kind(vert).items.link(i);
        }
        true
    }

    fn chan_g(&mut self, k: (bool, (usize, usize))) -> &mut RawGraph {
        if k.0 {
            &mut self.vchans.chan_mut(k.1).g
        } else {
            &mut self.hchans.chan_mut(k.1).g
        }
    }

    fn chan_g_ref(&self, k: (bool, (usize, usize))) -> &RawGraph {
        if k.0 {
            &self.vchans.chan(k.1).g
        } else {
            &self.hchans.chan(k.1).g
        }
    }

    /// `assignSegs(nrtes, route_list, mp)`. Returns false where C's
    /// `chanSearch` fails `assert (chani)` or `assert (cp)` (ortho.c:388/390)
    /// and aborts (#170): a segment of a route lies in no channel of the maze.
    fn assign_segs(&mut self) -> bool {
        for i in 0..self.routes.len() {
            for j in 0..self.routes[i].len() {
                let s = self.routes[i][j];
                let chans = if s.is_vert {
                    &mut self.vchans
                } else {
                    &mut self.hchans
                };
                let Some(k) = chans.find(&s) else {
                    return false;
                };
                let chan = chans.chan_mut(k);
                let ind = chan.seg_list.len();
                chan.seg_list.push((i, j));
                if std::env::var_os("GVDUMP").is_some() {
                    let (p1, p2) = (chan.p1, chan.p2);
                    let s = &self.routes[i][j];
                    eprintln!(
                        "OAS {i} {j} {} {:.6} [{p1:.6} {p2:.6}] {ind}",
                        i32::from(s.is_vert),
                        s.comm_coord
                    );
                }
                self.routes[i][j].ind_no = Some(ind);
            }
        }
        true
    }

    /// `add_edges_in_G(cp)`.
    fn add_edges_in_g(&mut self, vert: bool, k: (usize, usize)) -> bool {
        let chans = if vert { &self.vchans } else { &self.hchans };
        let segs = chans.chan(k).seg_list.clone();
        let size = segs.len();
        for x in 0..size.saturating_sub(1) {
            for y in x + 1..size {
                let cmp = seg_cmp(self.seg(segs[x]), self.seg(segs[y]));
                let g = self.chan_g((vert, k));
                if cmp == -2 {
                    return false;
                } else if cmp > 0 {
                    g.insert_edge(x, y);
                } else if cmp == -1 {
                    g.insert_edge(y, x);
                }
            }
        }
        true
    }

    /// `propagate_prec(seg, prec, hops, dir)`.
    fn propagate_prec(&self, seg: SegRef, prec: i32, hops: i32, dir: i32) -> i32 {
        let mut ans = prec;
        let mut current = seg;
        for _ in 1..=hops {
            let next = self.next_seg(current, dir).unwrap();
            let c = *self.seg(current);
            let n = self.seg(next);
            if !c.is_vert {
                if n.comm_coord == c.p1 {
                    if c.l1 == Bend::Up {
                        ans *= -1;
                    }
                } else if c.l2 == Bend::Down {
                    ans *= -1;
                }
            } else if n.comm_coord == c.p1 {
                if c.l1 == Bend::Right {
                    ans *= -1;
                }
            } else if c.l2 == Bend::Left {
                ans *= -1;
            }
            current = next;
        }
        ans
    }

    /// `decide_point(ret, si, sj, dir1, dir2)`: (hops, prec), `None` on error.
    fn decide_point(
        &self,
        mut si: SegRef,
        mut sj: SegRef,
        dir1: i32,
        dir2: i32,
    ) -> Option<(i32, i32)> {
        let mut ans = 0;
        let mut np1;
        let mut np2 = None;
        loop {
            np1 = self.next_seg(si, dir1);
            let Some(a) = np1 else { break };
            np2 = self.next_seg(sj, dir2);
            let Some(b) = np2 else { break };
            if !is_parallel(self.seg(a), self.seg(b)) {
                break;
            }
            ans += 1;
            si = a;
            sj = b;
        }
        let prec = match (np1, np2) {
            (None, _) => 0,
            (Some(_), None) => panic!("decide_point: unmatched segment"),
            (Some(a), Some(b)) => {
                let temp = seg_cmp(self.seg(a), self.seg(b));
                if temp == -2 {
                    return None;
                }
                self.propagate_prec(a, temp, ans + 1, 1 - dir1)
            }
        };
        Some((ans, prec))
    }

    /// `set_parallel_edges(seg1, seg2, dir1, dir2, hops, mp)`.
    fn set_parallel_edges(
        &mut self,
        mut seg1: SegRef,
        mut seg2: SegRef,
        dir1: i32,
        dir2: i32,
        hops: i32,
    ) {
        let mut chan = self.chan_key(seg1);
        let (i1, i2) = (
            self.seg(seg1).ind_no.unwrap(),
            self.seg(seg2).ind_no.unwrap(),
        );
        self.chan_g(chan).insert_edge(i1, i2);
        for _ in 1..=hops {
            let prev1 = self.next_seg(seg1, dir1).unwrap();
            let prev2 = self.next_seg(seg2, dir2).unwrap();
            let s1 = *self.seg(seg1);
            let (a, b) = (s1.ind_no.unwrap(), self.seg(seg2).ind_no.unwrap());
            let (p1i, p2i) = (
                self.seg(prev1).ind_no.unwrap(),
                self.seg(prev2).ind_no.unwrap(),
            );
            let nchan = (!s1.is_vert, {
                let p = *self.seg(prev1);
                self.chans_mut(!s1.is_vert).search(&p)
            });
            let exists = self.chan_g_ref(chan).edge_exists(a, b);
            let at_p1 = self.seg(prev1).comm_coord == s1.p1;
            let first_bend = if !s1.is_vert { Bend::Up } else { Bend::Left };
            // forward: insert prev1 -> prev2; else prev2 -> prev1
            let forward = if at_p1 {
                let hit = s1.l1 == first_bend;
                if !s1.is_vert {
                    // B_UP: exists ? (p2,p1) : (p1,p2); else exists ? (p1,p2) : (p2,p1)
                    if hit { !exists } else { exists }
                } else {
                    // B_LEFT: exists ? (p1,p2) : (p2,p1); else exists ? (p2,p1) : (p1,p2)
                    if hit { exists } else { !exists }
                }
            } else {
                let hit = s1.l2 == first_bend;
                if !s1.is_vert {
                    if hit { exists } else { !exists }
                } else if hit {
                    !exists
                } else {
                    exists
                }
            };
            let g = self.chan_g(nchan);
            if forward {
                g.insert_edge(p1i, p2i);
            } else {
                g.insert_edge(p2i, p1i);
            }
            if std::env::var_os("GVDUMP").is_some() {
                let g = self.chan_g_ref(nchan);
                eprintln!(
                    "OSP {:.6} {p1i} {p2i} {} {}",
                    self.seg(prev1).comm_coord,
                    i32::from(g.edge_exists(p1i, p2i)),
                    i32::from(g.edge_exists(p2i, p1i))
                );
            }
            chan = nchan;
            seg1 = prev1;
            seg2 = prev2;
        }
    }

    fn chans_for_kind(&self, vert: bool) -> &Chans {
        if vert { &self.vchans } else { &self.hchans }
    }

    /// `removeEdge(seg1, seg2, dir, mp)`.
    fn remove_edge(&mut self, seg1: SegRef, seg2: SegRef, dir: i32) {
        let mut ptr1 = seg1;
        let mut ptr2 = seg2;
        while is_parallel(self.seg(ptr1), self.seg(ptr2)) {
            ptr1 = self.next_seg(ptr1, 1).unwrap();
            ptr2 = self.next_seg(ptr2, dir).unwrap();
        }
        let chan = self.chan_key(ptr1);
        let (a, b) = (
            self.seg(ptr1).ind_no.unwrap(),
            self.seg(ptr2).ind_no.unwrap(),
        );
        self.chan_g(chan).remove_redge(a, b);
    }

    /// `addPEdges(cp, mp)`.
    fn add_p_edges_chan(&mut self, vert: bool, k: (usize, usize)) -> bool {
        let segs = self.chans_for_kind(vert).chan(k).seg_list.clone();
        let n = segs.len();
        for i in 0..n.saturating_sub(1) {
            for j in i + 1..n {
                let g = self.chan_g_ref((vert, k));
                if g.edge_exists(i, j) || g.edge_exists(j, i) {
                    continue;
                }
                let (si, sj) = (segs[i], segs[j]);
                if !is_parallel(self.seg(si), self.seg(sj)) {
                    continue;
                }
                let (pi, pj) = (self.seg(si).prev, self.seg(sj).prev);
                let dir = match (pi, pj) {
                    (None, None) => 0,
                    (None, Some(_)) | (Some(_), None) => 1,
                    (Some(a), Some(b)) => {
                        if self.routes[si.0][a].comm_coord == self.routes[sj.0][b].comm_coord {
                            0
                        } else {
                            1
                        }
                    }
                };
                let Some((h1, prec1)) = self.decide_point(si, sj, 0, dir) else {
                    return false;
                };
                let Some((h2, prec2)) = self.decide_point(si, sj, 1, 1 - dir) else {
                    return false;
                };
                if std::env::var_os("GVDUMP").is_some() {
                    eprintln!(
                        "OPE {:.6} {i} {j} {dir} {h1} {prec1} {h2} {prec2}",
                        self.seg(si).comm_coord
                    );
                }
                if prec1 == -1 {
                    self.set_parallel_edges(sj, si, dir, 0, h1);
                    self.set_parallel_edges(sj, si, 1 - dir, 1, h2);
                    if prec2 == 1 {
                        self.remove_edge(si, sj, 1 - dir);
                    }
                } else if prec1 == 0 {
                    if prec2 == -1 {
                        self.set_parallel_edges(sj, si, dir, 0, h1);
                        self.set_parallel_edges(sj, si, 1 - dir, 1, h2);
                    } else if prec2 == 0 || prec2 == 1 {
                        self.set_parallel_edges(si, sj, 0, dir, h1);
                        self.set_parallel_edges(si, sj, 1, 1 - dir, h2);
                    }
                } else if prec1 == 1 {
                    self.set_parallel_edges(si, sj, 0, dir, h1);
                    self.set_parallel_edges(si, sj, 1, 1 - dir, h2);
                    if prec2 == -1 {
                        self.remove_edge(si, sj, 1 - dir);
                    }
                }
            }
        }
        true
    }

    /// Debug dump of the channel graphs (`GVDUMP`).
    fn dump_chans(&mut self, tag: &str) {
        if std::env::var_os("GVDUMP").is_none() {
            return;
        }
        for vert in [false, true] {
            self.for_each_chan(vert, &mut |o, k| {
                let chans = o.chans_for_kind(vert);
                let ch = chans.chan(k);
                if ch.seg_list.is_empty() {
                    return true;
                }
                let mut l = format!(
                    "OCG {tag} {:.6} [{:.6} {:.6}]:",
                    chans.items.get(k.0).v,
                    ch.p1,
                    ch.p2
                );
                for (i, v) in ch.g.vertices.iter().enumerate() {
                    l.push_str(&format!(" {i}->"));
                    for a in &v.adj_list {
                        l.push_str(&format!("{a},"));
                    }
                }
                eprintln!("{l}");
                true
            });
        }
    }

    /// `assignTracks(mp)`.
    fn assign_tracks(&mut self) -> bool {
        // create_graphs
        for vert in [false, true] {
            self.for_each_chan(vert, &mut |o, k| {
                let ch = o.chans_mut(vert).chan_mut(k);
                ch.g = RawGraph::new(ch.seg_list.len());
                true
            });
        }
        // add_np_edges
        for vert in [false, true] {
            let ok = self.for_each_chan(vert, &mut |o, k| {
                o.chans_for_kind(vert).chan(k).seg_list.is_empty() || o.add_edges_in_g(vert, k)
            });
            if !ok {
                return false;
            }
        }
        self.dump_chans("np");
        // add_p_edges
        for vert in [false, true] {
            if !self.for_each_chan(vert, &mut |o, k| o.add_p_edges_chan(vert, k)) {
                return false;
            }
        }
        self.dump_chans("x");
        // assignTrackNo
        for vert in [false, true] {
            self.for_each_chan(vert, &mut |o, k| {
                let ch = o.chans_mut(vert).chan_mut(k);
                if ch.seg_list.is_empty() {
                    return true;
                }
                ch.g.top_sort();
                let orders: Vec<(SegRef, i32)> = ch
                    .seg_list
                    .iter()
                    .enumerate()
                    .map(|(i, &r)| (r, ch.g.vertices[i].topsort_order + 1))
                    .collect();
                for (r, t) in orders {
                    o.routes[r.0][r.1].track_no = Some(t);
                }
                true
            });
        }
        true
    }

    /// `vtrack(seg, m)`.
    fn vtrack(&mut self, r: SegRef) -> f64 {
        let s = *self.seg(r);
        let k = self.vchans.search(&s);
        let chp = self.vchans.chan(k);
        let f = f64::from(s.track_no.unwrap()) / (chp.seg_list.len() as f64 + 1.0);
        let bb = self.mp.cells[chp.cp].bb;
        f.mul_add(bb.ur.x - bb.ll.x, bb.ll.x)
    }

    /// `htrack(seg, m)`.
    fn htrack(&mut self, r: SegRef) -> f64 {
        let s = *self.seg(r);
        let k = self.hchans.search(&s);
        let chp = self.hchans.chan(k);
        let f = 1.0 - f64::from(s.track_no.unwrap()) / (chp.seg_list.len() as f64 + 1.0);
        let bb = self.mp.cells[chp.cp].bb;
        let (lo, hi) = (bb.ll.y, bb.ur.y);
        f.mul_add(hi - lo, lo).round()
    }
}

/// `addLoop(sg, cp, dp, sp)`.
fn add_loop(sg: &mut SGraph, cp: &Cell, c: usize, dp: usize, sp: usize) {
    for onp in cp.sides.iter().flatten().copied() {
        if sg.nodes[onp].is_vert {
            continue;
        }
        let on_top = sg.nodes[onp].cells[0] == Some(c);
        if on_top {
            sg.create_sedge(sp, onp, 0.0);
        } else {
            sg.create_sedge(dp, onp, 0.0);
        }
    }
    sg.nnodes += 2;
}

/// `addNodeEdges(sg, cp, np)`.
fn add_node_edges(sg: &mut SGraph, cp: &Cell, c: usize, np: usize) {
    for onp in cp.sides.iter().flatten().copied() {
        sg.create_sedge(np, onp, 0.0);
    }
    sg.nnodes += 1;
    sg.nodes[np].cells = [Some(c), Some(c)];
}

/// `orthoEdges(g, useLbls)`: route every edge of `g` orthogonally. Returns
/// false where C returns non-zero (the maze could not be built).
pub fn ortho_edges<S: OrthoGraph + ?Sized>(g: &mut S, use_lbls: bool) -> bool {
    let nodes = g.ortho_nodes();
    ortho_edges_of(g, nodes, use_lbls)
}

/// `orthoEdges(sg, useLbls)` on the subgraph made of `nodes` (in `agfstnode`
/// order) and their out-edges, e.g. one packed component.
pub fn ortho_edges_of<S: OrthoGraph + ?Sized>(
    g: &mut S,
    nodes: Vec<usize>,
    use_lbls: bool,
) -> bool {
    // lib/ortho/ortho.c:orthoEdges: label routing is not implemented, so useLbls
    // only warns and is then cleared.
    if use_lbls {
        graphviz_pure_core::diag::agwarningf(
            "Orthogonal edges do not currently handle edge labels. Try using xlabels.\n",
        );
    }
    let node_cell: std::collections::HashMap<usize, usize> =
        nodes.iter().enumerate().map(|(i, &n)| (n, i)).collect();
    let boxes: Vec<(Point, f64, f64)> = nodes
        .iter()
        .map(|&n| (g.coord(n), g.lw(n) + g.rw(n), g.ht(n)))
        .collect();
    let Some(mp) = mk_maze(&boxes) else {
        return false;
    };
    let concentrate = g.ortho_concentrate();
    let mut ps: std::collections::HashSet<(usize, usize)> = Default::default();
    let mut es: Vec<(f64, usize)> = Vec::new();
    for &n in &nodes {
        for e in g.ortho_out_edges(n) {
            if concentrate {
                let ti = g.node_seq(g.tail(e));
                let hi = g.node_seq(g.head(e));
                let key = if ti <= hi { (ti, hi) } else { (hi, ti) };
                if !ps.insert(key) {
                    continue;
                }
            }
            let p = g.coord(g.tail(e));
            let q = g.coord(g.head(e));
            let d = (p.x - q.x).mul_add(p.x - q.x, (p.y - q.y) * (p.y - q.y));
            es.push((d, e));
        }
    }
    crate::cutil::qsort_by(&mut es, |a, b| {
        if a.0 > b.0 {
            1
        } else if a.0 < b.0 {
            -1
        } else {
            0
        }
    });
    let dump = std::env::var_os("GVDUMP").is_some();
    if dump {
        for &(d, e) in &es {
            eprintln!(
                "OE {} {} {:.6}",
                g.xl_name(g.tail(e)),
                g.xl_name(g.head(e)),
                d
            );
        }
    }
    let mut o = Ortho {
        mp,
        routes: Vec::with_capacity(es.len()),
        hchans: Chans::default(),
        vchans: Chans::default(),
    };
    let gstart = o.mp.sg.nnodes;
    let mut pq = Pq::new(o.mp.sg.nnodes + 2);
    let (sn, dn) = (gstart, gstart + 1);
    for &(_, e) in &es {
        let start = node_cell[&g.tail(e)];
        let dest = node_cell[&g.head(e)];
        if start == dest {
            let cell = o.mp.cells[start].clone();
            add_loop(&mut o.mp.sg, &cell, start, dn, sn);
        } else {
            let cd = o.mp.cells[dest].clone();
            add_node_edges(&mut o.mp.sg, &cd, dest, dn);
            let cs = o.mp.cells[start].clone();
            add_node_edges(&mut o.mp.sg, &cs, start, sn);
        }
        if !o.mp.sg.short_path(&mut pq, dn, sn) {
            return true;
        }
        if o.mp.sg.nodes[sn].n_dad.is_none() {
            // shortPath has never reached `sn`, so its N_DAD is still NULL
            // (`reset` does not clear it), and C's convertSPtoRoute
            // dereferences it and segfaults (#186). Leave the edges
            // unrouted, as C's `goto orthofinish` does when shortPath fails.
            // When an earlier edge did reach `sn`, C follows the stale
            // parent without crashing, and so does the port. This matches C
            // where the port's search graph does; see #203 for a case where
            // it does not.
            graphviz_pure_core::diag::agwarningf(
                "orthogonal routing found no path for an edge - edges not routed orthogonally\n",
            );
            return true;
        }
        if dump {
            let mut l = format!("OSPN {}:", o.routes.len());
            let mut x = Some(sn);
            while let Some(v) = x {
                l.push_str(&format!(
                    " {}/{}",
                    o.mp.sg.nodes[v].index, o.mp.sg.nodes[v].n_val
                ));
                x = o.mp.sg.nodes[v].n_dad;
            }
            eprintln!("{l}");
        }
        let r = convert_sp_to_route(&mut o.mp, sn, dn);
        if dump {
            let mut l = format!("OR {}:", o.routes.len());
            for sg in &r {
                l.push_str(&format!(
                    " [{} {:?} {:?} {:?} {} {}]",
                    i32::from(sg.is_vert),
                    sg.comm_coord,
                    sg.p1,
                    sg.p2,
                    sg.l1 as i32,
                    sg.l2 as i32
                ));
            }
            eprintln!("{l}");
        }
        o.routes.push(r);
        o.mp.sg.reset();
    }
    o.hchans = extract_chans(&mut o.mp, true);
    o.vchans = extract_chans(&mut o.mp, false);
    if !o.assign_segs() {
        // C asserts in chanSearch and aborts here (#170). Leave the edges
        // unrouted, as C's `goto orthofinish` does when shortPath or
        // assignTracks fails: the neato-family callers then draw them as
        // straight lines, and dot loses them.
        graphviz_pure_core::diag::agwarningf(
            "orthogonal routing found no channel for an edge segment - edges not routed orthogonally\n",
        );
        return true;
    }
    if !o.assign_tracks() {
        return true;
    }
    if dump {
        for (i, r) in o.routes.iter().enumerate() {
            let mut l = format!("OT {i}:");
            for sg in r {
                l.push_str(&format!(
                    " {}/{}",
                    sg.ind_no.unwrap_or(0),
                    sg.track_no.unwrap_or(0)
                ));
            }
            eprintln!("{l}");
        }
    }
    // attachOrthoEdges
    let sinfo = SplineInfo {
        ignore_swap: true,
        is_ortho: true,
    };
    for (irte, &(_, e)) in es.iter().enumerate() {
        let p1 = crate::common::geom::add(g.coord(g.tail(e)), g.tail_port(e).p);
        let q1 = crate::common::geom::add(g.coord(g.head(e)), g.head_port(e).p);
        let n = o.routes[irte].len();
        if n == 0 {
            continue;
        }
        let mut ispline = Vec::with_capacity(1 + 3 * n);
        let s0 = (irte, 0);
        let mut p = if o.seg(s0).is_vert {
            Point::new(o.vtrack(s0), p1.y)
        } else {
            Point::new(p1.x, o.htrack(s0))
        };
        ispline.push(p);
        ispline.push(p);
        let mut last = s0;
        for i in 1..n {
            let s = (irte, i);
            if o.seg(s).is_vert {
                p.x = o.vtrack(s);
            } else {
                p.y = o.htrack(s);
            }
            ispline.push(p);
            ispline.push(p);
            ispline.push(p);
            last = s;
        }
        p = if o.seg(last).is_vert {
            Point::new(o.vtrack(last), q1.y)
        } else {
            Point::new(q1.x, o.htrack(last))
        };
        ispline.push(p);
        ispline.push(p);
        let h = g.head(e);
        clip_and_install_info(g, e, h, &ispline, sinfo);
    }
    true
}
