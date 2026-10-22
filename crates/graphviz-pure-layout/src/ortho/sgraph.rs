//! Port of `lib/ortho/sgraph.c` and `lib/ortho/fPQ.c`: the search graph of
//! the maze and Dijkstra's shortest path over it.

use super::fail::{At, Fail, Need, OResult, ensure};

/// `snode`.
#[derive(Debug, Clone, Default)]
pub struct SNode {
    pub n_val: i32,
    pub n_idx: i32,
    pub n_dad: Option<usize>,
    pub n_edge: Option<usize>,
    pub save_n_adj: usize,
    /// `cells[0]`: left or bottom, `cells[1]`: top or right adjacent cell.
    pub cells: [Option<usize>; 2],
    /// `n_adj`: entries of `adj_edge_list` in use.
    pub n_adj: usize,
    /// `adj_edge_list`: start of this node's slots in [`SGraph::adj`].
    pub adj_base: usize,
    pub index: usize,
    pub is_vert: bool,
}

/// `sedge`.
#[derive(Debug, Clone, Default)]
pub struct SEdge {
    pub weight: f64,
    pub cnt: i32,
    pub v1: usize,
    pub v2: usize,
}

/// `sgraph`.
#[derive(Debug, Clone, Default)]
pub struct SGraph {
    pub nnodes: usize,
    pub save_nnodes: usize,
    pub save_nedges: usize,
    /// Allocated nodes (`nnodes` in use plus the two dummies).
    pub nodes: Vec<SNode>,
    /// `edges[0..nedges]`; slots past `nedges` keep stale edges, as in C.
    pub edges: Vec<SEdge>,
    pub nedges: usize,
    /// The adjacency storage of `initSEdges`: 6 slots per node and `maxdeg`
    /// for each dummy node, back to back. A node with more edges than slots
    /// writes into its neighbour's slots, as C does (dot's `addNodeEdges`
    /// can give a side node a 7th edge); the search reads them back.
    pub adj: Vec<usize>,
}

impl SGraph {
    /// `createSGraph(nnodes)`.
    pub fn new(nnodes: usize) -> Self {
        SGraph {
            nodes: vec![SNode::default(); nnodes],
            ..SGraph::default()
        }
    }

    pub fn node(&self, n: usize) -> OResult<&SNode> {
        self.nodes.at(n)
    }

    pub fn node_mut(&mut self, n: usize) -> OResult<&mut SNode> {
        self.nodes.at_mut(n)
    }

    pub fn edge(&self, e: usize) -> OResult<&SEdge> {
        self.edges.at(e)
    }

    pub fn edge_mut(&mut self, e: usize) -> OResult<&mut SEdge> {
        self.edges.at_mut(e)
    }

    /// `gsave(G)`.
    pub fn gsave(&mut self) -> OResult<()> {
        self.save_nnodes = self.nnodes;
        self.save_nedges = self.nedges;
        for np in self.nodes.get_mut(..self.nnodes).need()? {
            np.save_n_adj = np.n_adj;
        }
        Ok(())
    }

    /// `reset(G)`.
    pub fn reset(&mut self) -> OResult<()> {
        self.nnodes = self.save_nnodes;
        self.nedges = self.save_nedges;
        let nn = self.nnodes;
        for np in self.nodes.get_mut(..nn).need()? {
            np.n_adj = np.save_n_adj;
        }
        for np in self.nodes.get_mut(nn..nn + 2).need()? {
            np.n_adj = 0;
        }
        Ok(())
    }

    /// `initSEdges(g, maxdeg)`.
    pub fn init_sedges(&mut self, maxdeg: usize) -> OResult<()> {
        self.adj = vec![0; 6 * self.nnodes + 2 * maxdeg];
        let mut base = 0;
        let nn = self.nnodes;
        for (i, np) in self.nodes.get_mut(..nn + 2).need()?.iter_mut().enumerate() {
            np.adj_base = base;
            base += if i < nn { 6 } else { maxdeg };
        }
        Ok(())
    }

    /// `adj_edge_list[k]` of node `n`.
    pub fn adj_edge(&self, n: usize, k: usize) -> OResult<usize> {
        Ok(self
            .adj
            .get(self.node(n)?.adj_base + k)
            .copied()
            .unwrap_or(0))
    }

    /// `addEdgeToNode(np, idx)`.
    fn add_edge_to_node(&mut self, n: usize, idx: usize) -> OResult<()> {
        let np = self.node(n)?;
        let slot = np.adj_base + np.n_adj;
        if slot >= self.adj.len() {
            self.adj.resize(slot + 1, 0);
        }
        self.adj[slot] = idx;
        self.node_mut(n)?.n_adj += 1;
        Ok(())
    }

    /// `createSNode(g)`. C writes past `nodes` if the maze makes more side
    /// nodes than `mkMazeGraph` allocated; the port fails.
    pub fn create_snode(&mut self) -> OResult<usize> {
        let i = self.nnodes;
        self.node_mut(i)?.index = i;
        self.nnodes += 1;
        Ok(i)
    }

    /// `createSEdge(g, v1, v2, wt)`.
    pub fn create_sedge(&mut self, v1: usize, v2: usize, wt: f64) -> OResult<usize> {
        let idx = self.nedges;
        let e = SEdge {
            weight: wt,
            cnt: 0,
            v1: self.node(v1)?.index,
            v2: self.node(v2)?.index,
        };
        self.nedges += 1;
        if idx < self.edges.len() {
            self.edges[idx] = e;
        } else {
            self.edges.push(e);
        }
        self.add_edge_to_node(v1, idx)?;
        self.add_edge_to_node(v2, idx)?;
        Ok(idx)
    }

    fn adjacent_node(&self, e: usize, n: usize) -> OResult<usize> {
        let ed = self.edge(e)?;
        Ok(if ed.v1 == self.node(n)?.index {
            ed.v2
        } else {
            ed.v1
        })
    }

    /// `shortPath(pq, g, from, to)`.
    pub fn short_path(&mut self, pq: &mut Pq, from: usize, to: usize) -> OResult<()> {
        const UNSEEN: i32 = i32::MIN;
        for np in self.nodes.get_mut(..self.nnodes).need()? {
            np.n_val = UNSEEN;
        }
        pq.init();
        pq.insert(self, from)?;
        self.node_mut(from)?.n_dad = None;
        self.node_mut(from)?.n_val = 0;
        while let Some(n) = pq.remove(self)? {
            // `N_VAL(n) *= -1`; [`distance`] keeps it off `INT_MIN`.
            let np = self.node_mut(n)?;
            np.n_val = np.n_val.checked_neg().need()?;
            if n == to {
                break;
            }
            for y in 0..self.node(n)?.n_adj {
                let e = self.adj_edge(n, y)?;
                let adjn = self.adjacent_node(e, n)?;
                if self.node(adjn)?.n_val < 0 {
                    let d = distance(self.node(n)?.n_val, self.edge(e)?.weight)?;
                    if self.node(adjn)?.n_val == UNSEEN {
                        self.node_mut(adjn)?.n_val = d;
                        pq.insert(self, adjn)?;
                        let ap = self.node_mut(adjn)?;
                        ap.n_dad = Some(n);
                        ap.n_edge = Some(e);
                    } else if self.node(adjn)?.n_val < d {
                        pq.update(self, adjn, d)?;
                        let ap = self.node_mut(adjn)?;
                        ap.n_dad = Some(n);
                        ap.n_edge = Some(e);
                    }
                }
            }
        }
        Ok(())
    }
}

/// `d = -(N_VAL(n) + E_WT(e))` in `shortPath`: an `int` plus a `double`,
/// negated and truncated back to `int`. Weights grow with the cell sizes, so
/// on huge coordinates the `double` does not fit an `int` (#229). C's
/// conversion is then undefined, and the value it gets breaks the heap:
/// `PQcheck` fails its assertion, or `N_VAL(n) *= -1` overflows on
/// `INT_MIN` (`UNSEEN`). A positive `d` (C's weights are never negative) would
/// make `PQupheap` climb past its guard and loop. The port fails on all of
/// these. On the inputs C routes every distance is in range (a scan of
/// neato pins finds C's first `PQcheck` failure where the port's first
/// rejection is, near 3e7 inches); an `INT_MIN` that C would only store on
/// the target node, and never read, is rejected too.
/// (A positive `d` always loops in C: every value in the heap is at or below
/// 0, so it climbs to the root and then past the guard forever.)
fn distance(n_val: i32, weight: f64) -> OResult<i32> {
    let d = (-(f64::from(n_val) + weight)).trunc();
    ensure(d > f64::from(i32::MIN) && d <= 0.0)?;
    Ok(d as i32)
}
/// `pq_t` (fPQ.c): a max-heap of node indices on `n_val`; slot 0 is a guard
/// with value 0.
#[derive(Debug, Clone, Default)]
pub struct Pq {
    pq: Vec<usize>,
    cnt: usize,
    size: usize,
}

const GUARD: usize = usize::MAX;

impl Pq {
    /// `PQgen(sz)`.
    pub fn new(sz: usize) -> Self {
        let mut pq = vec![0; sz + 1];
        pq[0] = GUARD;
        Pq {
            pq,
            cnt: 0,
            size: sz,
        }
    }

    fn val(g: &SGraph, n: usize) -> OResult<i32> {
        Ok(if n == GUARD { 0 } else { g.node(n)?.n_val })
    }

    /// `PQinit(pq)`.
    pub fn init(&mut self) {
        self.cnt = 0;
    }

    /// `PQupheap(pq, k)`. The guard's value 0 stops the climb, as
    /// [`distance`] keeps every value at or below 0.
    fn upheap(&mut self, g: &mut SGraph, mut k: usize) -> OResult<()> {
        let x = *self.pq.at(k)?;
        let v = g.node(x)?.n_val;
        let mut next = k / 2;
        loop {
            let n = *self.pq.at(next)?;
            if Self::val(g, n)? >= v {
                break;
            }
            self.pq[k] = n;
            g.node_mut(n)?.n_idx = k as i32;
            k = next;
            next /= 2;
        }
        self.pq[k] = x;
        g.node_mut(x)?.n_idx = k as i32;
        Ok(())
    }

    /// `PQ_insert(pq, np)`.
    pub fn insert(&mut self, g: &mut SGraph, np: usize) -> OResult<()> {
        if self.cnt == self.size {
            return Err(Fail::HeapOverflow);
        }
        self.cnt += 1;
        *self.pq.at_mut(self.cnt)? = np;
        self.upheap(g, self.cnt)
    }

    fn downheap(&mut self, g: &mut SGraph, mut k: usize) -> OResult<()> {
        let x = *self.pq.at(k)?;
        let v = g.node(x)?.n_val;
        let lim = self.cnt / 2;
        while k <= lim {
            let mut j = k + k;
            let mut n = *self.pq.at(j)?;
            if j < self.cnt && g.node(n)?.n_val < g.node(*self.pq.at(j + 1)?)?.n_val {
                j += 1;
                n = self.pq[j];
            }
            if v >= g.node(n)?.n_val {
                break;
            }
            self.pq[k] = n;
            g.node_mut(n)?.n_idx = k as i32;
            k = j;
        }
        self.pq[k] = x;
        g.node_mut(x)?.n_idx = k as i32;
        Ok(())
    }

    /// `PQremove(pq)`.
    pub fn remove(&mut self, g: &mut SGraph) -> OResult<Option<usize>> {
        if self.cnt == 0 {
            return Ok(None);
        }
        let n = *self.pq.at(1)?;
        self.pq[1] = *self.pq.at(self.cnt)?;
        self.cnt -= 1;
        if self.cnt > 0 {
            self.downheap(g, 1)?;
        }
        Ok(Some(n))
    }

    /// `PQupdate(pq, n, d)`.
    pub fn update(&mut self, g: &mut SGraph, n: usize, d: i32) -> OResult<()> {
        g.node_mut(n)?.n_val = d;
        let k = usize::try_from(g.node(n)?.n_idx).map_err(|_| Fail::here())?;
        self.upheap(g, k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `d = -(N_VAL(n) + E_WT(e))` is kept where C's `int` arithmetic is
    /// defined and `PQupheap` stops at its guard (#229).
    #[test]
    fn distance_stays_where_cs_int_arithmetic_is_defined() {
        assert_eq!(distance(0, 0.0), Ok(0));
        assert_eq!(distance(5, 500.7), Ok(-505));
        assert_eq!(distance(0, 2_147_483_647.5), Ok(-2_147_483_647));
        // -2^31 is `UNSEEN`, and `N_VAL(n) *= -1` overflows on it.
        assert!(distance(0, 2_147_483_648.0).is_err());
        assert!(distance(0, 1e12).is_err());
        assert!(distance(i32::MAX, 1.0).is_err());
        assert!(distance(0, f64::NAN).is_err());
        // C's PQupheap climbs past its guard forever on a positive value.
        assert!(distance(0, -1.0).is_err());
    }
}
