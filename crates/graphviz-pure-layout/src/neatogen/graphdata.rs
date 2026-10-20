//! Sparse graph representation (`vtx_data`, `lib/neatogen/sparsegraph.h`)
//! and its construction (`makeGraphData` in `lib/neatogen/neatoinit.c`).

use super::{MODE_HIER, MODE_IPSEP, MODEL_SUBSET};
use crate::common::types::{LGraph, Sub};
use std::collections::HashMap;

/// `vtx_data`: `edges[0]` is the vertex itself.
#[derive(Debug, Clone, Default)]
pub struct VtxData {
    pub edges: Vec<i32>,
    pub ewgts: Option<Vec<f32>>,
    pub eweights: Option<Vec<f32>>,
    pub edists: Option<Vec<i8>>,
}

impl VtxData {
    pub fn nedges(&self) -> usize {
        self.edges.len()
    }
}

/// `graph->ewgts != NULL` (C checks the first vertex).
pub fn has_ewgts(g: &[VtxData]) -> bool {
    g.first().is_some_and(|v| v.ewgts.is_some())
}

/// `makeGraphData(g, nv, &nedges, mode, model, &nodes)`.
///
/// Loops are ignored; multi-edges are merged (weights add, the length is the
/// maximum). Returns the graph, the edge count and the node list (by `ND_id`).
pub fn make_graph_data(
    g: &LGraph<'_>,
    sub: &Sub,
    mode: i32,
    model: i32,
) -> (Vec<VtxData>, usize, Vec<usize>) {
    let (have_len, have_wt) = if model != MODEL_SUBSET {
        (g.edge_attr_declared("len"), g.edge_attr_declared("weight"))
    } else {
        (false, false)
    };
    let have_dir = mode == MODE_HIER || mode == MODE_IPSEP;
    let nv = sub.nodes.len();
    let mut graph = Vec::with_capacity(nv);
    let mut nodes = Vec::with_capacity(nv);
    let mut ne = 0usize;
    for (i, &np) in sub.nodes.iter().enumerate() {
        debug_assert_eq!(g.nodes[np].id, i);
        nodes.push(np);
        let mut v = VtxData {
            edges: vec![i as i32],
            ewgts: (have_len || have_dir).then(|| vec![0.0f32]),
            eweights: have_wt.then(|| vec![0.0f32]),
            edists: have_dir.then(|| vec![0i8]),
        };
        let mut ps: HashMap<(usize, usize), usize> = HashMap::new();
        let mut j = 1usize;
        let edges: Vec<usize> = g.all_edges(sub, np).collect();
        for ep in edges {
            let ed = &g.edges[ep];
            if ed.head == ed.tail {
                continue;
            }
            let (mut a, mut b) = (g.nodes[ed.tail].id, g.nodes[ed.head].id);
            if a > b {
                std::mem::swap(&mut a, &mut b);
            }
            let idx = *ps.entry((a, b)).or_insert(j);
            if idx != j {
                if have_wt {
                    v.eweights.as_mut().unwrap()[idx] += ed.factor as f32;
                }
                if have_len {
                    let w = &mut v.ewgts.as_mut().unwrap()[idx];
                    *w = f64::from(*w).max(ed.dist) as f32;
                }
            } else {
                let vp = if ed.tail == np { ed.head } else { ed.tail };
                ne += 1;
                j += 1;
                v.edges.push(g.nodes[vp].id as i32);
                if have_wt {
                    v.eweights.as_mut().unwrap().push(ed.factor as f32);
                }
                if have_len {
                    v.ewgts.as_mut().unwrap().push(ed.dist as f32);
                } else if have_dir {
                    v.ewgts.as_mut().unwrap().push(1.0);
                }
                if have_dir {
                    let none = g.src.edges[ep]
                        .attrs
                        .get("dir")
                        .is_some_and(|s| s.starts_with("none"));
                    let d = if none {
                        0
                    } else if np == ed.head {
                        1
                    } else {
                        -1
                    };
                    v.edists.as_mut().unwrap().push(d);
                }
            }
        }
        graph.push(v);
    }
    if have_dir {
        acyclic(&mut graph, mode);
    }
    (graph, ne / 2, nodes)
}

/// `acyclic` / `dfsCycle`: reverse back edges of the direction constraints.
fn acyclic(graph: &mut [VtxData], mode: i32) {
    let nv = graph.len();
    let mut mark = vec![false; nv];
    let mut onstack = vec![false; nv];
    fn dfs(graph: &mut [VtxData], i: usize, mode: i32, mark: &mut [bool], onstack: &mut [bool]) {
        let x: i8 = if mode == MODE_IPSEP { -1 } else { 1 };
        mark[i] = true;
        onstack[i] = true;
        for e in 1..graph[i].nedges() {
            if graph[i].edists.as_ref().unwrap()[e] == 1 {
                continue;
            }
            let j = graph[i].edges[e] as usize;
            if onstack[j] {
                graph[i].edists.as_mut().unwrap()[e] = x;
                let f = (1..graph[j].nedges())
                    .find(|&f| graph[j].edges[f] as usize == i)
                    .unwrap();
                graph[j].edists.as_mut().unwrap()[f] = -1;
            } else if !mark[j] {
                dfs(graph, j, mode, mark, onstack);
            }
        }
        onstack[i] = false;
    }
    for i in 0..nv {
        if !mark[i] {
            dfs(graph, i, mode, &mut mark, &mut onstack);
        }
    }
}
