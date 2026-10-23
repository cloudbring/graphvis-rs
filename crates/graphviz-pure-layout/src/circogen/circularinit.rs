//! `lib/circogen/circularinit.c`: the circo driver.

use super::circular::{CData, Circ, circular_layout};
use super::graph::{CGraph, EdgeArena};
use crate::common::postproc::gv_postprocess;
use crate::common::types::{EdgeType, LGraph, Sub};
use crate::common::utils::mapbool;
use crate::neatogen::adjust::adjust_nodes;
use crate::neatogen::neatoinit::CL_OFFSET;
use crate::neatogen::neatosplines::spline_edges;
use crate::pack::{PackInfo, PackMode, ccomps, get_pack_info, pack_graphs};
use graphviz_pure_core::{Graph, Result};

/// The derived graph of `circomps`.
struct Derived {
    dg: CGraph,
    /// For each derived edge, the original edge that created it (same
    /// tail and head).
    orig_edge: Vec<usize>,
}

/// `circomps(g, &cnt)` without the component split: build the strict,
/// undirected, loop-free derived graph of `g`.
fn derived_graph(g: &LGraph<'_>, arena: &mut EdgeArena) -> Derived {
    let mut dg = CGraph::default();
    for v in 0..g.nodes.len() {
        dg.add_node(v);
    }
    let mut orig_edge = Vec::new();
    for v in 0..g.nodes.len() {
        for &e in &g.out[v] {
            let (dt, dh) = (v, g.edges[e].head);
            if dt != dh {
                let before = arena.tail.len();
                let de = dg.edge(arena, dt, dh);
                if de >= before {
                    orig_edge.resize(de + 1, usize::MAX);
                    orig_edge[de] = e;
                }
            }
        }
    }
    Derived { dg, orig_edge }
}

/// A component of the derived graph: its nodes and all derived edges among them.
fn component(dg: &CGraph, arena: &EdgeArena, nodes: &[usize]) -> CGraph {
    let mut sg = CGraph::default();
    for &n in nodes {
        sg.add_node(n);
    }
    for &n in nodes {
        for e in dg.outs(n) {
            sg.subedge(arena, e);
        }
    }
    sg
}

/// `copyPosns(g)`: derived positions to the original nodes.
fn copy_posns(c: &Circ, g: &mut LGraph<'_>, nodes: &[usize]) {
    for &n in nodes {
        g.nodes[n].pos[0] = c.pos[n][0];
        g.nodes[n].pos[1] = c.pos[n][1];
    }
}

/// `packSubgraphs(ncc, ccs, dg, &pinfo)`: C packs the components of the
/// derived graph, whose edges are the original ones without loops and with
/// one edge per node pair; pack them over the original nodes with the
/// adjacency temporarily reduced to those edges.
fn pack_derived(
    g: &mut LGraph<'_>,
    d: &Derived,
    arena: &EdgeArena,
    ccs: &mut [Sub],
    pinfo: &PackInfo,
) {
    let n = g.nodes.len();
    let mut out = vec![Vec::new(); n];
    let mut inn = vec![Vec::new(); n];
    for v in 0..n {
        for de in d.dg.outs(v) {
            let e = d.orig_edge[de];
            out[v].push(e);
            inn[arena.head[de]].push(e);
        }
    }
    for list in inn.iter_mut() {
        list.sort_by_key(|&e| (g.edges[e].tail, e));
    }
    let saved_out = std::mem::replace(&mut g.out, out);
    let saved_inn = std::mem::replace(&mut g.inn, inn);
    pack_graphs(g, ccs, pinfo);
    g.out = saved_out;
    g.inn = saved_inn;
}

/// `circoLayout(g)`.
fn circo_layout_main(g: &mut LGraph<'_>) {
    if g.nodes.is_empty() {
        return;
    }
    let nn = g.nodes.len();
    let mut arena = EdgeArena::default();
    let d = derived_graph(g, &mut arena);
    let mut c = Circ {
        nd: vec![CData::default(); nn],
        pos: vec![[0.0; 2]; nn],
        width: g.nodes.iter().map(|n| n.width).collect(),
        height: g.nodes.iter().map(|n| n.height).collect(),
        arena,
        blocks: Vec::new(),
        bl: Vec::new(),
        order_count: 1,
        min_dist: 0.0,
        rootname: None,
        n_root: false,
        node_root: (0..nn)
            .map(|n| {
                let v = g.nattr(n, "root").unwrap_or("");
                !v.is_empty() && mapbool(Some(v))
            })
            .collect(),
        names: g.src.nodes.iter().map(|n| n.name.clone()).collect(),
    };
    // Components of the derived graph are those of g, in the same order.
    let mut ccs = ccomps(g);
    if ccs.len() == 1 {
        let sg = component(&d.dg, &c.arena, &ccs[0].nodes);
        circular_layout(&mut c, &sg, g);
        copy_posns(&c, g, &ccs[0].nodes);
        let root = g.root_sub();
        adjust_nodes(g, &root);
    } else {
        let mut pinfo = PackInfo::default();
        get_pack_info(g, PackMode::Node, CL_OFFSET, &mut pinfo);
        for cc in &ccs {
            let sg = component(&d.dg, &c.arena, &cc.nodes);
            circular_layout(&mut c, &sg, g);
            // adjustNodes(sg) reads `overlap` from the derived graph, which
            // declares no attributes: a no-op.
            copy_posns(&c, g, &cc.nodes);
        }
        pack_derived(g, &d, &c.arena, &mut ccs, &pinfo);
    }
}

/// `circo_layout(g)`.
pub fn circo_layout(graph: &mut Graph) -> Result<()> {
    if graph.nodes.is_empty() {
        crate::common::postproc::labels_at_origin(graph);
        return Ok(());
    }
    let src = graph.clone();
    // circo_init_graph: EDGETYPE_LINE by default, 2 dimensions.
    let mut g = LGraph::new(&src, 2, EdgeType::Line);
    circo_layout_main(&mut g);
    let mut root = g.root_sub();
    spline_edges(&mut g, &mut root, true);
    gv_postprocess(&mut g, true);
    g.write_back(graph);
    Ok(())
}
