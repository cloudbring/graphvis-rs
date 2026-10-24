//! Port of `lib/dotgen/flat.c`: flat edges, flat edge label nodes.

use super::Dot;
use super::types::*;
use graphviz_pure_core::cnum::c_max;

const HLB: usize = 0;
const HRB: usize = 1;
const SLB: usize = 2;
const SRB: usize = 3;

impl Dot<'_> {
    fn make_vn_slot(&mut self, g: G, r: i32, pos: i32) -> N {
        let n = self.rk(g, r).n;
        let b = self.rk(g, r).avbuf;
        {
            let buf = &mut self.bufs[b];
            if buf.len() < (n + 2) as usize {
                buf.resize((n + 2) as usize, None);
            }
        }
        let rk = self.rk_mut(g, r);
        rk.vbuf = b;
        rk.voff = 0;
        let mut i = n;
        while i > pos {
            let v = self.bufs[b][(i - 1) as usize];
            self.bufs[b][i as usize] = v;
            if let Some(vv) = v {
                self.nodes[vv].order += 1;
            }
            i -= 1;
        }
        let vn = self.virtual_node(g);
        self.bufs[b][pos as usize] = Some(vn);
        self.nodes[vn].order = pos;
        self.nodes[vn].rank = r;
        self.rk_mut(g, r).n += 1;
        let nn = self.rk(g, r).n as usize;
        if self.bufs[b].len() <= nn {
            self.bufs[b].resize(nn + 1, None);
        }
        self.bufs[b][nn] = None;
        vn
    }

    fn findlr(&self, u: N, v: N) -> (i32, i32) {
        let l = self.nodes[u].order;
        let r = self.nodes[v].order;
        if l > r { (r, l) } else { (l, r) }
    }

    fn setbounds(&self, v: N, bounds: &mut [i32; 4], lpos: i32, rpos: i32) {
        if self.nodes[v].node_type == VIRTUAL {
            let ord = self.nodes[v].order;
            if self.nodes[v].in_.is_empty() {
                if self.nodes[v].out.len() < 2 {
                    return;
                }
                let (l, r) = self.findlr(
                    self.edges[self.nodes[v].out[0]].head,
                    self.edges[self.nodes[v].out[1]].head,
                );
                if r <= lpos {
                    bounds[SLB] = ord;
                    bounds[HLB] = ord;
                } else if l >= rpos {
                    bounds[SRB] = ord;
                    bounds[HRB] = ord;
                } else if l < lpos && r > rpos {
                    // spanning: ignore
                } else {
                    if l < lpos || (l == lpos && r < rpos) {
                        bounds[SLB] = ord;
                    }
                    if r > rpos || (r == rpos && l > lpos) {
                        bounds[SRB] = ord;
                    }
                }
            } else {
                let mut onleft = false;
                let mut onright = false;
                for &f in &self.nodes[v].out {
                    let o = self.nodes[self.edges[f].head].order;
                    if o <= lpos {
                        onleft = true;
                        continue;
                    }
                    if o >= rpos {
                        onright = true;
                        continue;
                    }
                }
                if onleft && !onright {
                    bounds[HLB] = ord + 1;
                }
                if onright && !onleft {
                    bounds[HRB] = ord - 1;
                }
            }
        }
    }

    fn flat_limits(&self, g: G, e: E) -> i32 {
        let r = self.nodes[self.edges[e].tail].rank - 1;
        let mut lnode = 0;
        let mut rnode = self.rank_n(g, r) - 1;
        let mut bounds = [lnode - 1, rnode + 1, lnode - 1, rnode + 1];
        let (lpos, rpos) = self.findlr(self.edges[e].tail, self.edges[e].head);
        while lnode <= rnode {
            if let Some(v) = self.rv(g, r, lnode) {
                self.setbounds(v, &mut bounds, lpos, rpos);
            }
            if lnode != rnode
                && let Some(v) = self.rv(g, r, rnode)
            {
                self.setbounds(v, &mut bounds, lpos, rpos);
            }
            lnode += 1;
            rnode -= 1;
            if bounds[HRB] - bounds[HLB] <= 1 {
                break;
            }
        }
        if bounds[HLB] <= bounds[HRB] {
            (bounds[HLB] + bounds[HRB] + 1) / 2
        } else {
            (bounds[SLB] + bounds[SRB] + 1) / 2
        }
    }

    fn flat_node(&mut self, e: E) {
        let Some(l) = self.edges[e].label else { return };
        let g = 0;
        let r = self.nodes[self.edges[e].tail].rank;
        let place = self.flat_limits(g, e);
        let ypos = if let Some(n) = self.rv(g, r - 1, 0) {
            self.nodes[n].coord.y - self.rk(g, r - 1).ht1
        } else {
            let n = self.rv(g, r, 0).unwrap();
            self.nodes[n].coord.y + self.rk(g, r).ht2 + self.graphs[g].ranksep as f64
        };
        let vn = self.make_vn_slot(g, r - 1, place);
        let mut dimen = self.labels[l].dimen;
        if self.graphs[g].flip() {
            std::mem::swap(&mut dimen.x, &mut dimen.y);
        }
        self.nodes[vn].ht = dimen.y;
        let h2 = self.nodes[vn].ht / 2.0;
        self.nodes[vn].lw = dimen.x / 2.0;
        self.nodes[vn].rw = dimen.x / 2.0;
        self.nodes[vn].label = Some(l);
        self.nodes[vn].coord.y = ypos + h2;
        let (et, eh) = (self.edges[e].tail, self.edges[e].head);
        let ve = self.virtual_edge(vn, et, Some(e));
        self.edges[ve].tail_port.p.x = -self.nodes[vn].lw;
        self.edges[ve].head_port.p.x = self.nodes[et].rw;
        self.edges[ve].edge_type = FLATORDER;
        let ve = self.virtual_edge(vn, eh, Some(e));
        self.edges[ve].tail_port.p.x = self.nodes[vn].rw;
        self.edges[ve].head_port.p.x = self.nodes[eh].lw;
        self.edges[ve].edge_type = FLATORDER;
        let rk = self.rk_mut(g, r - 1);
        if rk.ht1 < h2 {
            rk.ht1 = h2;
        }
        if rk.ht2 < h2 {
            rk.ht2 = h2;
        }
        self.nodes[vn].alg = Some(e);
    }

    /// `abomination(g)`: a new empty rank above the top rank `GD_minrank(g)`. C asserts
    /// that the top rank is 0 and makes rank -1; see `flat_edges` for the top rank above 0.
    fn abomination(&mut self, g: G) {
        let b = self.new_buf(2);
        let r = self
            .graphs
            .get(g)
            .map_or(-1, |gr| gr.minrank.saturating_sub(1));
        let rk = self.rk_mut(g, r);
        rk.n = 0;
        rk.an = 0;
        rk.vbuf = b;
        rk.avbuf = b;
        rk.voff = 0;
        rk.flat = None;
        rk.ht1 = 1.0;
        rk.ht2 = 1.0;
        rk.pht1 = 1.0;
        rk.pht2 = 1.0;
        self.graphs[g].minrank -= 1;
    }

    fn check_flat_adjacent(&mut self, mut e: E) {
        let (tn, hn) = (self.edges[e].tail, self.edges[e].head);
        let (lo, hi) = if self.nodes[tn].order < self.nodes[hn].order {
            (self.nodes[tn].order, self.nodes[hn].order)
        } else {
            (self.nodes[hn].order, self.nodes[tn].order)
        };
        let r = self.nodes[tn].rank;
        let mut i = lo + 1;
        while i < hi {
            let Some(n) = self.rv(0, r, i) else { break };
            if (self.nodes[n].node_type == VIRTUAL && self.nodes[n].label.is_some())
                || self.nodes[n].node_type == NORMAL
            {
                break;
            }
            i += 1;
        }
        if i == hi {
            loop {
                self.edges[e].adjacent = true;
                match self.edges[e].to_virt {
                    Some(x) => e = x,
                    None => break,
                }
            }
        }
    }

    /// Whether the `ND_other` loop of `flat_edges` will call `flat_node` for an edge
    /// whose tail is `n`: a labelled flat edge, not a loop, whose representative is
    /// not adjacent. Not in C; see the rank -1 test in `flat_edges`.
    fn other_needs_flat_node(&self, n: N) -> bool {
        let rank = |v: N| self.nodes.get(v).map(|nd| nd.rank);
        let Some(nd) = self.nodes.get(n) else {
            return false;
        };
        nd.other.iter().any(|&e| {
            let Some(ed) = self.edges.get(e) else {
                return false;
            };
            if ed.tail == ed.head || ed.label.is_none() || rank(ed.tail) != rank(ed.head) {
                return false;
            }
            let mut le = ed;
            while let Some(next) = le.to_virt.and_then(|x| self.edges.get(x)) {
                le = next;
            }
            !le.adjacent
        })
    }

    /// `flat_edges`: returns true if label nodes were added (y coords must be reset).
    pub fn flat_edges(&mut self, g: G) -> Result<bool, String> {
        let mut reset = false;
        let nl = {
            let mut v = Vec::new();
            let mut n = self.graphs[g].nlist;
            while let Some(x) = n {
                v.push(x);
                n = self.nodes[x].next;
            }
            v
        };
        for &n in &nl {
            for j in 0..self.nodes[n].flat_out.len() {
                let e = self.nodes[n].flat_out[j];
                self.check_flat_adjacent(e);
            }
            for j in 0..self.nodes[n].other.len() {
                let e = self.nodes[n].other[j];
                if self.nodes[self.edges[e].head].rank == self.nodes[self.edges[e].tail].rank {
                    self.check_flat_adjacent(e);
                }
            }
        }
        // C tests rank 0, the top rank once ranking has normalised the ranks. After a
        // failed ranking (`trouble in init_rank`) nothing normalised them, and the top rank
        // is `GD_minrank(g)` > 0. C's ranks above it were never allocated, so `flat_node`
        // on it reads a NULL `GD_rank(g)[r - 1].v` and segfaults (#241); labelled flat
        // edges on lower ranks draw. The port runs the test on the top rank, so it fires
        // only where `flat_node` runs on the top rank. With the top rank at 0 this is C's
        // test. (`minrank < 0` does not get here: `allocate_ranks` indexes by rank.)
        let top = self.graphs.get(g).map_or(0, |gr| gr.minrank.max(0));
        if self.rk(g, top).flat.is_some() || !self.graphs[g].clust.is_empty() {
            let mut found = false;
            let mut i = 0;
            while let Some(n) = self.rv(g, top, i) {
                for &e in &self.nodes[n].flat_in {
                    if self.edges[e].label.is_some() && !self.edges[e].adjacent {
                        found = true;
                        break;
                    }
                }
                // C (`flat.c:279-291`) looks only at `ND_flat_in`, but the `ND_other`
                // loop below also calls `flat_node` for a labelled, non-adjacent flat
                // edge: a parallel `a -> b` merged into an unlabelled representative,
                // or a reversed `b -> a` merged by `flat_rev`. On rank 0 C then reads
                // `GD_rank(g)[-1]` in `flat_node` and segfaults (#230). Make the rank
                // above for those edges too, as C does when the labelled edge is the
                // representative.
                found = found || self.other_needs_flat_node(n);
                if found {
                    self.abomination(g);
                    break;
                }
                i += 1;
            }
        }
        self.rec_save_vlists(g);
        let flip = self.graphs[g].flip();
        for &n in &nl {
            let mut i = 0;
            while i < self.nodes[n].flat_out.len() {
                let e = self.nodes[n].flat_out[i];
                if let Some(l) = self.edges[e].label {
                    if self.edges[e].adjacent {
                        let d = self.labels[l].dimen;
                        self.edges[e].dist = if flip { d.y } else { d.x };
                    } else {
                        reset = true;
                        self.flat_node(e);
                    }
                }
                i += 1;
            }
            let mut j = 0;
            while j < self.nodes[n].other.len() {
                let e = self.nodes[n].other[j];
                j += 1;
                let (et, eh) = (self.edges[e].tail, self.edges[e].head);
                if self.nodes[et].rank != self.nodes[eh].rank {
                    continue;
                }
                if et == eh {
                    continue;
                }
                let mut le = e;
                while let Some(x) = self.edges[le].to_virt {
                    le = x;
                }
                self.edges[e].adjacent = self.edges[le].adjacent;
                if let Some(l) = self.edges[e].label {
                    if self.edges[e].adjacent {
                        let d = self.labels[l].dimen;
                        let lw = if flip { d.y } else { d.x };
                        self.edges[le].dist = c_max(lw, self.edges[le].dist);
                    } else {
                        reset = true;
                        self.flat_node(e);
                    }
                }
            }
        }
        if reset {
            self.check_label_order(g);
            self.rec_reset_vlists(g)?;
        }
        Ok(reset)
    }
}
