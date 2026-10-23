//! dot's binding of `lib/common/ns.c` (network simplex, `rank`/`rank2`).
//!
//! The algorithm lives in [`crate::common::ns`], generic over
//! [`NsGraph`]; this module implements that trait over dot's fast graph
//! rooted at `GD_nlist(g)` (`ND_in`/`ND_out`, `ND_rank`, `ED_minlen`,
//! `ED_weight` and the tree scratch fields of `NodeInfo`/`EdgeInfo`).

use super::Dot;
use super::types::*;
use super::utils::atoi;
use crate::common::ns::{self, NsGraph, SEARCHSIZE};

/// `graph_t *g` as seen by `ns.c`: dot's arenas plus the graph being ranked.
struct DotNs<'x, 'a> {
    d: &'x mut Dot<'a>,
    g: G,
}

impl NsGraph for DotNs<'_, '_> {
    fn nlist(&self) -> Vec<usize> {
        let mut v = Vec::new();
        let mut n = self.d.graphs[self.g].nlist;
        while let Some(x) = n {
            v.push(x);
            n = self.d.nodes[x].next;
        }
        v
    }
    fn attr(&self, name: &str) -> Option<String> {
        self.d.agget(self.g, name).map(str::to_string)
    }
    fn is_normal(&self, n: usize) -> bool {
        self.d.nodes[n].node_type == NORMAL
    }
    fn dump_name(&self, n: usize) -> String {
        self.d.nodes[n].name.clone()
    }
    fn err_name(&self, n: usize) -> String {
        if self.d.nodes[n].node_type == NORMAL {
            self.d.nodes[n].name.clone()
        } else {
            "%0".to_string()
        }
    }
    fn rank(&self, n: usize) -> i32 {
        self.d.nodes[n].rank
    }
    fn set_rank(&mut self, n: usize, r: i32) {
        self.d.nodes[n].rank = r;
    }
    fn set_mark(&mut self, n: usize, m: bool) {
        self.d.nodes[n].mark = m as usize;
    }
    fn priority(&self, n: usize) -> i32 {
        self.d.nodes[n].priority
    }
    fn set_priority(&mut self, n: usize, p: i32) {
        self.d.nodes[n].priority = p;
    }
    fn low(&self, n: usize) -> i32 {
        self.d.nodes[n].low
    }
    fn set_low(&mut self, n: usize, v: i32) {
        self.d.nodes[n].low = v;
    }
    fn lim(&self, n: usize) -> i32 {
        self.d.nodes[n].lim
    }
    fn set_lim(&mut self, n: usize, v: i32) {
        self.d.nodes[n].lim = v;
    }
    fn par(&self, n: usize) -> Option<usize> {
        self.d.nodes[n].par
    }
    fn set_par(&mut self, n: usize, e: Option<usize>) {
        self.d.nodes[n].par = e;
    }
    fn subtree(&self, n: usize) -> Option<usize> {
        self.d.nodes[n].subtree
    }
    fn set_subtree(&mut self, n: usize, s: Option<usize>) {
        self.d.nodes[n].subtree = s;
    }
    fn in_edges(&self, n: usize) -> &[usize] {
        &self.d.nodes[n].in_
    }
    fn out_edges(&self, n: usize) -> &[usize] {
        &self.d.nodes[n].out
    }
    fn tree_in(&self, n: usize) -> &[usize] {
        &self.d.nodes[n].tree_in
    }
    fn tree_out(&self, n: usize) -> &[usize] {
        &self.d.nodes[n].tree_out
    }
    fn tree_in_mut(&mut self, n: usize) -> &mut Vec<usize> {
        &mut self.d.nodes[n].tree_in
    }
    fn tree_out_mut(&mut self, n: usize) -> &mut Vec<usize> {
        &mut self.d.nodes[n].tree_out
    }
    fn tail(&self, e: usize) -> usize {
        self.d.edges[e].tail
    }
    fn head(&self, e: usize) -> usize {
        self.d.edges[e].head
    }
    fn minlen(&self, e: usize) -> i32 {
        self.d.edges[e].minlen
    }
    fn weight(&self, e: usize) -> i32 {
        self.d.edges[e].weight
    }
    fn cutvalue(&self, e: usize) -> i32 {
        self.d.edges[e].cutvalue
    }
    fn set_cutvalue(&mut self, e: usize, v: i32) {
        self.d.edges[e].cutvalue = v;
    }
    fn tree_index(&self, e: usize) -> i32 {
        self.d.edges[e].tree_index
    }
    fn set_tree_index(&mut self, e: usize, v: i32) {
        self.d.edges[e].tree_index = v;
    }
}

impl Dot<'_> {
    /// `rank2(g, balance, maxiter, search_size)`
    pub fn rank2(
        &mut self,
        g: G,
        balance: i32,
        maxiter: i32,
        search_size: i32,
    ) -> Result<i32, String> {
        ns::rank2(&mut DotNs { d: self, g }, balance, maxiter, search_size)
    }

    /// `rank(g, balance, maxiter)`
    pub fn ns_rank(&mut self, g: G, balance: i32, maxiter: i32) -> Result<i32, String> {
        let search_size = match self.agget(g, "searchsize") {
            Some(s) => atoi(s),
            None => SEARCHSIZE,
        };
        self.rank2(g, balance, maxiter, search_size)
    }
}
