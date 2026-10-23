//! Port of `lib/dotgen/conc.c`: edge concentrators (`concentrate=true`).
//! Parallel virtual node chains with a common endpoint are merged rank by
//! rank (`mergevirtual`), then the cluster rank windows are rebuilt
//! (`rebuild_vlists`).

use super::Dot;
use super::dotsplines::portcmp;
use super::types::*;

const UP: i32 = 0;
const DOWN: i32 = 1;

impl Dot<'_> {
    /// `samedir(e, f)`.
    fn samedir(&self, e: E, f: E) -> bool {
        let walk = |mut x: Option<E>| {
            while let Some(y) = x {
                if self.edges[y].edge_type == NORMAL {
                    break;
                }
                x = self.edges[y].to_orig;
            }
            x
        };
        let (Some(e0), Some(f0)) = (walk(Some(e)), walk(Some(f))) else {
            return false;
        };
        if self.edges[e0].conc_opp_flag || self.edges[f0].conc_opp_flag {
            return false;
        }
        let rd = |x: E| self.nodes[self.edges[x].tail].rank - self.nodes[self.edges[x].head].rank;
        rd(f0) * rd(e0) > 0
    }

    /// `downcandidate(v)`.
    fn downcandidate(&self, v: N) -> bool {
        let nd = &self.nodes[v];
        nd.node_type == VIRTUAL && nd.in_.len() == 1 && nd.out.len() == 1 && nd.label.is_none()
    }

    /// `bothdowncandidates(u, v)`.
    fn bothdowncandidates(&self, u: N, v: N) -> bool {
        let e = self.nodes[u].in_[0];
        let Some(&f) = self.nodes[v].in_.first() else {
            return false;
        };
        if self.downcandidate(v) && self.edges[e].tail == self.edges[f].tail {
            return self.samedir(e, f)
                && portcmp(&self.edges[e].tail_port, &self.edges[f].tail_port) == 0;
        }
        false
    }

    /// `upcandidate(v)`.
    fn upcandidate(&self, v: N) -> bool {
        let nd = &self.nodes[v];
        nd.node_type == VIRTUAL && nd.out.len() == 1 && nd.in_.len() == 1 && nd.label.is_none()
    }

    /// `bothupcandidates(u, v)`.
    fn bothupcandidates(&self, u: N, v: N) -> bool {
        let e = self.nodes[u].out[0];
        let Some(&f) = self.nodes[v].out.first() else {
            return false;
        };
        if self.upcandidate(v) && self.edges[e].head == self.edges[f].head {
            return self.samedir(e, f)
                && portcmp(&self.edges[e].head_port, &self.edges[f].head_port) == 0;
        }
        false
    }

    /// `mergevirtual(g, r, lpos, rpos, dir)`.
    fn mergevirtual(&mut self, g: G, r: i32, lpos: i32, rpos: i32, dir: i32) {
        let left = self.rv(g, r, lpos).unwrap();
        for i in lpos + 1..=rpos {
            let right = self.rv(g, r, i).unwrap();
            if dir == DOWN {
                while let Some(&e) = self.nodes[right].out.first() {
                    let eh = self.edges[e].head;
                    let f = match self.nodes[left]
                        .out
                        .iter()
                        .copied()
                        .find(|&f| self.edges[f].head == eh)
                    {
                        Some(f) => f,
                        None => self.virtual_edge(left, eh, Some(e)),
                    };
                    while let Some(&e0) = self.nodes[right].in_.first() {
                        self.merge_oneway(e0, f);
                        self.delete_fast_edge(e0);
                    }
                    self.delete_fast_edge(e);
                }
            } else {
                while let Some(&e) = self.nodes[right].in_.first() {
                    let et = self.edges[e].tail;
                    let f = match self.nodes[left]
                        .in_
                        .iter()
                        .copied()
                        .find(|&f| self.edges[f].tail == et)
                    {
                        Some(f) => f,
                        None => self.virtual_edge(et, left, Some(e)),
                    };
                    while let Some(&e0) = self.nodes[right].out.first() {
                        self.merge_oneway(e0, f);
                        self.delete_fast_edge(e0);
                    }
                    self.delete_fast_edge(e);
                }
            }
            debug_assert!(self.nodes[right].in_.is_empty() && self.nodes[right].out.is_empty());
            self.delete_fast_node(g, right);
            if let Some(nd) = self.nodes.get_mut(right) {
                nd.conc_merged_into = Some(left);
            }
        }
        let mut k = lpos + 1;
        let n = self.rank_n(g, r);
        for i in rpos + 1..n {
            let v = self.rv(g, r, i);
            self.set_rv(g, r, k, v);
            if let Some(v) = v {
                self.nodes[v].order = k;
            }
            k += 1;
        }
        self.rk_mut(g, r).n = k;
        self.set_rv(g, r, k, None);
    }

    /// `infuse(g, n)`.
    fn infuse(&mut self, g: G, n: N) {
        let r = self.nodes[n].rank as usize;
        let lead = self.graphs[g].rankleader[r];
        if lead.is_none_or(|l| self.nodes[l].order > self.nodes[n].order) {
            self.graphs[g].rankleader[r] = Some(n);
        }
    }

    /// `rebuild_vlists(g)`.
    fn rebuild_vlists(&mut self, g: G) -> Result<(), String> {
        let (minr, maxr) = (self.graphs[g].minrank, self.graphs[g].maxrank);
        if self.graphs[g].rankleader.len() < (maxr + 2).max(0) as usize {
            self.graphs[g].rankleader.resize((maxr + 2) as usize, None);
        }
        for r in minr..=maxr {
            self.graphs[g].rankleader[r as usize] = None;
        }
        self.dot_scan_ranks(g);
        for n in self.nodes_of(g) {
            self.infuse(g, n);
            for e in self.out_edges(g, n) {
                let mut rep = e;
                while let Some(v) = self.edges[rep].to_virt {
                    rep = v;
                }
                let eh_rank = self.nodes[self.edges[e].head].rank;
                let mut rep = Some(rep);
                while let Some(x) = rep {
                    let h = self.edges[x].head;
                    if self.nodes[h].rank >= eh_rank {
                        break;
                    }
                    self.infuse(g, h);
                    rep = self.nodes[h].out.first().copied();
                }
            }
        }
        for r in self.graphs[g].minrank..=self.graphs[g].maxrank {
            let Some(lead) = self.graphs[g].rankleader[r as usize] else {
                return Err(format!("rebuild_vlists: lead is null for rank {r}"));
            };
            let order = self.nodes[lead].order;
            if self.rv(0, r, order) != Some(lead) {
                return Err(format!(
                    "rebuild_vlists: rank lead {} not in order {order} of rank {r}",
                    self.nodes[lead].name
                ));
            }
            let (vb, vo) = {
                let rk = self.rk(0, r);
                (rk.vbuf, rk.voff)
            };
            {
                let rk = self.rk_mut(g, r);
                rk.vbuf = vb;
                rk.voff = vo + order as usize;
            }
            let mut maxi = -1;
            let n = self.rank_n(g, r);
            for i in 0..n {
                let Some(v) = self.rv(g, r, i) else { break };
                if self.nodes[v].node_type == NORMAL {
                    if self.agcontains_node(g, v) {
                        maxi = i;
                    } else {
                        break;
                    }
                } else {
                    let mut e = self.nodes[v].in_.first().copied();
                    while let Some(x) = e {
                        match self.edges[x].to_orig {
                            Some(o) => e = Some(o),
                            None => break,
                        }
                    }
                    if let Some(x) = e
                        && self.agcontains_node(g, self.edges[x].tail)
                        && self.agcontains_node(g, self.edges[x].head)
                    {
                        maxi = i;
                    }
                }
            }
            if maxi == -1 {
                graphviz_pure_core::diag::agwarningf(&format!(
                    "degenerate concentrated rank {},{r}\n",
                    self.graph_name(g)
                ));
            }
            self.rk_mut(g, r).n = maxi + 1;
        }
        for c in self.graphs[g].clust.clone() {
            self.rebuild_vlists(c)?;
        }
        Ok(())
    }

    /// `dot_concentrate(g)`.
    pub fn dot_concentrate(&mut self) -> Result<(), String> {
        let g = 0;
        if self.graphs[g].maxrank - self.graphs[g].minrank <= 1 {
            return Ok(());
        }
        // downward looking pass; r is a candidate rank
        let mut r = 1;
        while self.rank_n(g, r + 1) != 0 {
            let mut leftpos = 0;
            while leftpos < self.rank_n(g, r) {
                let left = self.rv(g, r, leftpos).unwrap();
                if self.downcandidate(left) {
                    let mut rightpos = leftpos + 1;
                    while rightpos < self.rank_n(g, r) {
                        let right = self.rv(g, r, rightpos).unwrap();
                        if !self.bothdowncandidates(left, right) {
                            break;
                        }
                        rightpos += 1;
                    }
                    if rightpos - leftpos > 1 {
                        self.mergevirtual(g, r, leftpos, rightpos - 1, DOWN);
                    }
                }
                leftpos += 1;
            }
            r += 1;
        }
        // corresponding upward pass
        while r > 0 {
            let mut leftpos = 0;
            while leftpos < self.rank_n(g, r) {
                let left = self.rv(g, r, leftpos).unwrap();
                if self.upcandidate(left) {
                    let mut rightpos = leftpos + 1;
                    while rightpos < self.rank_n(g, r) {
                        let right = self.rv(g, r, rightpos).unwrap();
                        if !self.bothupcandidates(left, right) {
                            break;
                        }
                        rightpos += 1;
                    }
                    if rightpos - leftpos > 1 {
                        self.mergevirtual(g, r, leftpos, rightpos - 1, UP);
                    }
                }
                leftpos += 1;
            }
            r -= 1;
        }
        for c in self.graphs[g].clust.clone() {
            self.rebuild_vlists(c)?;
        }
        Ok(())
    }
}
