//! Port of `lib/ortho/maze.c` and `maze.h`: the maze of free cells around
//! the node cells and its search graph.

use super::partition::partition;
use super::sgraph::SGraph;
use super::trapezoid::dfp_cmp;
use crate::cdt::Cdt;
use graphviz_pure_core::{BoundingBox, Point};
use std::cmp::Ordering;

pub const M_RIGHT: usize = 0;
pub const M_TOP: usize = 1;
pub const M_LEFT: usize = 2;
pub const M_BOTTOM: usize = 3;

pub const MZ_ISNODE: i32 = 1;
pub const MZ_VSCAN: i32 = 2;
pub const MZ_HSCAN: i32 = 4;
pub const MZ_SMALLV: i32 = 8;
pub const MZ_SMALLH: i32 = 16;

const MARGIN: f64 = 36.0;
const DELTA: f64 = 1.0;
const MU: f64 = 500.0;
pub const BIG: f64 = 16384.0;

fn chansz(w: f64) -> f64 {
    (w - 3.0) / 2.0
}

fn is_small(v: f64) -> bool {
    chansz(v) < 2.0
}

/// `cell`. The `sides` of ordinary cells are indexed by `M_RIGHT`..`M_BOTTOM`;
/// node cells list all their side nodes.
#[derive(Debug, Clone, Default)]
pub struct Cell {
    pub flags: i32,
    pub edges: Vec<usize>,
    pub sides: Vec<Option<usize>>,
    pub bb: BoundingBox,
}

impl Cell {
    pub fn is_node(&self) -> bool {
        self.flags & MZ_ISNODE != 0
    }
}

/// `maze`: `cells[0..ngcells]` are the node cells (`gcells`), the rest the
/// free cells.
#[derive(Debug, Clone, Default)]
pub struct Maze {
    pub ngcells: usize,
    pub cells: Vec<Cell>,
    pub sg: SGraph,
}

impl Maze {
    pub fn ncells(&self) -> usize {
        self.cells.len() - self.ngcells
    }
}

fn vcmpid(a: &Point, b: &Point) -> i32 {
    let dx = dfp_cmp(a.x, b.x);
    if dx != 0 {
        return dx;
    }
    dfp_cmp(a.y, b.y)
}

fn hcmpid(a: &Point, b: &Point) -> i32 {
    let dy = dfp_cmp(a.y, b.y);
    if dy != 0 {
        return dy;
    }
    dfp_cmp(a.x, b.x)
}

/// `vdict`/`hdict`: a `Dtoset` of `(point, snode)` items under `vcmpid` or
/// `hcmpid`. The comparators are fuzzy (`dfp_cmp`), so lookups must follow
/// cdt's splay tree to find the same items as C.
struct PointDict {
    dt: Cdt<(Point, usize)>,
    cmp: fn(&Point, &Point) -> i32,
}

fn ord(c: i32) -> Ordering {
    c.cmp(&0)
}

impl PointDict {
    fn new(cmp: fn(&Point, &Point) -> i32) -> Self {
        PointDict {
            dt: Cdt::new(),
            cmp,
        }
    }

    /// `dtmatch(dict, &p)`.
    fn find(&mut self, p: &Point) -> Option<usize> {
        let cmp = self.cmp;
        self.dt.find(&|o: &(Point, usize)| ord(cmp(p, &o.0)))
    }

    /// `dtnext(dict, item)`.
    fn next(&mut self, k: usize) -> Option<usize> {
        let cmp = self.cmp;
        let key = self.dt.get(k).0;
        self.dt.next(k, &|o: &(Point, usize)| ord(cmp(&key, &o.0)))
    }

    fn item(&self, k: usize) -> (Point, usize) {
        *self.dt.get(k)
    }

    /// `dtinsert(dict, item)`.
    fn insert(&mut self, p: Point, np: usize) {
        let cmp = self.cmp;
        self.dt
            .insert((p, np), &|o: &(Point, usize)| ord(cmp(&p, &o.0)));
    }
}

/// `updateWt(ep, sz)`.
fn update_wt(sg: &mut SGraph, e: usize, sz: f64) {
    let ep = &mut sg.edges[e];
    ep.cnt += 1;
    if f64::from(ep.cnt) > sz {
        ep.cnt = 0;
        ep.weight += BIG;
    }
}

fn bend(sg: &SGraph, e: usize) -> bool {
    let ed = &sg.edges[e];
    sg.nodes[ed.v1].is_vert != sg.nodes[ed.v2].is_vert
}

fn horz(sg: &SGraph, e: usize) -> bool {
    sg.nodes[sg.edges[e].v1].is_vert
}

/// `updateWts(g, cp, ep)`.
pub fn update_wts(sg: &mut SGraph, cp: &Cell, ep: usize) {
    let is_bend = bend(sg, ep);
    let hsz = chansz(cp.bb.ur.y - cp.bb.ll.y);
    let vsz = chansz(cp.bb.ur.x - cp.bb.ll.x);
    let minsz = hsz.min(vsz);
    let mut i = 0;
    while i < cp.edges.len() {
        let e = cp.edges[i];
        if !bend(sg, e) {
            break;
        }
        update_wt(sg, e, minsz);
        i += 1;
    }
    while i < cp.edges.len() {
        let e = cp.edges[i];
        if is_bend || e == ep {
            let sz = if horz(sg, e) { hsz } else { vsz };
            update_wt(sg, e, sz);
        }
        i += 1;
    }
}

/// `markSmall(cp)`.
fn mark_small(mp: &mut Maze, c: usize) {
    let bb = mp.cells[c].bb;
    let sides: Vec<usize> = mp.cells[c].sides.iter().flatten().copied().collect();
    if is_small(bb.ur.y - bb.ll.y) {
        for &onp0 in &sides {
            if !mp.sg.nodes[onp0].is_vert {
                continue;
            }
            if mp.sg.nodes[onp0].cells[0] == Some(c) {
                let mut ocp = mp.sg.nodes[onp0].cells[1].unwrap();
                mp.cells[ocp].flags |= MZ_SMALLV;
                while let Some(onp) = mp.cells[ocp].sides[M_RIGHT] {
                    let nc = mp.sg.nodes[onp].cells[1].unwrap();
                    if mp.cells[nc].is_node() {
                        break;
                    }
                    ocp = nc;
                    mp.cells[ocp].flags |= MZ_SMALLV;
                }
            } else {
                let mut ocp = mp.sg.nodes[onp0].cells[0].unwrap();
                mp.cells[ocp].flags |= MZ_SMALLV;
                while let Some(onp) = mp.cells[ocp].sides[M_LEFT] {
                    let nc = mp.sg.nodes[onp].cells[0].unwrap();
                    if mp.cells[nc].is_node() {
                        break;
                    }
                    ocp = nc;
                    mp.cells[ocp].flags |= MZ_SMALLV;
                }
            }
        }
    }
    if is_small(bb.ur.x - bb.ll.x) {
        for &onp0 in &sides {
            if mp.sg.nodes[onp0].is_vert {
                continue;
            }
            if mp.sg.nodes[onp0].cells[0] == Some(c) {
                let mut ocp = mp.sg.nodes[onp0].cells[1].unwrap();
                mp.cells[ocp].flags |= MZ_SMALLH;
                while let Some(onp) = mp.cells[ocp].sides[M_TOP] {
                    let nc = mp.sg.nodes[onp].cells[1].unwrap();
                    if mp.cells[nc].is_node() {
                        break;
                    }
                    ocp = nc;
                    mp.cells[ocp].flags |= MZ_SMALLH;
                }
            } else {
                let mut ocp = mp.sg.nodes[onp0].cells[0].unwrap();
                mp.cells[ocp].flags |= MZ_SMALLH;
                while let Some(onp) = mp.cells[ocp].sides[M_BOTTOM] {
                    let nc = mp.sg.nodes[onp].cells[0].unwrap();
                    if mp.cells[nc].is_node() {
                        break;
                    }
                    ocp = nc;
                    mp.cells[ocp].flags |= MZ_SMALLH;
                }
            }
        }
    }
}

/// `createSEdges(cp, g)`.
fn create_sedges(mp: &mut Maze, c: usize) {
    let cp = &mp.cells[c];
    let bb = cp.bb;
    let mut hwt = DELTA * (bb.ur.x - bb.ll.x);
    let mut vwt = DELTA * (bb.ur.y - bb.ll.y);
    let mut wt = (hwt + vwt) / 2.0 + MU;
    if is_small(bb.ur.y - bb.ll.y) && cp.flags & MZ_SMALLV == 0 {
        hwt = BIG;
        wt = BIG;
    }
    if is_small(bb.ur.x - bb.ll.x) && cp.flags & MZ_SMALLH == 0 {
        vwt = BIG;
        wt = BIG;
    }
    let s = [
        cp.sides[M_RIGHT],
        cp.sides[M_TOP],
        cp.sides[M_LEFT],
        cp.sides[M_BOTTOM],
    ];
    let pairs = [
        (M_LEFT, M_TOP, wt),
        (M_TOP, M_RIGHT, wt),
        (M_LEFT, M_BOTTOM, wt),
        (M_BOTTOM, M_RIGHT, wt),
        (M_TOP, M_BOTTOM, vwt),
        (M_LEFT, M_RIGHT, hwt),
    ];
    for (a, b, w) in pairs {
        if let (Some(na), Some(nb)) = (s[a], s[b]) {
            let e = mp.sg.create_sedge(na, nb, w);
            mp.cells[c].edges.push(e);
        }
    }
}

/// `findSVert(g, cdt, p, ditems, isVert)`.
fn find_svert(sg: &mut SGraph, dict: &mut PointDict, p: Point, is_vert: bool) -> usize {
    if let Some(i) = dict.find(&p) {
        return dict.item(i).1;
    }
    let np = sg.create_snode();
    sg.nodes[np].is_vert = is_vert;
    dict.insert(p, np);
    np
}

/// `chkSgraph(g)`: every search-graph node lies between two cells.
fn chk_sgraph(sg: &SGraph) -> bool {
    sg.nodes[..sg.nnodes]
        .iter()
        .all(|np| np.cells[0].is_some() && np.cells[1].is_some())
}

/// `mkMazeGraph(mp, bb)`. False where C's `chkSgraph` fails an assertion
/// (`lib/ortho/maze.c:308-320`, e.g. on a non-finite node box, #171): C
/// aborts there, the port fails the maze so the caller falls back.
fn mk_maze_graph(mp: &mut Maze, bb: BoundingBox) -> bool {
    let ncells = mp.ncells();
    let bound = 4 * ncells;
    mp.sg = SGraph::new(bound + 2);
    let mut vdict = PointDict::new(vcmpid);
    let mut hdict = PointDict::new(hcmpid);
    for i in mp.ngcells..mp.cells.len() {
        mp.cells[i].sides = vec![None; 4];
        let cb = mp.cells[i].bb;
        if cb.ur.x < bb.ur.x {
            let np = find_svert(&mut mp.sg, &mut vdict, Point::new(cb.ur.x, cb.ll.y), true);
            mp.sg.nodes[np].cells[0] = Some(i);
            mp.cells[i].sides[M_RIGHT] = Some(np);
        }
        if cb.ur.y < bb.ur.y {
            let np = find_svert(&mut mp.sg, &mut hdict, Point::new(cb.ll.x, cb.ur.y), false);
            mp.sg.nodes[np].cells[0] = Some(i);
            mp.cells[i].sides[M_TOP] = Some(np);
        }
        if cb.ll.x > bb.ll.x {
            let np = find_svert(&mut mp.sg, &mut vdict, cb.ll, true);
            mp.sg.nodes[np].cells[1] = Some(i);
            mp.cells[i].sides[M_LEFT] = Some(np);
        }
        if cb.ll.y > bb.ll.y {
            let np = find_svert(&mut mp.sg, &mut hdict, cb.ll, false);
            mp.sg.nodes[np].cells[1] = Some(i);
            mp.cells[i].sides[M_BOTTOM] = Some(np);
        }
    }
    let mut maxdeg = 0;
    for i in 0..mp.ngcells {
        let cb = mp.cells[i].bb;
        let mut sides = Vec::new();
        let mut walk =
            |dict: &mut PointDict, start: Point, along_x: bool, ci: usize, sg: &mut SGraph| {
                let mut k = dict.find(&start);
                while let Some(kk) = k {
                    let (p, np) = dict.item(kk);
                    let lim = if along_x {
                        p.x < cb.ur.x
                    } else {
                        p.y < cb.ur.y
                    };
                    if !lim {
                        break;
                    }
                    sides.push(Some(np));
                    sg.nodes[np].cells[ci] = Some(i);
                    k = dict.next(kk);
                }
            };
        walk(&mut hdict, cb.ll, true, 1, &mut mp.sg);
        walk(&mut vdict, cb.ll, false, 1, &mut mp.sg);
        walk(
            &mut hdict,
            Point::new(cb.ll.x, cb.ur.y),
            true,
            0,
            &mut mp.sg,
        );
        walk(
            &mut vdict,
            Point::new(cb.ur.x, cb.ll.y),
            false,
            0,
            &mut mp.sg,
        );
        if sides.len() > maxdeg {
            maxdeg = sides.len();
        }
        mp.cells[i].sides = sides;
    }
    for i in 0..mp.ngcells {
        mark_small(mp, i);
    }
    let nn = mp.sg.nnodes;
    mp.sg.nodes[nn].index = nn;
    mp.sg.nodes[nn + 1].index = nn + 1;
    mp.sg.init_sedges(maxdeg);
    for i in mp.ngcells..mp.cells.len() {
        create_sedges(mp, i);
    }
    if !chk_sgraph(&mp.sg) {
        return false;
    }
    mp.sg.gsave();
    true
}

/// `mkMaze(g)`: one node cell per node box (`(x, y, xsize, ysize)`), then the
/// free cells from `partition`. `None` when partitioning fails or the search
/// graph is broken (`chkSgraph`).
pub fn mk_maze(nodes: &[(Point, f64, f64)]) -> Option<Maze> {
    let mut mp = Maze {
        ngcells: nodes.len(),
        ..Maze::default()
    };
    let mut big = BoundingBox::new(f64::MAX, f64::MAX, -f64::MAX, -f64::MAX);
    for &(c, xsize, ysize) in nodes {
        let w2 = 1f64.max(xsize / 2.0);
        let h2 = 1f64.max(ysize / 2.0);
        let bb = BoundingBox::new(c.x - w2, c.y - h2, c.x + w2, c.y + h2);
        big.ll.x = big.ll.x.min(bb.ll.x);
        big.ll.y = big.ll.y.min(bb.ll.y);
        big.ur.x = big.ur.x.max(bb.ur.x);
        big.ur.y = big.ur.y.max(bb.ur.y);
        mp.cells.push(Cell {
            flags: MZ_ISNODE,
            bb,
            ..Cell::default()
        });
    }
    big.ll.x -= MARGIN;
    big.ll.y -= MARGIN;
    big.ur.x += MARGIN;
    big.ur.y += MARGIN;
    let gbbs: Vec<BoundingBox> = mp.cells.iter().map(|c| c.bb).collect();
    let rects = partition(&gbbs, big)?;
    for r in rects {
        mp.cells.push(Cell {
            bb: r,
            ..Cell::default()
        });
    }
    let dump = std::env::var_os("GVDUMP").is_some();
    if dump {
        let ng = mp.ngcells;
        for (i, c) in mp.cells.iter().enumerate() {
            let (tag, k) = if i < ng { ("OMG", i) } else { ("OMC", i - ng) };
            eprintln!(
                "{tag} {k} {:?} {:?} {:?} {:?}",
                c.bb.ll.x, c.bb.ll.y, c.bb.ur.x, c.bb.ur.y
            );
        }
    }
    if !mk_maze_graph(&mut mp, big) {
        return None;
    }
    if dump {
        for i in 0..mp.ngcells {
            let s: String = mp.cells[i]
                .sides
                .iter()
                .map(|n| format!(" {}", n.map_or(-1, |n| mp.sg.nodes[n].index as i64)))
                .collect();
            eprintln!("OMS {i}:{s}");
        }
    }
    Some(mp)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue #171: osage places a node of a repeated cluster at `x = inf`, as C
    /// does. `partition`'s monotonation then needs a fifth chain slot at a vertex
    /// (C overruns `vpos[4]`), and the search graph has a node with one cell, so
    /// C fails `assert(np->cells[1])` in `lib/ortho/maze.c:chkSgraph`. The port
    /// fails the maze instead.
    #[test]
    fn non_finite_node_box_fails_the_maze() {
        let nodes = [
            (Point::new(58.0, 18.0), 54.0, 36.0),
            (
                Point::new(f64::INFINITY, 112.0),
                77.864_504_732_387_15,
                36.0,
            ),
            (
                Point::new(181.447_833_640_676_58, 112.0),
                122.895_667_281_353_17,
                36.0,
            ),
            (Point::new(181.0, 18.0), 54.0, 36.0),
            (Point::new(29.0, 100.0), 54.0, 36.0),
            (Point::new(87.0, 100.0), 54.0, 36.0),
            (
                Point::new(292.896_757_820_101_7, 112.0),
                91.793_515_640_203_44,
                36.0,
            ),
            (Point::new(58.0, 58.0), 54.0, 36.0),
            (Point::new(181.0, 58.0), 54.0, 36.0),
            (Point::new(293.0, 58.0), 54.0, 36.0),
        ];
        assert!(mk_maze(&nodes).is_none());
    }

    /// Issue #203: the tiny touching point nodes of a dot `splines=ortho` graph.
    /// `partition` keeps no free rectangle, so C's `LIST_DETACH` hands mkMaze a
    /// NULL list and `orthoEdges` fails. The port went on with an empty maze and
    /// panicked in `convert_sp_to_route`.
    #[test]
    fn partition_without_free_cells_fails_the_maze() {
        let centres = [
            (5.0, 57.36),
            (7.0, 47.92),
            (6.0, 38.48),
            (2.0, 29.04),
            (11.0, 29.04),
            (11.0, 19.6),
            (5.0, 10.16),
            (0.0, 0.72),
            (23.0, 10.16),
            (-1.0, 19.6),
            (-1.0, 10.16),
            (-2.0, 0.72),
        ];
        let nodes: Vec<_> = centres
            .iter()
            .map(|&(x, y)| (Point::new(x, y), 1.44, 1.44))
            .collect();
        assert!(mk_maze(&nodes).is_none());
    }
}
