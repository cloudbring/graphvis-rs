//! Port of `lib/dotgen/cluster.c`: cluster skeletons, expansion and marking.

use super::Dot;
use super::fastgr::short;
use super::types::*;
use graphviz_pure_core::{AgName, Obj};
use std::collections::VecDeque;

impl<'a> Dot<'a> {
    fn map_interclust_node(&self, n: N) -> N {
        match self.nodes[n].clust {
            None => n,
            Some(c) if self.graphs[c].expanded => n,
            Some(c) => self.graphs[c].rankleader[self.nodes[n].rank as usize].unwrap(),
        }
    }

    /// `make_slots`: make `d` slots starting at `pos` (where one already exists).
    fn make_slots(&mut self, root: G, r: i32, pos: i32, d: i32) {
        let n = self.rk(root, r).n;
        if d <= 0 {
            let mut i = pos - d + 1;
            while i < n {
                let v = self.rv(root, r, i).unwrap();
                self.nodes[v].order = i + d - 1;
                let o = self.nodes[v].order;
                self.set_rv(root, r, o, Some(v));
                i += 1;
            }
            let mut i = n + d - 1;
            while i < n {
                self.set_rv(root, r, i, None);
                i += 1;
            }
        } else {
            let mut i = n - 1;
            while i > pos {
                let v = self.rv(root, r, i).unwrap();
                self.nodes[v].order = i + d - 1;
                let o = self.nodes[v].order;
                self.set_rv(root, r, o, Some(v));
                i -= 1;
            }
            let mut i = pos + 1;
            while i < pos + d {
                self.set_rv(root, r, i, None);
                i += 1;
            }
        }
        self.rk_mut(root, r).n += d - 1;
    }

    fn clone_vn(&mut self, g: G, vn: N) -> N {
        let r = self.nodes[vn].rank;
        let o = self.nodes[vn].order;
        self.make_slots(g, r, o, 2);
        let rv = self.virtual_node(g);
        self.nodes[rv].lw = self.nodes[vn].lw;
        self.nodes[rv].rw = self.nodes[vn].rw;
        self.nodes[rv].rank = self.nodes[vn].rank;
        self.nodes[rv].order = self.nodes[vn].order + 1;
        let ro = self.nodes[rv].order;
        self.set_rv(g, r, ro, Some(rv));
        rv
    }

    fn map_path(&mut self, from: N, to: N, orig: E, ve: Option<E>, ty: u8) {
        let Some(mut ve) = ve else {
            return;
        };
        if self.edges[ve].tail == from && self.edges[ve].head == to {
            return;
        }
        let (fr, tr) = (self.nodes[from].rank, self.nodes[to].rank);
        if self.edges[ve].count > 1 {
            self.edges[orig].to_virt = None;
            if tr - fr == 1
                && let Some(e) = self.find_fast_edge(from, to)
                && self.ports_eq(orig, e)
            {
                self.merge_oneway(orig, e);
                if self.nodes[from].node_type == NORMAL && self.nodes[to].node_type == NORMAL {
                    self.other_edge(orig);
                }
                return;
            }
            let mut u = from;
            let mut r = fr;
            let mut vecur = Some(ve);
            while r < tr {
                let Some(vv) = vecur else { break };
                let v = if r < tr - 1 {
                    let hv = self.edges[vv].head;
                    self.clone_vn(0, hv)
                } else {
                    to
                };
                let e = self.virtual_edge(u, v, Some(orig));
                self.edges[e].edge_type = ty;
                u = v;
                self.edges[vv].count = short(self.edges[vv].count - 1);
                let hv = self.edges[vv].head;
                vecur = self.nodes[hv].out.first().copied();
                r += 1;
            }
        } else {
            if tr - fr == 1 {
                let fe = self.find_fast_edge(from, to);
                if let Some(x) = fe
                    && self.ports_eq(orig, x)
                {
                    ve = x;
                    self.edges[orig].to_virt = Some(ve);
                    self.edges[ve].edge_type = ty;
                    self.edges[ve].count = short(self.edges[ve].count + 1);
                    if self.nodes[from].node_type == NORMAL && self.nodes[to].node_type == NORMAL {
                        self.other_edge(orig);
                    }
                } else {
                    self.edges[orig].to_virt = None;
                    ve = self.virtual_edge(from, to, Some(orig));
                    self.edges[ve].edge_type = ty;
                }
            }
            if tr - fr > 1 {
                let mut e;
                if self.edges[ve].tail != from {
                    self.edges[orig].to_virt = None;
                    let hv = self.edges[ve].head;
                    e = self.virtual_edge(from, hv, Some(orig));
                    self.edges[orig].to_virt = Some(e);
                    self.delete_fast_edge(ve);
                } else {
                    e = ve;
                }
                while self.nodes[self.edges[e].head].rank != tr {
                    let hd = self.edges[e].head;
                    e = self.nodes[hd].out[0];
                }
                if self.edges[e].head != to {
                    let ve2 = e;
                    let t = self.edges[e].tail;
                    e = self.virtual_edge(t, to, Some(orig));
                    self.edges[e].edge_type = ty;
                    self.delete_fast_edge(ve2);
                }
            }
        }
    }

    fn make_interclust_chain(&mut self, from: N, to: N, orig: E) {
        let u = self.map_interclust_node(from);
        let v = self.map_interclust_node(to);
        let newtype = if u == from && v == to {
            VIRTUAL
        } else {
            CLUSTER_EDGE
        };
        let tv = self.edges[orig].to_virt;
        self.map_path(u, v, orig, tv, newtype);
    }

    fn interclexp(&mut self, subg: G) {
        let g = 0;
        for n in self.nodes_of(subg) {
            let mut prev: Option<E> = None;
            for e in self.all_edges(g, n) {
                if self.agcontains_edge(subg, e) {
                    continue;
                }
                let (et, eh) = (self.edges[e].tail, self.edges[e].head);
                if self.mergeable(prev, Some(e)) {
                    let p = prev.unwrap();
                    if self.nodes[et].rank == self.nodes[eh].rank {
                        self.edges[e].to_virt = Some(p);
                    } else {
                        self.edges[e].to_virt = None;
                    }
                    let Some(pv) = self.edges[p].to_virt else {
                        continue;
                    };
                    self.edges[e].to_virt = None;
                    self.merge_chain(subg, e, pv, false);
                    self.safe_other_edge(e);
                    continue;
                }
                if self.nodes[et].rank == self.nodes[eh].rank {
                    match self.find_flat_edge(et, eh) {
                        None => {
                            if self.edges[e].to_virt.is_none() {
                                self.flat_edge(g, e);
                            }
                            prev = Some(e);
                        }
                        Some(fe) if fe != e => {
                            self.safe_other_edge(e);
                            if self.edges[e].to_virt.is_none() {
                                self.merge_oneway(e, fe);
                            }
                        }
                        _ => {}
                    }
                    continue;
                }
                if self.nodes[eh].rank > self.nodes[et].rank {
                    self.make_interclust_chain(et, eh, e);
                    prev = Some(e);
                    continue;
                }
                self.make_interclust_chain(eh, et, e);
                prev = Some(e);
            }
        }
    }

    fn merge_ranks(&mut self, subg: G) {
        let root = 0;
        let (minr, maxr) = (self.graphs[subg].minrank, self.graphs[subg].maxrank);
        if minr > 0 {
            self.rk_mut(root, minr - 1).valid = false;
        }
        let mut r = minr;
        while r <= maxr {
            let d = self.rk(subg, r).n;
            let leader = self.graphs[subg].rankleader[r as usize].unwrap();
            let ipos = self.nodes[leader].order;
            let mut pos = ipos;
            self.make_slots(root, r, pos, d);
            for i in 0..self.rk(subg, r).n {
                let v = self.rv(subg, r, i).unwrap();
                self.set_rv(root, r, pos, Some(v));
                self.nodes[v].order = pos;
                pos += 1;
                self.delete_fast_node(subg, v);
                self.fast_node(root, v);
            }
            let (rb, ro) = (self.rk(root, r).vbuf, self.rk(root, r).voff);
            let sr = self.rk_mut(subg, r);
            sr.vbuf = rb;
            sr.voff = ro + ipos as usize;
            self.rk_mut(root, r).valid = false;
            r += 1;
        }
        if r < self.graphs[root].maxrank {
            self.rk_mut(root, r).valid = false;
        }
        self.graphs[subg].expanded = true;
    }

    fn remove_rankleaders(&mut self, g: G) {
        let (minr, maxr) = (self.graphs[g].minrank, self.graphs[g].maxrank);
        for r in minr..=maxr {
            let v = self.graphs[g].rankleader[r as usize].unwrap();
            while let Some(&e) = self.nodes[v].out.first() {
                self.delete_fast_edge(e);
            }
            while let Some(&e) = self.nodes[v].in_.first() {
                self.delete_fast_edge(e);
            }
            self.delete_fast_node(0, v);
            self.graphs[g].rankleader[r as usize] = None;
        }
    }

    /// `expand_cluster`: install the cluster contents in place of its skeleton.
    pub fn expand_cluster(&mut self, subg: G) -> Result<(), String> {
        self.class2(subg)?;
        // C's `build_ranks` starts with `for (ns = GD_nlist(g); ND_next(ns); ...)`
        // (mincross.c:1227) and segfaults on a cluster with an empty fast-node list. That is
        // a cluster whose nodes were all taken, after it was registered, by a rank set or an
        // earlier cluster: `mark_clusters` at its parent's level, or newrank's `set_parent`
        // before `node_induce` (issue #231). C removes such clusters only among the root's
        // (the check in `dot_mincross`); a nested one keeps the rank range it had, its rank
        // leaders are installed in its parent's ranks, and that can fail first with
        // `install_in_rank`'s own error (line 1158), as in C. The port fails the layout
        // where C crashes.
        let nl = self.graphs[subg].nlist;
        if nl.is_none() {
            return Err(format!(
                "expand_cluster: cluster {} has no nodes left (a rank set or an earlier cluster \
                 took them)",
                self.graph_name(subg)
            ));
        }
        self.graphs[subg].comp = vec![nl];
        self.allocate_ranks(subg);
        self.build_ranks(subg, 0)?;
        self.merge_ranks(subg);
        self.interclexp(subg);
        self.remove_rankleaders(subg);
        Ok(())
    }

    /// `agnameof(g)`: a pack component is `_cc_<i>`; the root and the input's subgraphs are
    /// named by [`graphviz_pure_core::Graph::agnameof`]. A projection pack.rs made of a subgraph (`projectG`'s
    /// `agsubg(g, agnameof(subg), 1)`) keeps that name as a string of its own.
    pub(super) fn graph_agnameof(&self, g: G) -> AgName<'a> {
        if g == 0 && self.comp {
            return AgName::own(format!("_cc_{}", self.comp_index));
        }
        let graph = &self.graphs[g];
        match (g, graph.sub_index, graph.proj_of) {
            (0, ..) => self.input.agnameof(Obj::Root),
            (_, Some(si), _) if si < self.input.subgraphs.len() => {
                self.input.agnameof(Obj::Subgraph(si))
            }
            (_, None, Some(o)) if o < self.input.subgraphs.len() => {
                AgName::own(self.input.subgraph_name(o))
            }
            _ => AgName::own(graph.name.clone()),
        }
    }

    /// `agnameof(g)` as a string.
    pub(super) fn graph_name(&self, g: G) -> String {
        self.graph_agnameof(g).name.into_owned()
    }

    /// `agnameof(n)`: a real node is the input's; a virtual node, a zeroed `Agobj_t` with id 0,
    /// is `%0` in agnameof's static buffer.
    pub(super) fn node_agnameof(&self, n: N) -> AgName<'a> {
        if self.nodes[n].node_type == NORMAL && n < self.input.nodes.len() {
            self.input
                .agnameof(Obj::Node(graphviz_pure_core::NodeId(n as u32)))
        } else {
            AgName::buffer("%0")
        }
    }

    /// `mark_clusters`: marks every node in `g` with its top-level cluster under `g`.
    pub fn mark_clusters(&mut self, g: G) {
        for n in self.nodes_of(g) {
            if self.nodes[n].ranktype == CLUSTER {
                self.uf_singleton(n);
            }
            self.nodes[n].clust = None;
        }
        for clust in self.graphs[g].clust.clone() {
            for n in self.nodes_of(clust) {
                if self.nodes[n].ranktype != NORMAL {
                    // cluster.c:318-320: both names share agnameof's static buffer when
                    // anonymous.
                    let [node, graph] = graphviz_pure_core::agnameof_buffer([
                        self.node_agnameof(n),
                        self.graph_agnameof(g),
                    ]);
                    graphviz_pure_core::diag::agwarningf(&format!(
                        "{node} was already in a rankset, deleted from cluster {graph}\n"
                    ));
                    self.agdelnode(clust, n);
                    continue;
                }
                let leader = self.graphs[clust].leader.unwrap();
                self.uf_setname(n, leader);
                self.nodes[n].clust = Some(clust);
                self.nodes[n].ranktype = CLUSTER;
                for orig in self.out_edges(clust, n) {
                    let mut e = self.edges[orig].to_virt;
                    while let Some(x) = e {
                        let vn = self.edges[x].head;
                        if self.nodes[vn].node_type != VIRTUAL {
                            break;
                        }
                        self.nodes[vn].clust = Some(clust);
                        e = self.nodes[vn].out.first().copied();
                    }
                }
            }
        }
    }

    /// `build_skeleton` (cluster.c). C indexes `GD_rankleader(subg)[ND_rank(v)]` for every
    /// node of `subg` and dereferences the result; a node whose rank lies outside the
    /// cluster's `minrank..=maxrank` has no leader and C crashes on the NULL pointer (issue
    /// #10: a cluster node also in a top-level `rank=max` set, without `pack`). It also reads
    /// the leader's first out-edge, which the maxrank leader lacks. The port fails the layout
    /// in both cases instead of panicking.
    pub fn build_skeleton(&mut self, g: G, subg: G) -> Result<(), String> {
        let (minr, maxr) = (self.graphs[subg].minrank, self.graphs[subg].maxrank);
        self.graphs[subg].rankleader = vec![None; (maxr + 2).max(0) as usize];
        let mut prev: Option<N> = None;
        for r in minr..=maxr {
            let v = self.virtual_node(g);
            self.graphs[subg].rankleader[r as usize] = Some(v);
            self.nodes[v].rank = r;
            self.nodes[v].ranktype = CLUSTER;
            self.nodes[v].clust = Some(subg);
            if let Some(p) = prev {
                let e = self.virtual_edge(p, v, None);
                self.edges[e].xpenalty = short(self.edges[e].xpenalty * CL_CROSS);
            }
            prev = Some(v);
        }
        for v in self.nodes_of(subg) {
            let rank = self.nodes[v].rank;
            let Some(rl) = usize::try_from(rank)
                .ok()
                .and_then(|r| self.graphs[subg].rankleader.get(r).copied().flatten())
            else {
                return Err(format!(
                    "build_skeleton: no rank leader for rank {rank} of node {} in cluster {}",
                    self.nodes[v].name, self.graphs[subg].name
                ));
            };
            self.nodes[rl].uf_size += 1;
            for e in self.out_edges(subg, v) {
                let (tr, hr) = (
                    self.nodes[self.edges[e].tail].rank,
                    self.nodes[self.edges[e].head].rank,
                );
                let mut r = tr;
                while r < hr {
                    // C reads `ND_out(rl).list[0]`, NULL for the maxrank leader.
                    let Some(&oe) = self.nodes[rl].out.first() else {
                        return Err(format!(
                            "build_skeleton: no skeleton edge below rank {rank} of node {} in \
                             cluster {}",
                            self.nodes[v].name, self.graphs[subg].name
                        ));
                    };
                    self.edges[oe].count = short(self.edges[oe].count + 1);
                    r += 1;
                }
            }
        }
        for r in minr..=maxr {
            let rl = self.graphs[subg].rankleader[r as usize].unwrap();
            if self.nodes[rl].uf_size > 1 {
                self.nodes[rl].uf_size -= 1;
            }
        }
        Ok(())
    }

    pub fn install_cluster(
        &mut self,
        g: G,
        n: N,
        pass: i32,
        q: &mut VecDeque<N>,
    ) -> Result<(), String> {
        let clust = self.nodes[n].clust.unwrap();
        if self.graphs[clust].installed != pass + 1 {
            let (minr, maxr) = (self.graphs[clust].minrank, self.graphs[clust].maxrank);
            for r in minr..=maxr {
                let rl = self.graphs[clust].rankleader[r as usize].unwrap();
                self.install_in_rank(g, rl)?;
            }
            for r in minr..=maxr {
                let rl = self.graphs[clust].rankleader[r as usize].unwrap();
                self.enqueue_neighbors(q, rl, pass);
            }
            self.graphs[clust].installed = pass + 1;
        }
        Ok(())
    }

    pub fn mark_lowclusters(&mut self, root: G) {
        for n in self.nodes_of(root) {
            self.nodes[n].clust = None;
            for orig in self.out_edges(root, n) {
                let mut e = self.edges[orig].to_virt;
                while let Some(x) = e {
                    let vn = self.edges[x].head;
                    if self.nodes[vn].node_type != VIRTUAL {
                        break;
                    }
                    self.nodes[vn].clust = None;
                    e = self.nodes[vn].out.first().copied();
                }
            }
        }
        self.mark_lowcluster_basic(root);
    }

    fn mark_lowcluster_basic(&mut self, g: G) {
        for clust in self.graphs[g].clust.clone() {
            self.mark_lowcluster_basic(clust);
        }
        for n in self.nodes_of(g) {
            if self.nodes[n].clust.is_none() {
                self.nodes[n].clust = Some(g);
            }
            for orig in self.out_edges(g, n) {
                let mut e = self.edges[orig].to_virt;
                while let Some(x) = e {
                    let vn = self.edges[x].head;
                    if self.nodes[vn].node_type != VIRTUAL {
                        break;
                    }
                    if self.nodes[vn].clust.is_none() {
                        self.nodes[vn].clust = Some(g);
                    }
                    e = self.nodes[vn].out.first().copied();
                }
            }
        }
    }
}
