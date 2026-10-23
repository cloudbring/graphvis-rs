//! Node and edge initialisation, ported from `lib/fdpgen/fdpinit.c`
//! (`fdp_init_node_edge`, `init_edge`, `initialPositions`). Node geometry
//! comes from [`LGraph::new`] (`common_init_node` + `gv_nodesize`).

use crate::common::types::{LGraph, P_PIN, P_SET};
use crate::common::utils::{late_double, mapbool};
use graphviz_pure_core::strtod::sscanf;

/// `init_edge(e, E_len)` for every edge: `ED_factor` from `weight` (no lower
/// bound but 0) and `ED_dist` from `len` (default `K`).
pub fn init_edges(g: &mut LGraph<'_>, k: f64) {
    for (i, e) in g.edges.iter_mut().enumerate() {
        let a = &g.src.edges[i].attrs;
        e.factor = late_double(a, "weight", 1.0, 0.0);
        e.dist = late_double(a, "len", k, 0.0);
    }
}

/// `initialPositions(g)`: `pos` (in inches, divided by `inputscale` when
/// positive) and `pin`.
pub fn initial_positions(g: &mut LGraph<'_>, inputscale: f64) {
    if !g.node_attr_declared("pos") {
        return;
    }
    for n in 0..g.nodes.len() {
        let Some(p) = g.nattr(n, "pos").filter(|s| !s.is_empty()) else {
            continue;
        };
        let r = sscanf(p, "%lf,%lf%c");
        let [x, y, ..] = r.values;
        if r.count < 2 {
            // fprintf(stderr, ...): not agerr, so it does not count as an error and its
            // bytes are written as they are, without agerr's escaping of control characters.
            let name = g.src.nodes.get(n).map_or("", |nd| nd.name.as_str());
            let msg = format!("Warning: node {name}, position {p}, expected two floats\n");
            graphviz_pure_core::diag::fprintf_stderr(&msg);
            continue;
        }
        let pin = mapbool(g.nattr(n, "pin"));
        let nd = &mut g.nodes[n];
        nd.pos[0] = x;
        nd.pos[1] = y;
        if inputscale > 0.0 {
            nd.pos[0] /= inputscale;
            nd.pos[1] /= inputscale;
        }
        nd.pinned = P_SET;
        if r.c == Some(b'!') || pin {
            nd.pinned = P_PIN;
        }
    }
}

/// `get_inputscale(g)`.
pub fn get_inputscale(g: &LGraph<'_>) -> f64 {
    crate::common::utils::get_inputscale(&g.src.graph_attrs)
}
