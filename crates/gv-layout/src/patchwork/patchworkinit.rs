//! The patchwork driver, ported from `lib/patchwork/patchworkinit.c`
//! (`patchwork_layout`, `patchwork_init_graph`, `patchwork_init_node_edge`)
//! and `finishNode` from `patchwork.c`.

use super::patchwork::patchwork_layout_tree;
use crate::common::postproc::gv_postprocess;
use crate::common::types::{EdgeType, LCluster, LGraph};
use crate::fdpgen::mk_clusters;
use gv_core::{Graph, Point, Result};

/// `patchwork_layout(g)`. Edges are neither used nor drawn.
pub fn patchwork_layout(graph: &mut Graph) -> Result<()> {
    // patchwork_init_graph: `N_shape = agattr_text(g, AGNODE, "shape", "box")`
    // and `patchwork_init_node`: `agset(n, "shape", "box")`.
    graph
        .node_defaults
        .insert("shape".to_string(), "box".to_string());
    // agattr on the root sets the declared default, which agwrite prints.
    if let Some(d) = graph.dicts.first_mut() {
        d.node.entry("shape".to_string()).or_default().defval = "box".to_string();
    }
    for n in &mut graph.nodes {
        n.attrs.insert("shape".to_string(), "box".to_string());
    }
    let tree = mk_clusters(graph);
    // mkClusters(g, NULL, g) never calls do_graph_label: clusters get no label.
    for &sg in &tree.subgraph {
        graph.subgraphs[sg]
            .graph_attrs
            .insert(gv_core::NO_GRAPH_LABEL_KEY.to_string(), String::new());
    }
    if graph.nodes.is_empty() && tree.root.is_empty() {
        graph.bb = Default::default();
        return Ok(());
    }
    let res = patchwork_layout_tree(graph, &tree);

    // finishNode: an empty declared `fontsize` becomes 0.7 of the node height.
    let fontsize_declared = graph.node_defaults.contains_key("fontsize")
        || graph.nodes.iter().any(|n| n.attrs.contains_key("fontsize"));
    if fontsize_declared {
        for (i, n) in graph.nodes.iter_mut().enumerate() {
            let Some((_, size)) = res.nodes[i] else {
                continue;
            };
            if n.attrs.get("fontsize").is_none_or(|s| s.is_empty()) {
                n.attrs
                    .insert("fontsize".to_string(), format!("{:.3}", size.y * 0.7));
            }
        }
    }

    let src = graph.clone();
    let mut g = LGraph::new(&src, 2, EdgeType::Line);
    // patchwork_init_edge binds the edge record without common_init_edge, so
    // edges carry no labels.
    for e in &mut g.edges {
        e.label = None;
        e.xlabel = None;
        e.head_label = None;
        e.tail_label = None;
    }
    for (i, r) in res.nodes.iter().enumerate() {
        let Some((center, size)) = *r else { continue };
        let nd = &mut g.nodes[i];
        nd.coord = center;
        // ND_width/ND_height = PS2INCH(size); gv_nodesize.
        nd.width = size.x / 72.0;
        nd.height = size.y / 72.0;
        let w = nd.width * 72.0;
        nd.lw = w / 2.0;
        nd.rw = w / 2.0;
        nd.ht = nd.height * 72.0;
        nd.pos[0] = center.x / 72.0;
        nd.pos[1] = center.y / 72.0;
    }
    for c in 0..tree.len() {
        g.clusters.push(LCluster {
            subgraph: tree.subgraph[c],
            bb: res.clusters[c],
            label: None,
            label_pos: 0,
            border: [Point::default(); 4],
            children: tree.children[c].clone(),
        });
    }
    g.root_clusters = tree.root.clone();
    g.bb = res.root_bb;
    gv_postprocess(&mut g, true);
    g.write_back(graph);
    Ok(())
}
