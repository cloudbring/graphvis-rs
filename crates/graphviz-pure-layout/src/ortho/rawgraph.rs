//! Port of `lib/ortho/rawgraph.c`: small directed graphs on segment indices
//! with a DFS topological sort.

use super::fail::{At, OResult};

const UNSCANNED: i32 = 0;
const SCANNING: i32 = 1;
const SCANNED: i32 = 2;

/// `vertex`.
#[derive(Debug, Clone, Default)]
pub struct Vertex {
    pub color: i32,
    pub topsort_order: i32,
    pub adj_list: Vec<usize>,
}

/// `rawgraph`.
#[derive(Debug, Clone, Default)]
pub struct RawGraph {
    pub vertices: Vec<Vertex>,
}

impl RawGraph {
    /// `make_graph(n)`.
    pub fn new(n: usize) -> Self {
        RawGraph {
            vertices: vec![Vertex::default(); n],
        }
    }

    /// `insert_edge(g, v1, v2)`.
    pub fn insert_edge(&mut self, v1: usize, v2: usize) -> OResult<()> {
        if !self.edge_exists(v1, v2)? {
            self.vertices[v1].adj_list.push(v2);
        }
        Ok(())
    }

    /// `remove_redge(g, v1, v2)`.
    pub fn remove_redge(&mut self, v1: usize, v2: usize) -> OResult<()> {
        let l1 = &mut self.vertices.at_mut(v1)?.adj_list;
        if let Some(i) = l1.iter().position(|&x| x == v2) {
            l1.remove(i);
        }
        let l2 = &mut self.vertices.at_mut(v2)?.adj_list;
        if let Some(i) = l2.iter().position(|&x| x == v1) {
            l2.remove(i);
        }
        Ok(())
    }

    /// `edge_exists(g, v1, v2)`.
    pub fn edge_exists(&self, v1: usize, v2: usize) -> OResult<bool> {
        Ok(self.vertices.at(v1)?.adj_list.contains(&v2))
    }

    /// `DFS_visit(g, v, time, sp)`, with an explicit stack: C recurses once
    /// per vertex on the path, and a channel can hold one segment per edge.
    /// The visiting order is C's (`time` is never read, so it is dropped).
    fn dfs_visit(&mut self, v: usize, sp: &mut Vec<usize>) -> OResult<()> {
        self.vertices.at_mut(v)?.color = SCANNING;
        let mut stack = vec![(v, 0usize)];
        while let Some(top) = stack.last_mut() {
            let (u, i) = *top;
            if let Some(&id) = self.vertices[u].adj_list.get(i) {
                top.1 += 1;
                let w = self.vertices.at_mut(id)?;
                if w.color == UNSCANNED {
                    w.color = SCANNING;
                    stack.push((id, 0));
                }
            } else {
                self.vertices[u].color = SCANNED;
                sp.push(u);
                stack.pop();
            }
        }
        Ok(())
    }

    /// `top_sort(g)`.
    pub fn top_sort(&mut self) -> OResult<()> {
        let n = self.vertices.len();
        if n == 0 {
            return Ok(());
        }
        if n == 1 {
            self.vertices[0].topsort_order = 0;
            return Ok(());
        }
        let mut sp = Vec::with_capacity(n);
        for i in 0..n {
            if self.vertices[i].color == UNSCANNED {
                self.dfs_visit(i, &mut sp)?;
            }
        }
        let mut count = 0;
        while let Some(v) = sp.pop() {
            self.vertices[v].topsort_order = count;
            count += 1;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// C's recursive `DFS_visit`, to check the port's explicit stack against.
    fn dfs_recursive(g: &mut RawGraph, v: usize, sp: &mut Vec<usize>) {
        g.vertices[v].color = SCANNING;
        for id in g.vertices[v].adj_list.clone() {
            if g.vertices[id].color == UNSCANNED {
                dfs_recursive(g, id, sp);
            }
        }
        g.vertices[v].color = SCANNED;
        sp.push(v);
    }

    fn orders(g: &RawGraph) -> Vec<i32> {
        g.vertices.iter().map(|v| v.topsort_order).collect()
    }

    #[test]
    fn top_sort_visits_in_cs_recursive_order() {
        crate::cutil::with_rand_state(&mut crate::cutil::RandState::default(), || {
            crate::cutil::srand48(7);
            top_sort_cases();
        });
    }

    fn top_sort_cases() {
        for n in 2..40 {
            let mut g = RawGraph::new(n);
            for _ in 0..2 * n {
                let a = (crate::cutil::drand48() * n as f64) as usize;
                let b = (crate::cutil::drand48() * n as f64) as usize;
                g.insert_edge(a, b).unwrap();
            }
            let mut want = g.clone();
            let mut sp = Vec::new();
            for i in 0..n {
                if want.vertices[i].color == UNSCANNED {
                    dfs_recursive(&mut want, i, &mut sp);
                }
            }
            for (count, v) in sp.into_iter().rev().enumerate() {
                want.vertices[v].topsort_order = count as i32;
            }
            g.top_sort().unwrap();
            assert_eq!(orders(&g), orders(&want), "n = {n}");
        }
    }

    /// A channel holds one segment per edge through it; a long chain of
    /// ordering constraints must not overflow the stack.
    #[test]
    fn top_sort_of_a_long_chain_does_not_recurse() {
        let n = 1_000_000;
        let mut g = RawGraph::new(n);
        for i in 0..n - 1 {
            g.vertices[i].adj_list.push(i + 1);
        }
        g.top_sort().unwrap();
        assert_eq!(g.vertices[n - 1].topsort_order, (n - 1) as i32);
    }
}
