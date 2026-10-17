//! Port of `lib/common/ns.c`: network simplex ranking (`rank`, `rank2`).
//!
//! Operates on the fast graph rooted at `GD_nlist(g)` using `ND_in`/`ND_out`,
//! `ND_rank`, `ED_minlen` and `ED_weight`, exactly like the C code: same
//! search order, same tie-breaking, same balancing modes. The recursive C
//! helpers (`rerank`, `tree_adjust`) are iterative here; they only add a
//! constant to every node of a subtree, so visiting order is irrelevant.

use super::Dot;
use super::qsort::qsort;
use super::types::*;
use super::utils::atoi;

const SEARCHSIZE: i32 = 30;

struct Subtree {
    rep: N,
    size: i32,
    heap_index: usize,
    par: usize,
}

const NOT_ON_HEAP: usize = usize::MAX;

struct Ctx {
    g: G,
    tree_edge: Vec<E>,
    s_i: usize,
    n_nodes: usize,
    search_size: i32,
    subtrees: Vec<Subtree>,
}

impl Dot<'_> {
    #[inline]
    fn slack(&self, e: E) -> i32 {
        let ed = &self.edges[e];
        self.nodes[ed.head].rank - self.nodes[ed.tail].rank - ed.minlen
    }

    #[inline]
    fn tree_edge(&self, e: E) -> bool {
        self.edges[e].tree_index >= 0
    }

    fn nlist_vec(&self, g: G) -> Vec<N> {
        let mut v = Vec::new();
        let mut n = self.graphs[g].nlist;
        while let Some(x) = n {
            v.push(x);
            n = self.nodes[x].next;
        }
        v
    }

    fn add_tree_edge(&mut self, ctx: &mut Ctx, e: E) -> i32 {
        if self.tree_edge(e) {
            return -1;
        }
        self.edges[e].tree_index = ctx.tree_edge.len() as i32;
        ctx.tree_edge.push(e);
        let t = self.edges[e].tail;
        self.nodes[t].mark = 1;
        self.nodes[t].tree_out.push(e);
        if self.nodes[t].out.len() < self.nodes[t].tree_out.len() {
            return -1;
        }
        let h = self.edges[e].head;
        self.nodes[h].mark = 1;
        self.nodes[h].tree_in.push(e);
        if self.nodes[h].in_.len() < self.nodes[h].tree_in.len() {
            return -1;
        }
        0
    }

    fn invalidate_path(&mut self, lca: N, mut to_node: N) {
        loop {
            if self.nodes[to_node].low == -1 {
                break;
            }
            self.nodes[to_node].low = -1;
            let Some(e) = self.nodes[to_node].par else {
                break;
            };
            if self.nodes[to_node].lim >= self.nodes[lca].lim {
                break;
            }
            let (t, h) = (self.edges[e].tail, self.edges[e].head);
            to_node = if self.nodes[t].lim > self.nodes[h].lim {
                t
            } else {
                h
            };
        }
    }

    fn exchange_tree_edges(&mut self, ctx: &mut Ctx, e: E, f: E) {
        let ti = self.edges[e].tree_index;
        self.edges[f].tree_index = ti;
        ctx.tree_edge[ti as usize] = f;
        self.edges[e].tree_index = -1;
        let t = self.edges[e].tail;
        super::fastgr::zapinlist(&mut self.nodes[t].tree_out, e);
        let h = self.edges[e].head;
        super::fastgr::zapinlist(&mut self.nodes[h].tree_in, e);
        let t = self.edges[f].tail;
        self.nodes[t].tree_out.push(f);
        let h = self.edges[f].head;
        self.nodes[h].tree_in.push(f);
    }

    fn init_rank(&mut self, ctx: &Ctx) {
        let mut q = std::collections::VecDeque::new();
        for v in self.nlist_vec(ctx.g) {
            if self.nodes[v].priority == 0 {
                q.push_back(v);
            }
        }
        while let Some(v) = q.pop_front() {
            self.nodes[v].rank = 0;
            for i in 0..self.nodes[v].in_.len() {
                let e = self.nodes[v].in_[i];
                let r = self.nodes[self.edges[e].tail].rank + self.edges[e].minlen;
                if r > self.nodes[v].rank {
                    self.nodes[v].rank = r;
                }
            }
            for i in 0..self.nodes[v].out.len() {
                let e = self.nodes[v].out[i];
                let h = self.edges[e].head;
                self.nodes[h].priority -= 1;
                if self.nodes[h].priority <= 0 {
                    q.push_back(h);
                }
            }
        }
    }

    fn leave_edge(&self, ctx: &mut Ctx) -> Option<E> {
        let mut rv: Option<E> = None;
        let mut cnt = 0;
        let j = ctx.s_i;
        while ctx.s_i < ctx.tree_edge.len() {
            let f = ctx.tree_edge[ctx.s_i];
            if self.edges[f].cutvalue < 0 {
                match rv {
                    Some(r) => {
                        if self.edges[r].cutvalue > self.edges[f].cutvalue {
                            rv = Some(f);
                        }
                    }
                    None => rv = Some(f),
                }
                cnt += 1;
                if cnt >= ctx.search_size {
                    return rv;
                }
            }
            ctx.s_i += 1;
        }
        if j > 0 {
            ctx.s_i = 0;
            while ctx.s_i < j {
                let f = ctx.tree_edge[ctx.s_i];
                if self.edges[f].cutvalue < 0 {
                    match rv {
                        Some(r) => {
                            if self.edges[r].cutvalue > self.edges[f].cutvalue {
                                rv = Some(f);
                            }
                        }
                        None => rv = Some(f),
                    }
                    cnt += 1;
                    if cnt >= ctx.search_size {
                        return rv;
                    }
                }
                ctx.s_i += 1;
            }
        }
        rv
    }

    fn dfs_enter_outedge(&self, v: N, low: i32, lim: i32) -> Option<E> {
        let mut enter: Option<E> = None;
        let mut slack = i32::MAX;
        let mut todo = vec![v];
        while let Some(v) = todo.pop() {
            for &e in &self.nodes[v].out {
                let h = self.edges[e].head;
                if !self.tree_edge(e) {
                    let hl = self.nodes[h].lim;
                    if !(low <= hl && hl <= lim) {
                        let s = self.slack(e);
                        if s < slack || enter.is_none() {
                            enter = Some(e);
                            slack = s;
                        }
                    }
                } else if self.nodes[h].lim < self.nodes[v].lim {
                    todo.push(h);
                }
            }
            for &e in &self.nodes[v].tree_in {
                if slack <= 0 {
                    break;
                }
                let t = self.edges[e].tail;
                if self.nodes[t].lim < self.nodes[v].lim {
                    todo.push(t);
                }
            }
        }
        enter
    }

    fn dfs_enter_inedge(&self, v: N, low: i32, lim: i32) -> Option<E> {
        let mut enter: Option<E> = None;
        let mut slack = i32::MAX;
        let mut todo = vec![v];
        while let Some(v) = todo.pop() {
            for &e in &self.nodes[v].in_ {
                let t = self.edges[e].tail;
                if !self.tree_edge(e) {
                    let tl = self.nodes[t].lim;
                    if !(low <= tl && tl <= lim) {
                        let s = self.slack(e);
                        if s < slack || enter.is_none() {
                            enter = Some(e);
                            slack = s;
                        }
                    }
                } else if self.nodes[t].lim < self.nodes[v].lim {
                    todo.push(t);
                }
            }
            for &e in &self.nodes[v].tree_out {
                if slack <= 0 {
                    break;
                }
                let h = self.edges[e].head;
                if self.nodes[h].lim < self.nodes[v].lim {
                    todo.push(h);
                }
            }
        }
        enter
    }

    fn enter_edge(&self, e: E) -> Option<E> {
        let (t, h) = (self.edges[e].tail, self.edges[e].head);
        if self.nodes[t].lim < self.nodes[h].lim {
            self.dfs_enter_inedge(t, self.nodes[t].low, self.nodes[t].lim)
        } else {
            self.dfs_enter_outedge(h, self.nodes[h].low, self.nodes[h].lim)
        }
    }

    fn init_cutvalues(&mut self, ctx: &Ctx) {
        let first = self.graphs[ctx.g].nlist.unwrap();
        self.dfs_range_init(first);
        self.dfs_cutval(first, None);
    }

    fn tight_subtree_search(&mut self, ctx: &mut Ctx, v: N, st: usize) -> i32 {
        struct Tst {
            v: N,
            in_i: usize,
            out_i: usize,
            rv: i32,
        }
        let mut rv = 1;
        self.nodes[v].subtree = Some(st);
        let mut todo = vec![Tst {
            v,
            in_i: 0,
            out_i: 0,
            rv: 1,
        }];
        while !todo.is_empty() {
            let mut updated = false;
            let top = todo.len() - 1;
            let tv = todo[top].v;
            while todo[top].in_i < self.nodes[tv].in_.len() {
                let e = self.nodes[tv].in_[todo[top].in_i];
                if self.tree_edge(e) {
                    todo[top].in_i += 1;
                    continue;
                }
                let t = self.edges[e].tail;
                if self.nodes[t].subtree.is_none() && self.slack(e) == 0 {
                    if self.add_tree_edge(ctx, e) != 0 {
                        todo.pop();
                        if todo.is_empty() {
                            rv = -1;
                        } else {
                            todo.last_mut().unwrap().rv -= 1;
                        }
                    } else {
                        todo[top].in_i += 1;
                        self.nodes[t].subtree = Some(st);
                        todo.push(Tst {
                            v: t,
                            in_i: 0,
                            out_i: 0,
                            rv: 1,
                        });
                    }
                    updated = true;
                    break;
                }
                todo[top].in_i += 1;
            }
            if updated {
                continue;
            }
            while todo[top].out_i < self.nodes[tv].out.len() {
                let e = self.nodes[tv].out[todo[top].out_i];
                if self.tree_edge(e) {
                    todo[top].out_i += 1;
                    continue;
                }
                let h = self.edges[e].head;
                if self.nodes[h].subtree.is_none() && self.slack(e) == 0 {
                    if self.add_tree_edge(ctx, e) != 0 {
                        todo.pop();
                        if todo.is_empty() {
                            rv = -1;
                        } else {
                            todo.last_mut().unwrap().rv -= 1;
                        }
                    } else {
                        todo[top].out_i += 1;
                        self.nodes[h].subtree = Some(st);
                        todo.push(Tst {
                            v: h,
                            in_i: 0,
                            out_i: 0,
                            rv: 1,
                        });
                    }
                    updated = true;
                    break;
                }
                todo[top].out_i += 1;
            }
            if updated {
                continue;
            }
            let last = todo.pop().unwrap();
            if todo.is_empty() {
                rv = last.rv;
            } else {
                todo.last_mut().unwrap().rv += last.rv;
            }
        }
        rv
    }

    fn find_tight_subtree(&mut self, ctx: &mut Ctx, v: N) -> Option<usize> {
        let st = ctx.subtrees.len();
        ctx.subtrees.push(Subtree {
            rep: v,
            size: 0,
            heap_index: NOT_ON_HEAP,
            par: st,
        });
        let size = self.tight_subtree_search(ctx, v, st);
        if size < 0 {
            return None;
        }
        ctx.subtrees[st].size = size;
        ctx.subtrees[st].par = st;
        Some(st)
    }

    fn st_set_find(&self, ctx: &mut Ctx, n0: N) -> usize {
        let mut s0 = self.nodes[n0].subtree.unwrap();
        while ctx.subtrees[s0].par != s0 {
            let p = ctx.subtrees[s0].par;
            let pp = ctx.subtrees[p].par;
            ctx.subtrees[s0].par = pp;
            s0 = ctx.subtrees[s0].par;
        }
        s0
    }

    fn st_set_union(ctx: &mut Ctx, s0: usize, s1: usize) -> usize {
        let mut r0 = s0;
        while ctx.subtrees[r0].par != r0 {
            r0 = ctx.subtrees[r0].par;
        }
        let mut r1 = s1;
        while ctx.subtrees[r1].par != r1 {
            r1 = ctx.subtrees[r1].par;
        }
        if r0 == r1 {
            return r0;
        }
        let on0 = ctx.subtrees[r0].heap_index != NOT_ON_HEAP;
        let on1 = ctx.subtrees[r1].heap_index != NOT_ON_HEAP;
        let r = if !on1 {
            r0
        } else if !on0 {
            r1
        } else if ctx.subtrees[r1].size < ctx.subtrees[r0].size {
            r0
        } else {
            r1
        };
        ctx.subtrees[r0].par = r;
        ctx.subtrees[r1].par = r;
        ctx.subtrees[r].size = ctx.subtrees[r0].size + ctx.subtrees[r1].size;
        r
    }

    fn inter_tree_edge_search(&self, ctx: &mut Ctx, v: N) -> Option<E> {
        struct St {
            v: N,
            ts: usize,
            from: Option<N>,
            out_i: usize,
            in_i: usize,
        }
        let ts0 = self.st_set_find(ctx, v);
        let mut todo = vec![St {
            v,
            ts: ts0,
            from: None,
            out_i: 0,
            in_i: 0,
        }];
        let mut best: Option<E> = None;
        while !todo.is_empty() {
            let top = todo.len() - 1;
            if todo[top].out_i == 0
                && todo[top].in_i == 0
                && best.is_some_and(|b| self.slack(b) == 0)
            {
                todo.pop();
                continue;
            }
            let sv = todo[top].v;
            let mut updated = false;
            while todo[top].out_i < self.nodes[sv].out.len() {
                let e = self.nodes[sv].out[todo[top].out_i];
                let h = self.edges[e].head;
                if self.tree_edge(e) {
                    if Some(h) == todo[top].from {
                        todo[top].out_i += 1;
                        continue;
                    }
                    todo[top].out_i += 1;
                    let ts = self.st_set_find(ctx, h);
                    todo.push(St {
                        v: h,
                        ts,
                        from: Some(sv),
                        out_i: 0,
                        in_i: 0,
                    });
                    updated = true;
                    break;
                } else {
                    let hs = self.st_set_find(ctx, h);
                    if hs != todo[top].ts
                        && (best.is_none() || self.slack(e) < self.slack(best.unwrap()))
                    {
                        best = Some(e);
                    }
                }
                todo[top].out_i += 1;
            }
            if updated {
                continue;
            }
            while todo[top].in_i < self.nodes[sv].in_.len() {
                let e = self.nodes[sv].in_[todo[top].in_i];
                let t = self.edges[e].tail;
                if self.tree_edge(e) {
                    if Some(t) == todo[top].from {
                        todo[top].in_i += 1;
                        continue;
                    }
                    todo[top].in_i += 1;
                    let ts = self.st_set_find(ctx, t);
                    todo.push(St {
                        v: t,
                        ts,
                        from: Some(sv),
                        out_i: 0,
                        in_i: 0,
                    });
                    updated = true;
                    break;
                } else {
                    let tsf = self.st_set_find(ctx, t);
                    if tsf != todo[top].ts
                        && (best.is_none() || self.slack(e) < self.slack(best.unwrap()))
                    {
                        best = Some(e);
                    }
                }
                todo[top].in_i += 1;
            }
            if updated {
                continue;
            }
            todo.pop();
        }
        best
    }

    fn st_heapify(ctx: &mut Ctx, heap: &mut [usize], size: usize, mut i: usize) {
        loop {
            let left = 2 * (i + 1) - 1;
            let right = 2 * (i + 1);
            let mut smallest = i;
            if left < size && ctx.subtrees[heap[left]].size < ctx.subtrees[heap[smallest]].size {
                smallest = left;
            }
            if right < size && ctx.subtrees[heap[right]].size < ctx.subtrees[heap[smallest]].size {
                smallest = right;
            }
            if smallest != i {
                heap.swap(i, smallest);
                ctx.subtrees[heap[i]].heap_index = i;
                ctx.subtrees[heap[smallest]].heap_index = smallest;
                i = smallest;
            } else {
                break;
            }
            if i >= size {
                break;
            }
        }
    }

    fn tree_adjust(&mut self, v: N, delta: i32) {
        let mut stack: Vec<(N, Option<N>)> = vec![(v, None)];
        while let Some((v, from)) = stack.pop() {
            self.nodes[v].rank += delta;
            for i in 0..self.nodes[v].tree_in.len() {
                let w = self.edges[self.nodes[v].tree_in[i]].tail;
                if Some(w) != from {
                    stack.push((w, Some(v)));
                }
            }
            for i in 0..self.nodes[v].tree_out.len() {
                let w = self.edges[self.nodes[v].tree_out[i]].head;
                if Some(w) != from {
                    stack.push((w, Some(v)));
                }
            }
        }
    }

    fn merge_trees(&mut self, ctx: &mut Ctx, e: E) -> Option<usize> {
        let t0 = self.st_set_find(ctx, self.edges[e].tail);
        let t1 = self.st_set_find(ctx, self.edges[e].head);
        if ctx.subtrees[t0].heap_index == NOT_ON_HEAP {
            let delta = self.slack(e);
            if delta != 0 {
                self.tree_adjust(ctx.subtrees[t0].rep, delta);
            }
        } else {
            let delta = -self.slack(e);
            if delta != 0 {
                self.tree_adjust(ctx.subtrees[t1].rep, delta);
            }
        }
        if self.add_tree_edge(ctx, e) != 0 {
            return None;
        }
        Some(Self::st_set_union(ctx, t0, t1))
    }

    fn feasible_tree(&mut self, ctx: &mut Ctx) -> i32 {
        let nl = self.nlist_vec(ctx.g);
        for &n in &nl {
            self.nodes[n].subtree = None;
        }
        let mut tree: Vec<usize> = Vec::new();
        for &n in &nl {
            if self.nodes[n].subtree.is_none() {
                match self.find_tight_subtree(ctx, n) {
                    Some(t) => tree.push(t),
                    None => return 2,
                }
            }
        }
        // STbuildheap
        let mut size = tree.len();
        for (i, &t) in tree.iter().enumerate() {
            ctx.subtrees[t].heap_index = i;
        }
        let mut i = size / 2;
        loop {
            Self::st_heapify(ctx, &mut tree, size, i);
            if i == 0 {
                break;
            }
            i -= 1;
        }
        let mut error = 0;
        while size > 1 {
            // STextractmin
            let rv = tree[0];
            ctx.subtrees[rv].heap_index = NOT_ON_HEAP;
            tree[0] = tree[size - 1];
            ctx.subtrees[tree[0]].heap_index = 0;
            tree[size - 1] = rv;
            size -= 1;
            Self::st_heapify(ctx, &mut tree, size, 0);
            let rep = ctx.subtrees[rv].rep;
            let Some(ee) = self.inter_tree_edge_search(ctx, rep) else {
                error = 1;
                break;
            };
            let Some(t1) = self.merge_trees(ctx, ee) else {
                error = 2;
                break;
            };
            let hi = ctx.subtrees[t1].heap_index;
            Self::st_heapify(ctx, &mut tree, size, hi);
        }
        for &n in &nl {
            self.nodes[n].subtree = None;
        }
        if error != 0 {
            return error;
        }
        self.init_cutvalues(ctx);
        0
    }

    fn treeupdate(&mut self, mut v: N, w: N, cutvalue: i32, dir: bool) -> N {
        loop {
            let (lo, li) = (self.nodes[v].low, self.nodes[v].lim);
            let wl = self.nodes[w].lim;
            if lo <= wl && wl <= li {
                break;
            }
            let e = self.nodes[v].par.unwrap();
            let d = if v == self.edges[e].tail { dir } else { !dir };
            if d {
                self.edges[e].cutvalue += cutvalue;
            } else {
                self.edges[e].cutvalue -= cutvalue;
            }
            let (t, h) = (self.edges[e].tail, self.edges[e].head);
            v = if self.nodes[t].lim > self.nodes[h].lim {
                t
            } else {
                h
            };
        }
        v
    }

    fn rerank(&mut self, v: N, delta: i32) {
        let mut stack = vec![v];
        while let Some(v) = stack.pop() {
            self.nodes[v].rank -= delta;
            let par = self.nodes[v].par;
            for i in 0..self.nodes[v].tree_out.len() {
                let e = self.nodes[v].tree_out[i];
                if Some(e) != par {
                    stack.push(self.edges[e].head);
                }
            }
            for i in 0..self.nodes[v].tree_in.len() {
                let e = self.nodes[v].tree_in[i];
                if Some(e) != par {
                    stack.push(self.edges[e].tail);
                }
            }
        }
    }

    fn ns_update(&mut self, ctx: &mut Ctx, e: E, f: E) -> i32 {
        let delta = self.slack(f);
        let (et, eh) = (self.edges[e].tail, self.edges[e].head);
        if delta > 0 {
            let s = self.nodes[et].tree_in.len() + self.nodes[et].tree_out.len();
            if s == 1 {
                self.rerank(et, delta);
            } else {
                let s = self.nodes[eh].tree_in.len() + self.nodes[eh].tree_out.len();
                if s == 1 {
                    self.rerank(eh, -delta);
                } else if self.nodes[et].lim < self.nodes[eh].lim {
                    self.rerank(et, delta);
                } else {
                    self.rerank(eh, -delta);
                }
            }
        }
        let cutvalue = self.edges[e].cutvalue;
        let (ft, fh) = (self.edges[f].tail, self.edges[f].head);
        let lca = self.treeupdate(ft, fh, cutvalue, true);
        if self.treeupdate(fh, ft, cutvalue, false) != lca {
            return 2;
        }
        let lca_low = self.nodes[lca].low;
        self.invalidate_path(lca, fh);
        self.invalidate_path(lca, ft);
        self.edges[f].cutvalue = -cutvalue;
        self.edges[e].cutvalue = 0;
        self.exchange_tree_edges(ctx, e, f);
        let par = self.nodes[lca].par;
        self.dfs_range(lca, par, lca_low);
        0
    }

    fn scan_and_normalize(&mut self, ctx: &Ctx) -> i32 {
        let mut minrank = i32::MAX;
        let mut maxrank = i32::MIN;
        let nl = self.nlist_vec(ctx.g);
        for &n in &nl {
            if self.nodes[n].node_type == NORMAL {
                minrank = minrank.min(self.nodes[n].rank);
                maxrank = maxrank.max(self.nodes[n].rank);
            }
        }
        for &n in &nl {
            self.nodes[n].rank = self.nodes[n].rank.wrapping_sub(minrank);
        }
        maxrank.wrapping_sub(minrank)
    }

    fn free_tree_list(&mut self, ctx: &mut Ctx) {
        for n in self.nlist_vec(ctx.g) {
            self.nodes[n].tree_in.clear();
            self.nodes[n].tree_out.clear();
            self.nodes[n].mark = 0;
        }
        ctx.tree_edge.clear();
    }

    fn lr_balance(&mut self, ctx: &mut Ctx) {
        for i in 0..ctx.tree_edge.len() {
            let e = ctx.tree_edge[i];
            if self.edges[e].cutvalue == 0 {
                let Some(f) = self.enter_edge(e) else {
                    continue;
                };
                let delta = self.slack(f);
                if delta <= 1 {
                    continue;
                }
                let (t, h) = (self.edges[e].tail, self.edges[e].head);
                if self.nodes[t].lim < self.nodes[h].lim {
                    self.rerank(t, delta / 2);
                } else {
                    self.rerank(h, -delta / 2);
                }
            }
        }
        self.free_tree_list(ctx);
    }

    fn tb_balance(&mut self, ctx: &mut Ctx) {
        let maxrank = self.scan_and_normalize(ctx);
        let mut adj = 0;
        if let Some(s) = self.agget(ctx.g, "TBbalance") {
            if s == "min" {
                adj = 1;
            } else if s == "max" {
                adj = 2;
            }
            if adj != 0 {
                for n in self.nlist_vec(ctx.g) {
                    if self.nodes[n].node_type == NORMAL {
                        if self.nodes[n].in_.is_empty() && adj == 1 {
                            self.nodes[n].rank = 0;
                        }
                        if self.nodes[n].out.is_empty() && adj == 2 {
                            self.nodes[n].rank = maxrank;
                        }
                    }
                }
            }
        }
        let nsz = (maxrank.max(0) as usize) + 1;
        let mut nrank = vec![0i32; nsz];
        let mut tree_node = self.nlist_vec(ctx.g);
        if std::env::var_os("GVDUMP").is_some() {
            let mut l = String::from("TBPRE");
            for &n in &tree_node {
                let nm = if self.nodes[n].node_type == NORMAL { self.nodes[n].name.as_str() } else { "_" };
                l.push_str(&format!(" {}:{}", nm, self.nodes[n].rank));
            }
            eprintln!("{l}");
        }
        {
            let nodes = &self.nodes;
            if adj > 1 {
                qsort(&mut tree_node, &mut |a: &N, b: &N| {
                    nodes[*b].rank.cmp(&nodes[*a].rank)
                });
            } else {
                qsort(&mut tree_node, &mut |a: &N, b: &N| {
                    nodes[*a].rank.cmp(&nodes[*b].rank)
                });
            }
        }
        for &n in &tree_node {
            if self.nodes[n].node_type == NORMAL {
                let r = self.nodes[n].rank;
                if r >= 0 && (r as usize) < nsz {
                    nrank[r as usize] += 1;
                }
            }
        }
        let dump = std::env::var_os("GVDUMP").is_some();
        if dump {
            let mut l = String::from("TBORDER");
            for &n in &tree_node {
                let nm = if self.nodes[n].node_type == NORMAL { self.nodes[n].name.as_str() } else { "_" };
                l.push_str(&format!(" {}:{}", nm, self.nodes[n].rank));
            }
            eprintln!("{l}");
        }
        for &n in &tree_node {
            if self.nodes[n].node_type != NORMAL {
                continue;
            }
            let mut inweight = 0;
            let mut outweight = 0;
            let mut low = 0;
            let mut high = maxrank;
            for &e in &self.nodes[n].in_ {
                inweight += self.edges[e].weight;
                low = low.max(self.nodes[self.edges[e].tail].rank + self.edges[e].minlen);
            }
            for &e in &self.nodes[n].out {
                outweight += self.edges[e].weight;
                high = high.min(self.nodes[self.edges[e].head].rank - self.edges[e].minlen);
            }
            if low < 0 {
                low = 0;
            }
            if adj != 0 {
                if inweight == outweight {
                    self.nodes[n].rank = if adj == 1 { low } else { high };
                }
            } else if inweight == outweight {
                let get = |nr: &Vec<i32>, i: i32| -> i32 {
                    if i >= 0 && (i as usize) < nr.len() {
                        nr[i as usize]
                    } else {
                        0
                    }
                };
                let mut choice = low;
                for i in (low + 1)..=high {
                    if get(&nrank, i) < get(&nrank, choice) {
                        choice = i;
                    }
                }
                let r = self.nodes[n].rank;
                if r >= 0 && (r as usize) < nsz {
                    nrank[r as usize] -= 1;
                }
                if choice >= 0 && (choice as usize) < nsz {
                    nrank[choice as usize] += 1;
                }
                if dump {
                    eprintln!("TB {} {} {} {} -> {}", self.nodes[n].name, r, low, high, choice);
                }
                self.nodes[n].rank = choice;
            }
            self.nodes[n].tree_in.clear();
            self.nodes[n].tree_out.clear();
            self.nodes[n].mark = 0;
        }
    }

    fn ns_init_graph(&mut self, ctx: &mut Ctx) -> bool {
        let nl = self.nlist_vec(ctx.g);
        for &n in &nl {
            self.nodes[n].mark = 0;
            ctx.n_nodes += 1;
        }
        let mut feasible = true;
        for &n in &nl {
            self.nodes[n].priority = 0;
            for i in 0..self.nodes[n].in_.len() {
                let e = self.nodes[n].in_[i];
                self.nodes[n].priority += 1;
                self.edges[e].cutvalue = 0;
                self.edges[e].tree_index = -1;
                let ed = &self.edges[e];
                if self.nodes[ed.head].rank - self.nodes[ed.tail].rank < ed.minlen {
                    feasible = false;
                }
            }
            self.nodes[n].tree_in.clear();
            self.nodes[n].tree_out.clear();
        }
        feasible
    }

    /// `rank2(g, balance, maxiter, search_size)`
    pub fn rank2(&mut self, g: G, balance: i32, maxiter: i32, search_size: i32) -> i32 {
        let mut ctx = Ctx {
            g,
            tree_edge: Vec::new(),
            s_i: 0,
            n_nodes: 0,
            search_size: SEARCHSIZE,
            subtrees: Vec::new(),
        };
        if self.graphs[g].nlist.is_none() {
            return 0;
        }
        let feasible = self.ns_init_graph(&mut ctx);
        if !feasible {
            self.init_rank(&ctx);
        }
        ctx.search_size = if search_size >= 0 {
            search_size
        } else {
            SEARCHSIZE
        };
        let err = self.feasible_tree(&mut ctx);
        if err != 0 {
            self.free_tree_list(&mut ctx);
            return err;
        }
        if maxiter <= 0 {
            self.free_tree_list(&mut ctx);
            return 0;
        }
        let mut iter = 0;
        while let Some(e) = self.leave_edge(&mut ctx) {
            let Some(f) = self.enter_edge(e) else {
                self.free_tree_list(&mut ctx);
                return 2;
            };
            let err = self.ns_update(&mut ctx, e, f);
            if err != 0 {
                self.free_tree_list(&mut ctx);
                return err;
            }
            iter += 1;
            if iter >= maxiter {
                break;
            }
        }
        let _ = ctx.n_nodes;
        match balance {
            1 => {
                self.tb_balance(&mut ctx);
                ctx.tree_edge.clear();
            }
            2 => self.lr_balance(&mut ctx),
            _ => {
                self.scan_and_normalize(&ctx);
                self.free_tree_list(&mut ctx);
            }
        }
        0
    }

    /// `rank(g, balance, maxiter)`
    pub fn ns_rank(&mut self, g: G, balance: i32, maxiter: i32) -> i32 {
        let search_size = match self.agget(g, "searchsize") {
            Some(s) => atoi(s),
            None => SEARCHSIZE,
        };
        self.rank2(g, balance, maxiter, search_size)
    }

    fn x_val(&self, e: E, v: N, dir: i32) -> i32 {
        let ed = &self.edges[e];
        let other = if ed.tail == v { ed.head } else { ed.tail };
        let (lo, li) = (self.nodes[v].low, self.nodes[v].lim);
        let ol = self.nodes[other].lim;
        let f;
        let mut rv;
        if !(lo <= ol && ol <= li) {
            f = true;
            rv = ed.weight;
        } else {
            f = false;
            rv = if self.tree_edge(e) { ed.cutvalue } else { 0 };
            rv -= ed.weight;
        }
        let mut d = if dir > 0 {
            if ed.head == v { 1 } else { -1 }
        } else if ed.tail == v {
            1
        } else {
            -1
        };
        if f {
            d = -d;
        }
        if d < 0 {
            rv = -rv;
        }
        rv
    }

    fn x_cutval(&mut self, f: E) {
        let ft = self.edges[f].tail;
        let (v, dir) = if self.nodes[ft].par == Some(f) {
            (ft, 1)
        } else {
            (self.edges[f].head, -1)
        };
        let mut sum: i32 = 0;
        for &e in &self.nodes[v].out {
            sum = sum.wrapping_add(self.x_val(e, v, dir));
        }
        for &e in &self.nodes[v].in_ {
            sum = sum.wrapping_add(self.x_val(e, v, dir));
        }
        self.edges[f].cutvalue = sum;
    }

    fn dfs_cutval(&mut self, v: N, par: Option<E>) {
        struct St {
            v: N,
            par: Option<E>,
            out_i: usize,
            in_i: usize,
        }
        let mut todo = vec![St {
            v,
            par,
            out_i: 0,
            in_i: 0,
        }];
        while !todo.is_empty() {
            let top = todo.len() - 1;
            let tv = todo[top].v;
            let mut updated = false;
            while todo[top].out_i < self.nodes[tv].tree_out.len() {
                let e = self.nodes[tv].tree_out[todo[top].out_i];
                todo[top].out_i += 1;
                if Some(e) != todo[top].par {
                    todo.push(St {
                        v: self.edges[e].head,
                        par: Some(e),
                        out_i: 0,
                        in_i: 0,
                    });
                    updated = true;
                    break;
                }
            }
            if updated {
                continue;
            }
            while todo[top].in_i < self.nodes[tv].tree_in.len() {
                let e = self.nodes[tv].tree_in[todo[top].in_i];
                todo[top].in_i += 1;
                if Some(e) != todo[top].par {
                    todo.push(St {
                        v: self.edges[e].tail,
                        par: Some(e),
                        out_i: 0,
                        in_i: 0,
                    });
                    updated = true;
                    break;
                }
            }
            if updated {
                continue;
            }
            if let Some(p) = todo[top].par {
                self.x_cutval(p);
            }
            todo.pop();
        }
    }

    fn dfs_range_init(&mut self, v: N) -> i32 {
        self.dfs_range_impl(v, None, 1, false)
    }

    fn dfs_range(&mut self, v: N, par: Option<E>, low: i32) -> i32 {
        if self.nodes[v].par == par && self.nodes[v].low == low {
            return self.nodes[v].lim + 1;
        }
        self.dfs_range_impl(v, par, low, true)
    }

    fn dfs_range_impl(&mut self, v: N, par: Option<E>, low: i32, incremental: bool) -> i32 {
        struct St {
            v: N,
            par: Option<E>,
            lim: i32,
            out_i: usize,
            in_i: usize,
        }
        let mut lim = 0;
        self.nodes[v].par = par;
        self.nodes[v].low = low;
        let mut todo = vec![St {
            v,
            par,
            lim: low,
            out_i: 0,
            in_i: 0,
        }];
        while !todo.is_empty() {
            let top = todo.len() - 1;
            let sv = todo[top].v;
            let mut pushed = false;
            while todo[top].out_i < self.nodes[sv].tree_out.len() {
                let e = self.nodes[sv].tree_out[todo[top].out_i];
                todo[top].out_i += 1;
                if Some(e) != todo[top].par {
                    let n = self.edges[e].head;
                    let slim = todo[top].lim;
                    if incremental && self.nodes[n].par == Some(e) && self.nodes[n].low == slim {
                        todo[top].lim = self.nodes[n].lim + 1;
                    } else {
                        self.nodes[n].par = Some(e);
                        self.nodes[n].low = slim;
                        todo.push(St {
                            v: n,
                            par: Some(e),
                            lim: slim,
                            out_i: 0,
                            in_i: 0,
                        });
                    }
                    pushed = true;
                    break;
                }
            }
            if pushed {
                continue;
            }
            while todo[top].in_i < self.nodes[sv].tree_in.len() {
                let e = self.nodes[sv].tree_in[todo[top].in_i];
                todo[top].in_i += 1;
                if Some(e) != todo[top].par {
                    let n = self.edges[e].tail;
                    let slim = todo[top].lim;
                    if incremental && self.nodes[n].par == Some(e) && self.nodes[n].low == slim {
                        todo[top].lim = self.nodes[n].lim + 1;
                    } else {
                        self.nodes[n].par = Some(e);
                        self.nodes[n].low = slim;
                        todo.push(St {
                            v: n,
                            par: Some(e),
                            lim: slim,
                            out_i: 0,
                            in_i: 0,
                        });
                    }
                    pushed = true;
                    break;
                }
            }
            if pushed {
                continue;
            }
            self.nodes[sv].lim = todo[top].lim;
            lim = todo[top].lim;
            todo.pop();
            if let Some(b) = todo.last_mut() {
                b.lim = lim + 1;
            }
        }
        lim + 1
    }
}
