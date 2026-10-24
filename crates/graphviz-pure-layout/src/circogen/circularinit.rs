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
use std::collections::HashMap;

/// The derived graph of `circomps`.
struct Derived {
    dg: CGraph,
    /// For each derived edge, the original edge that created it (same
    /// tail and head).
    orig_edge: Vec<usize>,
}

/// `makeDerivedNode(dg, agnameof(v), 1, v)` for each node of `g`. The derived graph is a root
/// of its own, whose dictionary has no HTML strings, so nodes named `<G>` and `G` are one
/// derived node. The first of them creates it (its place in the derived graph); the last is
/// its `ORIGN` (the sizes and attributes it reads, and the node its position is copied to).
/// The others never get a position. Returns each node's derived node, named by the first
/// node, and each derived node's `ORIGN`.
fn derived_nodes(g: &LGraph<'_>) -> (Vec<usize>, Vec<usize>) {
    let n = g.nodes.len();
    let mut first: HashMap<&str, usize> = HashMap::new();
    let mut dnode = vec![0; n];
    let mut orign: Vec<usize> = (0..n).collect();
    for v in 0..n {
        let f = *first.entry(g.src.nodes[v].name.as_str()).or_insert(v);
        dnode[v] = f;
        orign[f] = v;
    }
    (dnode, orign)
}

/// `circomps(g, &cnt)` without the component split: build the strict,
/// undirected, loop-free derived graph of `g`.
fn derived_graph(g: &LGraph<'_>, dnode: &[usize], arena: &mut EdgeArena) -> Derived {
    let mut dg = CGraph::default();
    for v in 0..g.nodes.len() {
        if dnode[v] == v {
            dg.add_node(v);
        }
    }
    let mut orig_edge = Vec::new();
    for v in 0..g.nodes.len() {
        for &e in &g.out[v] {
            let (dt, dh) = (dnode[v], dnode[g.edges[e].head]);
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

/// `ccomps(dg, &c_cnt, 0)`: the connected components of the derived graph, each in node
/// order, ordered by their first node.
fn derived_comps(dg: &CGraph, arena: &EdgeArena, n_total: usize) -> Vec<Sub> {
    let mut mark = vec![false; n_total];
    let mut out = Vec::new();
    for v in dg.nodes() {
        if mark[v] {
            continue;
        }
        mark[v] = true;
        let (mut c, mut stack) = (Vec::new(), vec![v]);
        while let Some(u) = stack.pop() {
            c.push(u);
            for e in dg.edges(u) {
                let w = arena.other(e, u);
                if !mark[w] {
                    mark[w] = true;
                    stack.push(w);
                }
            }
        }
        let mut sub = Sub::new(n_total, c);
        // ccomps(dg, &c_cnt, 0) names them with setPrefix's default
        sub.name = format!("_cc_{}", out.len());
        out.push(sub);
    }
    out
}

/// A component of the derived graph as `circomps` fills it in: its nodes, and for each the
/// derived edges of its `ORIGN`'s out-edges. An edge whose tail was merged into another
/// node's derived node is not among them.
fn component(
    g: &LGraph<'_>,
    dg: &CGraph,
    arena: &EdgeArena,
    nodes: &[usize],
    dnode: &[usize],
    orign: &[usize],
) -> CGraph {
    let mut sg = CGraph::default();
    for &n in nodes {
        sg.add_node(n);
    }
    for &n in nodes {
        for &e in &g.out[orign[n]] {
            let dh = dnode[g.edges[e].head];
            if let Some(de) = dg.find_edge(n, dh).filter(|_| dh != n) {
                sg.subedge(arena, de);
            }
        }
    }
    sg
}

/// `copyPosns(g)`: derived positions to the original nodes (`ORIGN`).
fn copy_posns(c: &Circ, g: &mut LGraph<'_>, nodes: &[usize], orign: &[usize]) {
    for &n in nodes {
        g.nodes[orign[n]].pos[0] = c.pos[n][0];
        g.nodes[orign[n]].pos[1] = c.pos[n][1];
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
    // the derived edge joins derived nodes, which the original edge's ends name
    let mut ends = Vec::new();
    for v in 0..n {
        for de in d.dg.outs(v) {
            let e = d.orig_edge[de];
            out[v].push(e);
            inn[arena.head[de]].push(e);
            ends.push((e, g.edges[e].tail, g.edges[e].head));
            g.edges[e].tail = v;
            g.edges[e].head = arena.head[de];
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
    for (e, t, h) in ends {
        g.edges[e].tail = t;
        g.edges[e].head = h;
    }
}

/// `circoLayout(g)`.
fn circo_layout_main(g: &mut LGraph<'_>) {
    if g.nodes.is_empty() {
        return;
    }
    let nn = g.nodes.len();
    let mut arena = EdgeArena::default();
    let (dnode, orign) = derived_nodes(g);
    let d = derived_graph(g, &dnode, &mut arena);
    let mut c = Circ {
        nd: vec![CData::default(); nn],
        pos: vec![[0.0; 2]; nn],
        width: (0..nn).map(|n| g.nodes[orign[n]].width).collect(),
        height: (0..nn).map(|n| g.nodes[orign[n]].height).collect(),
        arena,
        blocks: Vec::new(),
        bl: Vec::new(),
        order_count: 1,
        min_dist: 0.0,
        rootname: None,
        n_root: false,
        node_root: (0..nn)
            .map(|n| {
                let v = g.nattr(orign[n], "root").unwrap_or("");
                !v.is_empty() && mapbool(Some(v))
            })
            .collect(),
        names: g.src.nodes.iter().map(|n| n.name.clone()).collect(),
    };
    // Without merged names, the components of the derived graph are those of g, in the same
    // order.
    let merged = dnode.iter().enumerate().any(|(v, &f)| v != f);
    let mut ccs = if merged {
        derived_comps(&d.dg, &c.arena, nn)
    } else {
        ccomps(g)
    };
    if ccs.len() == 1 {
        let sg = component(g, &d.dg, &c.arena, &ccs[0].nodes, &dnode, &orign);
        circular_layout(&mut c, &sg, g);
        copy_posns(&c, g, &ccs[0].nodes, &orign);
        let root = g.root_sub();
        adjust_nodes(g, &root);
    } else {
        let mut pinfo = PackInfo::default();
        get_pack_info(g, PackMode::Node, CL_OFFSET, &mut pinfo);
        // The port packs the derived nodes in place of the original ones: a merged node's
        // first node stands in for it with its ORIGN's size (makeDerivedNode copies `lw`,
        // `rw` and `ht`), and gets its own position and size back after packing.
        let identity: Vec<usize> = (0..nn).collect();
        let kept: Vec<(usize, Vec<f64>, [f64; 3])> = (0..nn)
            .filter(|&f| orign[f] != f)
            .map(|f| {
                let (n, o) = (&g.nodes[f], &g.nodes[orign[f]]);
                let kept = (f, n.pos.clone(), [n.lw, n.rw, n.ht]);
                let size = [o.lw, o.rw, o.ht];
                [g.nodes[f].lw, g.nodes[f].rw, g.nodes[f].ht] = size;
                kept
            })
            .collect();
        for cc in &ccs {
            let sg = component(g, &d.dg, &c.arena, &cc.nodes, &dnode, &orign);
            circular_layout(&mut c, &sg, g);
            // adjustNodes(sg) reads `overlap` from the derived graph, which
            // declares no attributes: a no-op.
            copy_posns(&c, g, &cc.nodes, &identity);
        }
        pack_derived(g, &d, &c.arena, &mut ccs, &pinfo);
        for (f, pos, [lw, rw, ht]) in kept {
            let packed = std::mem::replace(&mut g.nodes[f].pos, pos);
            g.nodes[orign[f]].pos = packed;
            [g.nodes[f].lw, g.nodes[f].rw, g.nodes[f].ht] = [lw, rw, ht];
        }
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
