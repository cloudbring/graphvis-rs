//! Port of `lib/dotgen/decomp.c`: connected components of the fast graph.

use super::Dot;
use super::types::*;

impl Dot<'_> {
    fn begin_component(&mut self, g: G) {
        self.decomp_last = None;
        self.graphs[g].nlist = None;
    }

    fn add_to_component(&mut self, g: G, n: N) {
        self.nodes[n].mark = self.cmark;
        if let Some(last) = self.decomp_last {
            self.nodes[n].prev = Some(last);
            self.nodes[last].next = Some(n);
        } else {
            self.nodes[n].prev = None;
            self.graphs[g].nlist = Some(n);
        }
        self.decomp_last = Some(n);
        self.nodes[n].next = None;
    }

    fn end_component(&mut self, g: G) {
        let head = self.graphs[g].nlist;
        self.graphs[g].comp.push(head);
    }

    fn search_component(&mut self, stk: &mut Vec<N>, g: G, n: N) {
        self.nodes[n].mark = self.cmark + 1;
        stk.push(n);
        while let Some(n) = stk.pop() {
            if self.nodes[n].mark == self.cmark {
                continue;
            }
            self.add_to_component(g, n);
            if std::env::var_os("GVDUMP").is_some() {
                let nm = |x: N| self.nodes[x].name.clone();
                let ins: String = self.nodes[n]
                    .in_
                    .iter()
                    .map(|&e| format!(" {}", nm(self.edges[e].tail)))
                    .collect();
                let outs: String = self.nodes[n]
                    .out
                    .iter()
                    .map(|&e| format!(" {}", nm(self.edges[e].head)))
                    .collect();
                graphviz_pure_core::diag::dump(&format!(
                    "SC {} in:{} out:{}",
                    self.nodes[n].name, ins, outs
                ));
            }
            let vecs = [
                self.nodes[n].flat_in.clone(),
                self.nodes[n].flat_out.clone(),
                self.nodes[n].in_.clone(),
                self.nodes[n].out.clone(),
            ];
            for v in &vecs {
                for &e in v.iter().rev() {
                    let mut other = self.edges[e].head;
                    if other == n {
                        other = self.edges[e].tail;
                    }
                    if self.nodes[other].mark != self.cmark && other == self.uf_find(other) {
                        self.nodes[other].mark = self.cmark + 1;
                        stk.push(other);
                    }
                }
            }
        }
    }

    pub fn decompose(&mut self, g: G, pass: i32) {
        self.cmark = self.cmark.wrapping_add(1);
        if self.cmark == 0 {
            self.cmark = 1;
        }
        self.graphs[g].comp.clear();
        let mut stk = Vec::new();
        for n in self.nodes_of(g) {
            let mut v = n;
            if pass > 0
                && let Some(subg) = self.nodes[v].clust
            {
                v = self.graphs[subg].rankleader[self.nodes[v].rank as usize].unwrap();
            } else if v != self.uf_find(v) {
                continue;
            }
            if self.nodes[v].mark != self.cmark {
                self.begin_component(g);
                self.search_component(&mut stk, g, v);
                self.end_component(g);
            }
        }
    }
}
