//! Port of `lib/dotgen/acyclic.c`: break cycles by reversing back edges.

use super::Dot;
use super::types::*;

impl Dot<'_> {
    pub fn reverse_edge(&mut self, e: E) {
        self.delete_fast_edge(e);
        let (t, h) = (self.edges[e].tail, self.edges[e].head);
        if let Some(f) = self.find_fast_edge(h, t) {
            self.merge_oneway(e, f);
        } else {
            self.virtual_edge(h, t, Some(e));
        }
    }

    /// Recursive `dfs` of acyclic.c with an explicit stack of `(node, index)`.
    fn acyclic_dfs(&mut self, n: N) {
        if self.nodes[n].mark != 0 {
            return;
        }
        self.nodes[n].mark = 1;
        self.nodes[n].onstack = true;
        let mut stack: Vec<(N, usize)> = vec![(n, 0)];
        while let Some(&mut (v, ref mut i)) = stack.last_mut() {
            if *i < self.nodes[v].out.len() {
                let e = self.nodes[v].out[*i];
                let w = self.edges[e].head;
                if self.nodes[w].onstack {
                    self.reverse_edge(e);
                    // i-- ; i++  => same index again
                } else {
                    *i += 1;
                    if self.nodes[w].mark == 0 {
                        self.nodes[w].mark = 1;
                        self.nodes[w].onstack = true;
                        stack.push((w, 0));
                    }
                }
            } else {
                self.nodes[v].onstack = false;
                stack.pop();
            }
        }
    }

    pub fn acyclic(&mut self, g: G) {
        for c in 0..self.graphs[g].comp.len() {
            self.graphs[g].nlist = self.graphs[g].comp[c];
            let mut n = self.graphs[g].nlist;
            while let Some(x) = n {
                self.nodes[x].mark = 0;
                n = self.nodes[x].next;
            }
            let mut n = self.graphs[g].nlist;
            while let Some(x) = n {
                self.acyclic_dfs(x);
                n = self.nodes[x].next;
            }
        }
    }
}
