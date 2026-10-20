//! The subset of cgraph (`lib/cgraph`) that dotgen uses: node/edge/subgraph
//! iteration in sequence order, membership tests, and the few mutations dot
//! performs on subgraphs (`agdelete` of a node from a cluster, `agsubedge`,
//! `agsubnode`) and on its private auxiliary graphs (newrank `Xg`, the
//! `checkLabelOrder` graph).

use super::Dot;
use super::types::*;

impl Dot<'_> {
    /// Creates a new empty cgraph root graph (`agopen`).
    pub fn cg_open(&mut self, name: &str) -> G {
        let g = self.graphs.len();
        self.graphs.push(GraphInfo {
            name: name.to_string(),
            root: g,
            bound: true,
            ..GraphInfo::default()
        });
        g
    }

    /// `agsubg(g, name, 1)`: finds or creates a child subgraph.
    pub fn cg_subg(&mut self, g: G, name: &str) -> G {
        for &s in &self.graphs[g].subgs {
            if self.graphs[s].name == name {
                return s;
            }
        }
        let s = self.graphs.len();
        let root = self.graphs[g].root;
        let attrs = self.graphs[g].attrs.clone();
        self.graphs.push(GraphInfo {
            name: name.to_string(),
            root,
            cg_parent: Some(g),
            attrs,
            ..GraphInfo::default()
        });
        self.graphs[g].subgs.push(s);
        s
    }

    /// `agdelsubg` (only used for subgraphs with no remaining members).
    pub fn cg_delsubg(&mut self, g: G, s: G) {
        self.graphs[g].subgs.retain(|&x| x != s);
    }

    /// Nodes of `g` in `agfstnode`/`agnxtnode` order (snapshot).
    pub fn nodes_of(&self, g: G) -> Vec<N> {
        self.graphs[g].nodes.iter().copied().collect()
    }

    /// First node of `g` (`agfstnode`).
    pub fn agfstnode(&self, g: G) -> Option<N> {
        self.graphs[g].nodes.iter().next().copied()
    }

    /// `agnnodes(g)`
    pub fn agnnodes(&self, g: G) -> usize {
        self.graphs[g].nodes.len()
    }

    /// `agnedges(g)`
    pub fn agnedges(&self, g: G) -> usize {
        self.graphs[g].edges.len()
    }

    pub fn agcontains_node(&self, g: G, n: N) -> bool {
        self.graphs[g].nodes.contains(&n)
    }

    pub fn agcontains_edge(&self, g: G, e: E) -> bool {
        self.graphs[g].edges.contains(&e)
    }

    fn is_root(&self, g: G) -> bool {
        self.graphs[g].root == g
    }

    /// Out-edges of `n` in `g` (`agfstout`/`agnxtout` order).
    pub fn out_edges(&self, g: G, n: N) -> Vec<E> {
        if self.is_root(g) {
            self.nodes[n].cg_out.clone()
        } else {
            self.nodes[n]
                .cg_out
                .iter()
                .copied()
                .filter(|e| self.graphs[g].edges.contains(e))
                .collect()
        }
    }

    /// In-edges of `n` in `g` (`agfstin`/`agnxtin` order).
    pub fn in_edges(&self, g: G, n: N) -> Vec<E> {
        if self.is_root(g) {
            self.nodes[n].cg_in.clone()
        } else {
            self.nodes[n]
                .cg_in
                .iter()
                .copied()
                .filter(|e| self.graphs[g].edges.contains(e))
                .collect()
        }
    }

    /// `agfstout(g, n)`
    pub fn agfstout(&self, g: G, n: N) -> Option<E> {
        self.nodes[n]
            .cg_out
            .iter()
            .copied()
            .find(|e| self.is_root(g) || self.graphs[g].edges.contains(e))
    }

    /// `agfstin(g, n)`
    pub fn agfstin(&self, g: G, n: N) -> Option<E> {
        self.nodes[n]
            .cg_in
            .iter()
            .copied()
            .find(|e| self.is_root(g) || self.graphs[g].edges.contains(e))
    }

    /// `agfstedge`/`agnxtedge`: out-edges, then in-edges that are not loops.
    pub fn all_edges(&self, g: G, n: N) -> Vec<E> {
        let mut v = self.out_edges(g, n);
        for e in self.in_edges(g, n) {
            if self.edges[e].tail != n {
                v.push(e);
            }
        }
        v
    }

    /// `agdegree(g, n, in, out)`
    pub fn agdegree(&self, g: G, n: N, inn: bool, out: bool) -> usize {
        let mut d = 0;
        if inn {
            d += self.in_edges(g, n).len();
        }
        if out {
            d += self.out_edges(g, n).len();
        }
        d
    }

    /// Descendant subgraphs of `g` including `g`, post-order (as `agapply(.., false)`).
    fn subtree_postorder(&self, g: G, out: &mut Vec<G>) {
        for &s in &self.graphs[g].subgs {
            self.subtree_postorder(s, out);
        }
        out.push(g);
    }

    /// `agdelete(g, n)` for a node: removes the node image from `g` and all its
    /// subgraphs (and from the root entirely when `g` is a root).
    pub fn agdelnode(&mut self, g: G, n: N) {
        if !self.graphs[g].nodes.contains(&n) {
            return;
        }
        if self.is_root(g) {
            for e in self.all_edges(g, n) {
                self.agdeledge(g, e);
            }
        }
        let mut gs = Vec::new();
        self.subtree_postorder(g, &mut gs);
        for s in gs {
            if self.graphs[s].nodes.remove(&n) {
                let inc: Vec<E> = self.nodes[n]
                    .cg_out
                    .iter()
                    .chain(self.nodes[n].cg_in.iter())
                    .copied()
                    .collect();
                for e in inc {
                    self.graphs[s].edges.remove(&e);
                }
            }
        }
    }

    /// `agdeledge(g, e)`
    pub fn agdeledge(&mut self, g: G, e: E) {
        if !self.graphs[g].edges.contains(&e) {
            return;
        }
        let mut gs = Vec::new();
        self.subtree_postorder(g, &mut gs);
        for s in gs {
            self.graphs[s].edges.remove(&e);
        }
        if self.is_root(g) {
            let (t, h) = (self.edges[e].tail, self.edges[e].head);
            self.nodes[t].cg_out.retain(|&x| x != e);
            self.nodes[h].cg_in.retain(|&x| x != e);
        }
    }

    /// `agsubnode(g, n, 1)`: installs `n` in `g` and its ancestors.
    pub fn agsubnode(&mut self, g: G, n: N) {
        let mut cur = Some(g);
        while let Some(c) = cur {
            if !self.graphs[c].nodes.insert(n) {
                break;
            }
            cur = self.graphs[c].cg_parent;
        }
    }

    /// `agsubedge(g, e, 1)`: installs `e` (and its endpoints) in `g` and its ancestors.
    pub fn agsubedge(&mut self, g: G, e: E) {
        let (t, h) = (self.edges[e].tail, self.edges[e].head);
        self.agsubnode(g, t);
        self.agsubnode(g, h);
        let mut cur = Some(g);
        while let Some(c) = cur {
            if !self.graphs[c].edges.insert(e) {
                break;
            }
            cur = self.graphs[c].cg_parent;
        }
    }

    /// `agnode(g, name, 1)` creating a new cgraph node in `g`'s root.
    pub fn cg_new_node(&mut self, g: G, name: &str) -> N {
        let root = self.graphs[g].root;
        let n = self.nodes.len();
        self.nodes.push(NodeInfo {
            name: name.to_string(),
            cg_root: Some(root),
            ..NodeInfo::default()
        });
        self.agsubnode(g, n);
        n
    }

    /// Finds a node of `g`'s root by name (`agnode(g, name, 0)` on a root).
    pub fn cg_find_node(&self, g: G, name: &str) -> Option<N> {
        self.graphs[g]
            .nodes
            .iter()
            .copied()
            .find(|&n| self.nodes[n].name == name)
    }

    /// `agnode(g, name, 1)`: find or create.
    pub fn cg_node(&mut self, g: G, name: &str) -> N {
        if let Some(n) = self.cg_find_node(g, name) {
            return n;
        }
        self.cg_new_node(g, name)
    }

    /// `agfindedge(g, t, h)` (`agedge(g, t, h, NULL, 0)`): for undirected
    /// semantics cgraph also matches `h -> t`; dot's private graphs are
    /// strict directed, so only `t -> h` is searched.
    pub fn cg_find_edge(&self, g: G, t: N, h: N) -> Option<E> {
        self.nodes[t]
            .cg_out
            .iter()
            .copied()
            .find(|&e| self.edges[e].head == h && self.graphs[g].edges.contains(&e))
    }

    /// `agedge(g, t, h, NULL, 1)` on a strict directed graph.
    pub fn cg_edge(&mut self, g: G, t: N, h: N) -> E {
        if let Some(e) = self.cg_find_edge(g, t, h) {
            return e;
        }
        let root = self.graphs[g].root;
        // A strict graph also refuses an edge that exists in the root.
        if let Some(e) = self.cg_find_edge(root, t, h) {
            self.agsubedge(g, e);
            return e;
        }
        let e = self.edges.len();
        self.edge_seq += 1;
        self.edges.push(EdgeInfo {
            tail: t,
            head: h,
            seq: self.edge_seq,
            ..EdgeInfo::default()
        });
        self.cg_insert_out(t, e);
        self.cg_insert_in(h, e);
        self.agsubedge(g, e);
        e
    }

    /// Key of an out-edge in cgraph's `agedgeseqcmpf` order.
    pub fn out_key(&self, e: E) -> (N, u64) {
        (self.edges[e].head, self.edges[e].seq)
    }

    fn cg_insert_out(&mut self, t: N, e: E) {
        let k = self.out_key(e);
        let pos = self.nodes[t]
            .cg_out
            .iter()
            .position(|&x| self.out_key(x) > k)
            .unwrap_or(self.nodes[t].cg_out.len());
        self.nodes[t].cg_out.insert(pos, e);
    }

    fn cg_insert_in(&mut self, h: N, e: E) {
        let k = (self.edges[e].tail, self.edges[e].seq);
        let pos = self.nodes[h]
            .cg_in
            .iter()
            .position(|&x| (self.edges[x].tail, self.edges[x].seq) > k)
            .unwrap_or(self.nodes[h].cg_in.len());
        self.nodes[h].cg_in.insert(pos, e);
    }
}
