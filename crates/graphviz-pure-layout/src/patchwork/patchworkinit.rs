//! The patchwork driver, ported from `lib/patchwork/patchworkinit.c`
//! (`patchwork_layout`, `patchwork_init_graph`, `patchwork_init_node_edge`)
//! and `finishNode` from `patchwork.c`.

use super::patchwork::patchwork_layout_tree;
use crate::common::postproc::gv_postprocess;
use crate::common::types::{EdgeType, LCluster, LGraph};
use crate::fdpgen::mk_clusters;
use graphviz_pure_core::{Graph, Point, Result};

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
    // patchwork_init_edge only binds the edge record: no common_init_edge, so no labels.
    graph.graph_attrs.insert(
        graphviz_pure_core::NO_EDGE_INIT_KEY.to_string(),
        String::new(),
    );
    let tree = mk_clusters(graph);
    // mkClusters(g, NULL, g) never calls do_graph_label: clusters get no label.
    for &sg in &tree.subgraph {
        graph.subgraphs[sg].graph_attrs.insert(
            graphviz_pure_core::NO_GRAPH_LABEL_KEY.to_string(),
            String::new(),
        );
    }
    if graph.nodes.is_empty() && tree.root.is_empty() {
        graph.bb = Default::default();
        crate::nop::labels_at_origin(graph);
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
    // finishNode's common_init_node runs in walkTree order, so the label diagnostics do;
    // LGraph's own sizing below does not repeat them.
    let env = graphviz_pure_geom::NodeSizeEnv::from_graph(&src);
    for &n in &res.walk_order {
        let _ = graphviz_pure_geom::node_geometry(&src.nodes[n], &env);
    }
    let mut g = graphviz_pure_core::diag::quietly(|| LGraph::new_unported(&src, 2, EdgeType::Line));
    // patchwork_init_edge binds the edge record without common_init_edge, so
    // edges carry no labels (new_unported makes none).
    for (i, r) in res.nodes.iter().enumerate() {
        let Some((center, size)) = *r else { continue };
        let nd = &mut g.nodes[i];
        nd.coord = center;
        // walkTree: ND_width/ND_height = PS2INCH(size) and gv_nodesize set
        // ND_lw/rw/ht from the patch. finishNode then calls common_init_node,
        // which resets ND_width/ND_height to the shape's own size (the `width`
        // and `height` attributes, the label), as LGraph::new computed them;
        // addXLabels sizes the node from those, the drawing from ND_lw/rw/ht.
        nd.lw = size.x / 2.0;
        nd.rw = size.x / 2.0;
        nd.ht = size.y;
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
    // The output and the renderer use the patch size (ND_lw + ND_rw, ND_ht).
    for (i, r) in res.nodes.iter().enumerate() {
        if let Some((_, size)) = *r {
            graph.nodes[i].width = size.x / 72.0;
            graph.nodes[i].height = size.y / 72.0;
        }
    }
    Ok(())
}
