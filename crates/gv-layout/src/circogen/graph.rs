//! A small model of the cgraph operations circo performs on its derived
//! graph and the graphs built from it.
//!
//! Every graph here is strict and undirected (`Agstrictundirected`), so a
//! node has at most one edge to another node and `agfstout` order (head
//! sequence, then edge sequence) reduces to head order. Nodes are identified
//! by the index of the original node they stand for: derived nodes are
//! created in original node order, and clones in derived order, so index
//! order is node sequence order. Edges live in an [`EdgeArena`] shared by all
//! graphs; subgraphs hold edges of their root by id, as `agsubedge` does.

use std::collections::BTreeMap;

/// Edge storage: `agtail`/`aghead` plus the `edata` and `ED_to_orig` fields.
#[derive(Debug, Default, Clone)]
pub struct EdgeArena {
    pub tail: Vec<usize>,
    pub head: Vec<usize>,
    /// `EDGEORDER(e)`.
    pub order: Vec<i32>,
    /// `ORIGE(e)`: for clone edges, the block edge they copy.
    pub orig: Vec<Option<usize>>,
}

impl EdgeArena {
    fn new_edge(&mut self, t: usize, h: usize) -> usize {
        self.tail.push(t);
        self.head.push(h);
        self.order.push(0);
        self.orig.push(None);
        self.tail.len() - 1
    }

    /// The other end of `e` seen from `n`.
    pub fn other(&self, e: usize, n: usize) -> usize {
        let h = self.head[e];
        if h == n { self.tail[e] } else { h }
    }
}

#[derive(Debug, Default, Clone)]
struct Adj {
    /// head -> edge.
    out: BTreeMap<usize, usize>,
    /// tail -> edge.
    inn: BTreeMap<usize, usize>,
}

/// A graph or subgraph: its node set and, per node, its edges.
#[derive(Debug, Default, Clone)]
pub struct CGraph {
    nodes: BTreeMap<usize, Adj>,
}

impl CGraph {
    /// `agfstnode`..`agnxtnode`.
    pub fn nodes(&self) -> Vec<usize> {
        self.nodes.keys().copied().collect()
    }

    pub fn first_node(&self) -> Option<usize> {
        self.nodes.keys().next().copied()
    }

    /// `agnnodes`.
    pub fn nnodes(&self) -> usize {
        self.nodes.len()
    }

    pub fn contains(&self, n: usize) -> bool {
        self.nodes.contains_key(&n)
    }

    /// `agsubnode(g, n, 1)` / `agnode(g, name, 1)`.
    pub fn add_node(&mut self, n: usize) {
        self.nodes.entry(n).or_default();
    }

    /// `agfstout`..`agnxtout`.
    pub fn outs(&self, n: usize) -> Vec<usize> {
        self.nodes
            .get(&n)
            .map(|a| a.out.values().copied().collect())
            .unwrap_or_default()
    }

    /// `agfstin`..`agnxtin`.
    pub fn ins(&self, n: usize) -> Vec<usize> {
        self.nodes
            .get(&n)
            .map(|a| a.inn.values().copied().collect())
            .unwrap_or_default()
    }

    /// `agfstedge`..`agnxtedge`: out-edges, then in-edges that are not loops.
    pub fn edges(&self, n: usize) -> Vec<usize> {
        let Some(a) = self.nodes.get(&n) else {
            return Vec::new();
        };
        a.out
            .values()
            .copied()
            .chain(a.inn.iter().filter(|(t, _)| **t != n).map(|(_, e)| *e))
            .collect()
    }

    /// `agsubedge(g, e, 1)`: add `e` and its endpoints.
    pub fn subedge(&mut self, arena: &EdgeArena, e: usize) {
        let (t, h) = (arena.tail[e], arena.head[e]);
        self.nodes.entry(t).or_default().out.insert(h, e);
        self.nodes.entry(h).or_default().inn.insert(t, e);
    }

    /// `agfindedge(g, t, h)` in an undirected graph.
    pub fn find_edge(&self, t: usize, h: usize) -> Option<usize> {
        let fwd = self.nodes.get(&t).and_then(|a| a.out.get(&h));
        fwd.or_else(|| self.nodes.get(&h).and_then(|a| a.out.get(&t)))
            .copied()
    }

    /// `agedge(g, t, h, NULL, 1)` on a strict undirected root graph: the
    /// existing edge between `t` and `h`, or a new one `t -> h`.
    pub fn edge(&mut self, arena: &mut EdgeArena, t: usize, h: usize) -> usize {
        if let Some(e) = self.find_edge(t, h) {
            return e;
        }
        let e = arena.new_edge(t, h);
        self.subedge(arena, e);
        e
    }

    /// `agdelete(g, e)`: remove `e` from this graph.
    pub fn delete_edge(&mut self, arena: &EdgeArena, e: usize) {
        let (t, h) = (arena.tail[e], arena.head[e]);
        if let Some(a) = self.nodes.get_mut(&t)
            && a.out.get(&h) == Some(&e)
        {
            a.out.remove(&h);
        }
        if let Some(a) = self.nodes.get_mut(&h)
            && a.inn.get(&t) == Some(&e)
        {
            a.inn.remove(&t);
        }
    }

    /// `agdelete(g, n)`: remove `n` and its edges.
    pub fn delete_node(&mut self, n: usize) {
        let Some(a) = self.nodes.remove(&n) else {
            return;
        };
        for h in a.out.keys() {
            if let Some(b) = self.nodes.get_mut(h) {
                b.inn.remove(&n);
            }
        }
        for t in a.inn.keys() {
            if let Some(b) = self.nodes.get_mut(t) {
                b.out.remove(&n);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_undirected_edges_and_order() {
        let mut a = EdgeArena::default();
        let mut g = CGraph::default();
        for n in 0..4 {
            g.add_node(n);
        }
        let e1 = g.edge(&mut a, 2, 0);
        let e2 = g.edge(&mut a, 0, 3);
        let e3 = g.edge(&mut a, 0, 1);
        // agedge finds the existing edge in either orientation
        assert_eq!(g.edge(&mut a, 0, 2), e1);
        assert_eq!(g.find_edge(3, 0), Some(e2));
        // out-edges by head sequence, then non-loop in-edges by tail
        assert_eq!(g.edges(0), vec![e3, e2, e1]);
        g.delete_node(2);
        assert_eq!(g.edges(0), vec![e3, e2]);
        g.delete_edge(&a, e3);
        assert_eq!(g.edges(0), vec![e2]);
        assert_eq!(g.nnodes(), 3);
    }
}
