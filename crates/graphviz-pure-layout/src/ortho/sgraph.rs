//! Port of `lib/ortho/sgraph.c` and `lib/ortho/fPQ.c`: the search graph of
//! the maze and Dijkstra's shortest path over it.

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

    /// `gsave(G)`.
    pub fn gsave(&mut self) {
        self.save_nnodes = self.nnodes;
        self.save_nedges = self.nedges;
        for i in 0..self.nnodes {
            self.nodes[i].save_n_adj = self.nodes[i].n_adj;
        }
    }

    /// `reset(G)`.
    pub fn reset(&mut self) {
        self.nnodes = self.save_nnodes;
        self.nedges = self.save_nedges;
        for i in 0..self.nnodes {
            self.nodes[i].n_adj = self.nodes[i].save_n_adj;
        }
        for i in self.nnodes..self.nnodes + 2 {
            self.nodes[i].n_adj = 0;
        }
    }

    /// `initSEdges(g, maxdeg)`.
    pub fn init_sedges(&mut self, maxdeg: usize) {
        self.adj = vec![0; 6 * self.nnodes + 2 * maxdeg];
        let mut base = 0;
        for i in 0..self.nnodes + 2 {
            self.nodes[i].adj_base = base;
            base += if i < self.nnodes { 6 } else { maxdeg };
        }
    }

    /// `adj_edge_list[k]` of node `n`.
    pub fn adj_edge(&self, n: usize, k: usize) -> usize {
        self.adj
            .get(self.nodes[n].adj_base + k)
            .copied()
            .unwrap_or(0)
    }

    /// `addEdgeToNode(np, idx)`.
    fn add_edge_to_node(&mut self, n: usize, idx: usize) {
        let slot = self.nodes[n].adj_base + self.nodes[n].n_adj;
        if slot >= self.adj.len() {
            self.adj.resize(slot + 1, 0);
        }
        self.adj[slot] = idx;
        self.nodes[n].n_adj += 1;
    }

    /// `createSNode(g)`.
    pub fn create_snode(&mut self) -> usize {
        let i = self.nnodes;
        self.nodes[i].index = i;
        self.nnodes += 1;
        i
    }

    /// `createSEdge(g, v1, v2, wt)`.
    pub fn create_sedge(&mut self, v1: usize, v2: usize, wt: f64) -> usize {
        let idx = self.nedges;
        self.nedges += 1;
        let e = SEdge {
            weight: wt,
            cnt: 0,
            v1: self.nodes[v1].index,
            v2: self.nodes[v2].index,
        };
        if idx < self.edges.len() {
            self.edges[idx] = e;
        } else {
            self.edges.push(e);
        }
        self.add_edge_to_node(v1, idx);
        self.add_edge_to_node(v2, idx);
        idx
    }

    fn adjacent_node(&self, e: usize, n: usize) -> usize {
        let ed = &self.edges[e];
        if ed.v1 == self.nodes[n].index {
            ed.v2
        } else {
            ed.v1
        }
    }

    /// Whether the last [`SGraph::short_path`] reached its `to` node `n`.
    /// The search runs until it pops `to` or empties the queue, so `to` is
    /// either reached or still `UNSEEN`, with a stale `n_dad`.
    pub fn reached(&self, n: usize) -> bool {
        self.nodes[n].n_val != UNSEEN
    }

    /// `shortPath(pq, g, from, to)`: returns false on heap overflow.
    pub fn short_path(&mut self, pq: &mut Pq, from: usize, to: usize) -> bool {
        for x in 0..self.nnodes {
            self.nodes[x].n_val = UNSEEN;
        }
        pq.init();
        if !pq.insert(self, from) {
            return false;
        }
        self.nodes[from].n_dad = None;
        self.nodes[from].n_val = 0;
        while let Some(n) = pq.remove(self) {
            self.nodes[n].n_val *= -1;
            if n == to {
                break;
            }
            for y in 0..self.nodes[n].n_adj {
                let e = self.adj_edge(n, y);
                let adjn = self.adjacent_node(e, n);
                if self.nodes[adjn].n_val < 0 {
                    // int + double, negated, back to int (truncation)
                    let d = (-(f64::from(self.nodes[n].n_val) + self.edges[e].weight)) as i32;
                    if self.nodes[adjn].n_val == UNSEEN {
                        self.nodes[adjn].n_val = d;
                        if !pq.insert(self, adjn) {
                            return false;
                        }
                        self.nodes[adjn].n_dad = Some(n);
                        self.nodes[adjn].n_edge = Some(e);
                    } else if self.nodes[adjn].n_val < d {
                        pq.update(self, adjn, d);
                        self.nodes[adjn].n_dad = Some(n);
                        self.nodes[adjn].n_edge = Some(e);
                    }
                }
            }
        }
        true
    }
}

/// `UNSEEN` (sgraph.c): the `n_val` of a node `shortPath` has not seen.
const UNSEEN: i32 = i32::MIN;

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

    fn val(g: &SGraph, n: usize) -> i32 {
        if n == GUARD { 0 } else { g.nodes[n].n_val }
    }

    /// `PQinit(pq)`.
    pub fn init(&mut self) {
        self.cnt = 0;
    }

    fn upheap(&mut self, g: &mut SGraph, mut k: usize) {
        let x = self.pq[k];
        let v = g.nodes[x].n_val;
        let mut next = k / 2;
        loop {
            let n = self.pq[next];
            if Self::val(g, n) >= v {
                break;
            }
            self.pq[k] = n;
            g.nodes[n].n_idx = k as i32;
            k = next;
            next /= 2;
        }
        self.pq[k] = x;
        g.nodes[x].n_idx = k as i32;
    }

    /// `PQ_insert(pq, np)`: false on overflow.
    pub fn insert(&mut self, g: &mut SGraph, np: usize) -> bool {
        if self.cnt == self.size {
            return false;
        }
        self.cnt += 1;
        self.pq[self.cnt] = np;
        self.upheap(g, self.cnt);
        true
    }

    fn downheap(&mut self, g: &mut SGraph, mut k: usize) {
        let x = self.pq[k];
        let v = g.nodes[x].n_val;
        let lim = self.cnt / 2;
        while k <= lim {
            let mut j = k + k;
            let mut n = self.pq[j];
            if j < self.cnt && g.nodes[n].n_val < g.nodes[self.pq[j + 1]].n_val {
                j += 1;
                n = self.pq[j];
            }
            if v >= g.nodes[n].n_val {
                break;
            }
            self.pq[k] = n;
            g.nodes[n].n_idx = k as i32;
            k = j;
        }
        self.pq[k] = x;
        g.nodes[x].n_idx = k as i32;
    }

    /// `PQremove(pq)`.
    pub fn remove(&mut self, g: &mut SGraph) -> Option<usize> {
        if self.cnt == 0 {
            return None;
        }
        let n = self.pq[1];
        self.pq[1] = self.pq[self.cnt];
        self.cnt -= 1;
        if self.cnt > 0 {
            self.downheap(g, 1);
        }
        Some(n)
    }

    /// `PQupdate(pq, n, d)`.
    pub fn update(&mut self, g: &mut SGraph, n: usize, d: i32) {
        g.nodes[n].n_val = d;
        let k = g.nodes[n].n_idx as usize;
        self.upheap(g, k);
    }
}
