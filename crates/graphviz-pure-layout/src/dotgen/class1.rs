//! Port of `lib/dotgen/class1.c`: build the fast graph for ranking.

use super::Dot;
use super::types::*;

impl Dot<'_> {
    fn interclust1(&mut self, g: G, t: N, h: N, e: E) {
        let et = self.edges[e].tail;
        let eh = self.edges[e].head;
        let t_rank = match self.nodes[et].clust {
            Some(c) => self.nodes[et].rank - self.nodes[self.graphs[c].leader.unwrap()].rank,
            None => 0,
        };
        let h_rank = match self.nodes[eh].clust {
            Some(c) => self.nodes[eh].rank - self.nodes[self.graphs[c].leader.unwrap()].rank,
            None => 0,
        };
        let offset = self.edges[e].minlen + t_rank - h_rank;
        let (t_len, h_len) = if offset > 0 {
            (0, offset)
        } else {
            (-offset, 0)
        };
        let v = self.virtual_node(g);
        self.nodes[v].node_type = SLACKNODE;
        let t0 = self.uf_find(t);
        let h0 = self.uf_find(h);
        let w = self.edges[e].weight;
        // No overflow check in C (undefined behaviour); the port wraps.
        let rt = self.make_aux_edge(v, t0, t_len as f64, CL_BACK.wrapping_mul(w));
        let rh = self.make_aux_edge(v, h0, h_len as f64, w);
        self.edges[rt].to_orig = Some(e);
        self.edges[rh].to_orig = Some(e);
    }

    pub fn class1(&mut self, g: G) {
        self.mark_clusters(g);
        for n in self.nodes_of(g) {
            for e in self.out_edges(g, n) {
                if self.edges[e].to_virt.is_some() {
                    continue;
                }
                if self.nonconstraint_edge(e) {
                    continue;
                }
                let (et, eh) = (self.edges[e].tail, self.edges[e].head);
                let t = self.uf_find(et);
                let h = self.uf_find(eh);
                if t == h {
                    continue;
                }
                if self.nodes[t].clust.is_some() || self.nodes[h].clust.is_some() {
                    self.interclust1(g, et, eh, e);
                    continue;
                }
                if let Some(rep) = self.find_fast_edge(t, h) {
                    self.merge_oneway(e, rep);
                } else {
                    self.virtual_edge(t, h, Some(e));
                }
            }
        }
    }
}
