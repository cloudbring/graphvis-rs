//! Port of `lib/ortho/maze.c` and `maze.h`: the maze of free cells around
//! the node cells and its search graph.

use super::fail::{At, Fail, Need, OResult};
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

    pub fn cell(&self, c: usize) -> OResult<&Cell> {
        self.cells.at(c)
    }

    pub fn cell_mut(&mut self, c: usize) -> OResult<&mut Cell> {
        self.cells.at_mut(c)
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
fn update_wt(sg: &mut SGraph, e: usize, sz: f64) -> OResult<()> {
    let ep = sg.edge_mut(e)?;
    ep.cnt += 1;
    if f64::from(ep.cnt) > sz {
        ep.cnt = 0;
        ep.weight += BIG;
    }
    Ok(())
}

fn bend(sg: &SGraph, e: usize) -> OResult<bool> {
    let ed = sg.edge(e)?;
    Ok(sg.node(ed.v1)?.is_vert != sg.node(ed.v2)?.is_vert)
}

fn horz(sg: &SGraph, e: usize) -> OResult<bool> {
    Ok(sg.node(sg.edge(e)?.v1)?.is_vert)
}

/// `updateWts(g, cp, ep)`.
pub fn update_wts(sg: &mut SGraph, cp: &Cell, ep: usize) -> OResult<()> {
    let is_bend = bend(sg, ep)?;
    let hsz = chansz(cp.bb.ur.y - cp.bb.ll.y);
    let vsz = chansz(cp.bb.ur.x - cp.bb.ll.x);
    let minsz = hsz.min(vsz);
    let mut i = 0;
    while i < cp.edges.len() {
        let e = cp.edges[i];
        if !bend(sg, e)? {
            break;
        }
        update_wt(sg, e, minsz)?;
        i += 1;
    }
    while i < cp.edges.len() {
        let e = cp.edges[i];
        if is_bend || e == ep {
            let sz = if horz(sg, e)? { hsz } else { vsz };
            update_wt(sg, e, sz)?;
        }
        i += 1;
    }
    Ok(())
}

/// `markSmall(cp)`. Fails where C dereferences a missing cell or reads past
/// the allocation of a node cell's side list (`lib/ortho/maze.c:markSmall`).
/// Tiny or touching node boxes in dot get there (#202): C segfaults, so the
/// port fails the maze instead and the caller falls back.
fn mark_small(mp: &mut Maze, c: usize) -> OResult<()> {
    let bb = mp.cell(c)?.bb;
    let sides: Vec<usize> = mp.cell(c)?.sides.iter().flatten().copied().collect();
    if is_small(bb.ur.y - bb.ll.y) {
        for &onp0 in &sides {
            let onp = mp.sg.node(onp0)?;
            if !onp.is_vert {
                continue;
            }
            if onp.cells[0] == Some(c) {
                mark_run(mp, onp0, 1, M_RIGHT, MZ_SMALLV)?;
            } else {
                mark_run(mp, onp0, 0, M_LEFT, MZ_SMALLV)?;
            }
        }
    }
    if is_small(bb.ur.x - bb.ll.x) {
        for &onp0 in &sides {
            let onp = mp.sg.node(onp0)?;
            if onp.is_vert {
                continue;
            }
            if onp.cells[0] == Some(c) {
                mark_run(mp, onp0, 1, M_TOP, MZ_SMALLH)?;
            } else {
                mark_run(mp, onp0, 0, M_BOTTOM, MZ_SMALLH)?;
            }
        }
    }
    Ok(())
}

/// One walk of `markSmall`: flag the cell `onp->cells[ci]`, then keep stepping
/// through `side` while the next cell is not a node. Fails on a missing cell
/// or an unallocated side slot, where C crashes. The walk moves one way across
/// the maze, so it cannot pass more cells than there are.
fn mark_run(mp: &mut Maze, onp: usize, ci: usize, side: usize, flag: i32) -> OResult<()> {
    let mut ocp = mp.sg.node(onp)?.cells[ci].need()?;
    mp.cell_mut(ocp)?.flags |= flag;
    let mut budget = mp.cells.len();
    while let Some(onp) = side_slot(&mp.cell(ocp)?.sides, side).need()? {
        budget = budget.checked_sub(1).need()?;
        let nc = mp.sg.node(onp)?.cells[ci].need()?;
        if mp.cell(nc)?.is_node() {
            break;
        }
        ocp = nc;
        mp.cell_mut(ocp)?.flags |= flag;
    }
    Ok(())
}

/// `cp->sides[side]` as C reads it. A node cell's side list comes from
/// `LIST_DETACH` (`lib/util/list.c`), which keeps the capacity: 1, 2, 4, ...
/// slots, the unused ones zeroed. So a read past the list but within the
/// capacity is NULL and ends C's walk; past the capacity (or on an empty list,
/// whose buffer is NULL) C reads unallocated memory, and this returns `None`.
/// This holds because `mkMazeGraph` fills the list only with `LIST_APPEND`
/// (head 0, capacity `len.next_power_of_two()`).
fn side_slot(sides: &[Option<usize>], side: usize) -> Option<Option<usize>> {
    match sides.get(side) {
        Some(&s) => Some(s),
        None if !sides.is_empty() && side < sides.len().next_power_of_two() => Some(None),
        None => None,
    }
}

/// `createSEdges(cp, g)`.
fn create_sedges(mp: &mut Maze, c: usize) -> OResult<()> {
    let cp = mp.cell(c)?;
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
        *cp.sides.at(M_RIGHT)?,
        *cp.sides.at(M_TOP)?,
        *cp.sides.at(M_LEFT)?,
        *cp.sides.at(M_BOTTOM)?,
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
            let e = mp.sg.create_sedge(na, nb, w)?;
            mp.cells[c].edges.push(e);
        }
    }
    Ok(())
}

/// `findSVert(g, cdt, p, ditems, isVert)`.
fn find_svert(sg: &mut SGraph, dict: &mut PointDict, p: Point, is_vert: bool) -> OResult<usize> {
    if let Some(i) = dict.find(&p) {
        return Ok(dict.item(i).1);
    }
    let np = sg.create_snode()?;
    sg.nodes[np].is_vert = is_vert;
    dict.insert(p, np);
    Ok(np)
}

/// `chkSgraph(g)`: every search-graph node lies between two cells. C
/// asserts it (`lib/ortho/maze.c:308-320`); a non-finite node box breaks it
/// (#171).
fn chk_sgraph(sg: &SGraph) -> OResult<()> {
    for (node, np) in sg.nodes.get(..sg.nnodes).need()?.iter().enumerate() {
        if let Some(side) = np.cells.iter().position(Option::is_none) {
            return Err(Fail::NoCell { node, side });
        }
    }
    Ok(())
}

/// `mkMazeGraph(mp, bb)`.
fn mk_maze_graph(mp: &mut Maze, bb: BoundingBox) -> OResult<()> {
    let ncells = mp.ncells();
    let bound = 4 * ncells;
    mp.sg = SGraph::new(bound + 2);
    let mut vdict = PointDict::new(vcmpid);
    let mut hdict = PointDict::new(hcmpid);
    for i in mp.ngcells..mp.cells.len() {
        mp.cells[i].sides = vec![None; 4];
        let cb = mp.cells[i].bb;
        if cb.ur.x < bb.ur.x {
            let np = find_svert(&mut mp.sg, &mut vdict, Point::new(cb.ur.x, cb.ll.y), true)?;
            mp.sg.nodes[np].cells[0] = Some(i);
            mp.cells[i].sides[M_RIGHT] = Some(np);
        }
        if cb.ur.y < bb.ur.y {
            let np = find_svert(&mut mp.sg, &mut hdict, Point::new(cb.ll.x, cb.ur.y), false)?;
            mp.sg.nodes[np].cells[0] = Some(i);
            mp.cells[i].sides[M_TOP] = Some(np);
        }
        if cb.ll.x > bb.ll.x {
            let np = find_svert(&mut mp.sg, &mut vdict, cb.ll, true)?;
            mp.sg.nodes[np].cells[1] = Some(i);
            mp.cells[i].sides[M_LEFT] = Some(np);
        }
        if cb.ll.y > bb.ll.y {
            let np = find_svert(&mut mp.sg, &mut hdict, cb.ll, false)?;
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
                    sg.node_mut(np)?.cells[ci] = Some(i);
                    k = dict.next(kk);
                }
                Ok::<(), Fail>(())
            };
        walk(&mut hdict, cb.ll, true, 1, &mut mp.sg)?;
        walk(&mut vdict, cb.ll, false, 1, &mut mp.sg)?;
        walk(
            &mut hdict,
            Point::new(cb.ll.x, cb.ur.y),
            true,
            0,
            &mut mp.sg,
        )?;
        walk(
            &mut vdict,
            Point::new(cb.ur.x, cb.ll.y),
            false,
            0,
            &mut mp.sg,
        )?;
        if sides.len() > maxdeg {
            maxdeg = sides.len();
        }
        mp.cells[i].sides = sides;
    }
    for i in 0..mp.ngcells {
        mark_small(mp, i)?;
    }
    let nn = mp.sg.nnodes;
    mp.sg.node_mut(nn)?.index = nn;
    mp.sg.node_mut(nn + 1)?.index = nn + 1;
    mp.sg.init_sedges(maxdeg)?;
    for i in mp.ngcells..mp.cells.len() {
        create_sedges(mp, i)?;
    }
    chk_sgraph(&mp.sg)?;
    mp.sg.gsave()
}

/// `mkMaze(g)`: one node cell per node box (`(x, y, xsize, ysize)`), then the
/// free cells from `partition`. Any failure here is a failure of C's
/// `mkMaze`, which returns NULL where it detects one.
pub fn mk_maze(nodes: &[(Point, f64, f64)]) -> OResult<Maze> {
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
            graphviz_pure_core::diag::dump(&format!(
                "{tag} {k} {:?} {:?} {:?} {:?}",
                c.bb.ll.x, c.bb.ll.y, c.bb.ur.x, c.bb.ur.y
            ));
        }
    }
    mk_maze_graph(&mut mp, big)?;
    if dump {
        for i in 0..mp.ngcells {
            let s: String = mp.cells[i]
                .sides
                .iter()
                .map(|n| format!(" {}", n.map_or(-1, |n| mp.sg.nodes[n].index as i64)))
                .collect();
            graphviz_pure_core::diag::dump(&format!("OMS {i}:{s}"));
        }
    }
    Ok(mp)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue #202: dot places tiny nodes so that `n2` and `n1` touch. Every box
    /// is clamped to 2x2, and `markSmall`'s walk reaches a search node with a
    /// missing cell (`lib/ortho/maze.c:markSmall`), where C segfaults.
    /// The port panicked on an `unwrap`; now the maze fails.
    #[test]
    fn touching_tiny_node_boxes_fail_the_maze() {
        crate::cutil::in_fresh_run(|| {
            let nodes = [
                (Point::new(0.72, 10.16), 0.72, 1.44),
                (Point::new(10.72, 0.72), 0.72, 1.44),
                (Point::new(22.72, 10.16), 0.72, 1.44),
                (Point::new(20.72, 10.16), 0.72, 1.44),
            ];
            assert!(mk_maze(&nodes).is_err());
        });
    }

    /// Issue #202 review: walking down from a small node lands on a node cell
    /// with three sides. C's side list has a fourth, zeroed slot (`LIST_DETACH`
    /// keeps the capacity), so `sides[M_BOTTOM]` is NULL and `markSmall` ends
    /// the walk; C builds the maze and routes the edges. The port must too.
    #[test]
    fn three_sided_node_cell_ends_the_mark_small_walk() {
        crate::cutil::in_fresh_run(|| {
            let nodes = [
                (Point::new(3.0, 5.5), 4.0, 4.0),
                (Point::new(9.5, 10.0), 1.0, 0.5),
                (Point::new(1.0, 9.0), 6.0, 1.0),
                (Point::new(10.5, 10.0), 0.5, 0.5),
            ];
            assert!(mk_maze(&nodes).is_ok());
        });
    }

    #[test]
    fn side_slot_reads_like_cs_detached_list() {
        let three = [Some(1), Some(2), Some(3)];
        assert_eq!(side_slot(&three, M_TOP), Some(Some(2)));
        assert_eq!(side_slot(&three, M_BOTTOM), Some(None));
        assert_eq!(side_slot(&three[..1], M_TOP), None);
        assert_eq!(side_slot(&[], M_RIGHT), None);
    }

    /// Issue #171: osage places a node of a repeated cluster at `x = inf`, as C
    /// does. `partition`'s monotonation then needs a fifth chain slot at a vertex
    /// (C overruns `vpos[4]`), and the search graph has a node with one cell, so
    /// C fails `assert(np->cells[1])` in `lib/ortho/maze.c:chkSgraph`. The port
    /// fails the maze instead.
    #[test]
    fn non_finite_node_box_fails_the_maze() {
        crate::cutil::in_fresh_run(|| {
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
            let fail = mk_maze(&nodes).unwrap_err();
            assert_eq!(fail, Fail::NoCell { node: 1, side: 1 });
            // What C prints before its assertion fails (#127).
            assert_eq!(fail.c_message().as_deref(), Some("failed at node 1[1]\n"));
        });
    }

    /// Issue #203: the tiny touching point nodes of a dot `splines=ortho` graph.
    /// `partition` keeps no free rectangle, so C's `LIST_DETACH` hands mkMaze a
    /// NULL list and `orthoEdges` fails. The port went on with an empty maze and
    /// panicked in `convert_sp_to_route`.
    #[test]
    fn partition_without_free_cells_fails_the_maze() {
        crate::cutil::in_fresh_run(|| {
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
            assert!(mk_maze(&nodes).is_err());
        });
    }
}
