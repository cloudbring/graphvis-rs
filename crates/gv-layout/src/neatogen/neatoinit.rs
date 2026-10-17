//! The neato driver, ported from `lib/neatogen/neatoinit.c`: `neato_layout`,
//! `neatoLayout`, `majorization`, `user_pos`, `checkStart`/`setSeed`,
//! `initRegular`, `neatoMode`, `neatoModel`, `checkExp`, `addCluster`.

use super::adjust::{AdjustData, graph_adjust_mode, remove_overlap_with};
use super::graphdata::make_graph_data;
use super::neatosplines::{spline_edges, spline_edges0};
use super::stress::{DFLT_ITERATIONS, OPT_SMART_INIT, stress_majorization_kd_mkernel};
use super::stuff::{kk_neato, scan_graph_mode};
use super::*;
use crate::common::postproc::gv_postprocess;
use crate::common::types::*;
use crate::common::utils::{atoi, compute_bb, late_int, mapbool, parse_f64_rest};
use crate::pack::{PackInfo, PackMode, get_pack, get_pack_mode_info, pack_graphs, pccomps};
use gv_core::{Graph, Point, Result};

/// `CL_OFFSET`: default margin around packed components, in points.
pub const CL_OFFSET: i64 = 8;

/// `user_pos(posptr, pinptr, np, nG)`: parse a node's `pos`/`pin`.
fn user_pos(g: &mut LGraph<'_>, n: usize, inputscale: f64, pos_declared: bool) -> bool {
    if !pos_declared {
        return false;
    }
    let Some(p) = g.nattr(n, "pos").filter(|s| !s.is_empty()) else {
        return false;
    };
    let ndim = g.ndim;
    let mut vals = Vec::new();
    let mut rest = p;
    let mut c = None;
    while vals.len() < 3 {
        let Some((v, r)) = parse_f64_rest(rest) else {
            break;
        };
        vals.push(v);
        rest = r;
        if vals.len() == 2 && ndim < 3 {
            c = rest.chars().next();
            break;
        }
        match rest.strip_prefix(',') {
            Some(r) if vals.len() < 3 => rest = r,
            _ => {
                c = rest.chars().next();
                break;
            }
        }
    }
    if vals.len() >= 3 && ndim < 3 {
        vals.truncate(2);
    }
    if vals.len() < 2 {
        return false;
    }
    let pin_attr = mapbool(g.nattr(n, "pin"));
    let node = &mut g.nodes[n];
    for (i, v) in vals.iter().enumerate().take(ndim) {
        node.pos[i] = *v;
    }
    node.pinned = P_SET;
    if inputscale > 0.0 {
        for v in node.pos.iter_mut().take(vals.len()) {
            *v /= inputscale;
        }
    }
    if c == Some('!') || pin_attr {
        node.pinned = P_PIN;
    }
    true
}

/// `neatoMode(g)`.
fn neato_mode(g: &LGraph<'_>) -> i32 {
    match g.gattr("mode") {
        Some("KK") => MODE_KK,
        Some("major") => MODE_MAJOR,
        Some("sgd") => MODE_SGD,
        Some("hier") => MODE_HIER,
        Some("ipsep") => MODE_IPSEP,
        _ => MODE_MAJOR,
    }
}

/// `neatoModel(g)`.
fn neato_model(g: &LGraph<'_>) -> i32 {
    match g.gattr("model") {
        None | Some("") | Some("shortpath") => MODEL_SHORTPATH,
        Some("circuit") => MODEL_CIRCUIT,
        Some("subset") => MODEL_SUBSET,
        Some("mds") => {
            if g.edge_attr_declared("len") {
                MODEL_MDS
            } else {
                MODEL_SHORTPATH
            }
        }
        Some(_) => MODEL_SHORTPATH,
    }
}

/// `setSeed(G, dflt, &seed)`.
fn set_seed(g: &LGraph<'_>, dflt: i32, seed: &mut i64) -> i32 {
    let Some(p) = g.gattr("start").filter(|s| !s.is_empty()) else {
        return dflt;
    };
    let mut init = dflt;
    let mut rest = p;
    let first = p.as_bytes()[0];
    if first.is_ascii_alphabetic() {
        if let Some(r) = p.strip_prefix("self") {
            init = INIT_SELF;
            rest = r;
        } else if let Some(r) = p.strip_prefix("regular") {
            init = INIT_REGULAR;
            rest = r;
        } else if let Some(r) = p.strip_prefix("random") {
            init = INIT_RANDOM;
            rest = r;
        }
    } else if first.is_ascii_digit() {
        init = INIT_RANDOM;
    }
    if init == INIT_RANDOM {
        // C seeds from time(NULL) when no number follows; the port stays
        // deterministic and uses 1 instead.
        *seed = match rest.as_bytes().first() {
            Some(b) if b.is_ascii_digit() => {
                crate::common::utils::parse_leading_int(rest).unwrap_or(1)
            }
            _ => 1,
        };
    }
    init
}

/// `initRegular(G, nG)`: nodes evenly on a circle.
fn init_regular(g: &mut LGraph<'_>, sub: &Sub, ng: usize) {
    let mut a = 0.0f64;
    let da = 2.0 * std::f64::consts::PI / ng as f64;
    for &n in &sub.nodes {
        g.nodes[n].pos[0] = ng as f64 * a.cos();
        g.nodes[n].pos[1] = ng as f64 * a.sin();
        g.nodes[n].pinned = P_SET;
        a += da;
        if g.ndim > 2 {
            for d in 2..g.ndim {
                g.nodes[n].pos[d] = ng as f64 * g.drand.drand48();
            }
        }
    }
}

/// `checkStart(G, nG, dflt)`: interpret `start` and seed `drand48`.
pub fn check_start(g: &mut LGraph<'_>, sub: &Sub, ng: usize, dflt: i32, _p: &NeatoParams) -> i32 {
    let mut seed = 1i64;
    let init = set_seed(g, dflt, &mut seed);
    if init == INIT_REGULAR {
        init_regular(g, sub, ng);
    }
    g.drand.srand48(seed);
    init
}

/// `checkExp(G)`: the `stresswt` exponent (1 or 2).
fn check_exp(g: &LGraph<'_>) -> i32 {
    let e = late_int(&g.src.graph_attrs, "stresswt", 2, 0);
    if e == 0 || e > 2 { 2 } else { e as i32 }
}

/// `majorization(mg, g, nv, mode, model, dim, am)`.
fn majorization(
    g: &mut LGraph<'_>,
    sub: &Sub,
    nv: usize,
    mode: i32,
    model: i32,
    dim: usize,
    p: &NeatoParams,
) {
    let init = check_start(
        g,
        sub,
        nv,
        if mode == MODE_HIER {
            INIT_SELF
        } else {
            INIT_RANDOM
        },
        p,
    );
    let mut opts = check_exp(g);
    if init == INIT_SELF {
        opts |= OPT_SMART_INIT;
    }
    let mut coords = vec![vec![0.0f64; nv]; dim];
    let (gp, _ne, nodes) = make_graph_data(g, sub, mode, model);
    let mut drand = g.drand;
    let mut crand = g.crand;
    let rv = stress_majorization_kd_mkernel(
        g,
        p,
        &mut drand,
        &mut crand,
        &gp,
        nv,
        &mut coords,
        &nodes,
        dim,
        opts,
        model,
        p.max_iter,
    );
    g.drand = drand;
    g.crand = crand;
    if rv.is_ok() {
        for &v in &sub.nodes {
            let idx = g.nodes[v].id;
            for (i, c) in coords.iter().enumerate().take(g.ndim) {
                g.nodes[v].pos[i] = c[idx];
            }
        }
    }
}

/// `neatoLayout(mg, g, layoutMode, layoutModel, am)`: lay out one component.
fn neato_layout_component(g: &mut LGraph<'_>, sub: &Sub, layout_mode: i32, model: i32) {
    let max_iter = match g.gattr("maxiter") {
        Some(s) => atoi(s),
        None if layout_mode == MODE_MAJOR => DFLT_ITERATIONS,
        None if layout_mode == MODE_SGD => 30,
        None => 100 * sub.nodes.len() as i64,
    };
    let mut p = NeatoParams {
        max_iter,
        ..NeatoParams::default()
    };
    let (ng, kk) = scan_graph_mode(g, sub, layout_mode, &mut p);
    if ng < 2 || p.max_iter < 0 {
        return;
    }
    if layout_mode == MODE_KK {
        let mut kk = kk.expect("KK state");
        kk_neato(g, sub, &mut kk, ng, model, &p);
    } else if layout_mode == MODE_SGD {
        super::sgd::sgd(g, sub, model, &p);
    } else {
        let dim = g.ndim;
        majorization(g, sub, ng, layout_mode, model, dim, &p);
    }
}

fn is_a_cluster(sg: &gv_core::SubgraphData) -> bool {
    sg.name.len() >= 7 && sg.name[..7].eq_ignore_ascii_case("cluster")
        || mapbool(sg.graph_attrs.get("cluster").map(String::as_str))
}

/// Top-level subgraphs of the root in creation order.
pub fn top_level_subgraphs(graph: &Graph) -> Vec<usize> {
    let mut child = vec![false; graph.subgraphs.len()];
    for sg in &graph.subgraphs {
        for c in &sg.subgraphs {
            child[c.0 as usize] = true;
        }
    }
    (0..graph.subgraphs.len()).filter(|&i| !child[i]).collect()
}

/// `addCluster(g)` (IPSEPCOLA builds): register the root's top-level
/// clusters and compute their boxes from member nodes and edges.
fn add_cluster(g: &mut LGraph<'_>) {
    let src = g.src;
    for si in top_level_subgraphs(src) {
        let sg = &src.subgraphs[si];
        if !is_a_cluster(sg) {
            continue;
        }
        let (label, label_pos) =
            crate::common::init::graph_label(&sg.graph_attrs, &sg.name, false, g.is_directed());
        let mut border = [Point::default(); 4];
        if let Some(l) = &label {
            let d = crate::common::init::pad(l.dimen);
            border[if label_pos & LABEL_AT_TOP != 0 { 2 } else { 0 }] = d;
        }
        let ci = g.clusters.len();
        g.clusters.push(LCluster {
            subgraph: si,
            bb: Default::default(),
            label,
            label_pos,
            border,
            children: Vec::new(),
        });
        g.root_clusters.push(ci);
        // compute_bb(subg): nodes and the subgraph's own edges.
        let nodes = subgraph_nodes(src, si);
        if nodes.is_empty() {
            continue;
        }
        let m = f64::from(i32::MAX);
        let mut bb = gv_core::BoundingBox::new(m, m, -m, -m);
        for &n in &nodes {
            let nd = &g.nodes[n];
            let c = crate::common::utils::coord(g, n);
            let b = gv_core::BoundingBox::new(
                c.x - (nd.lw + nd.rw) / 2.0,
                c.y - nd.ht / 2.0,
                c.x + (nd.lw + nd.rw) / 2.0,
                c.y + nd.ht / 2.0,
            );
            crate::common::geom::expandbb(&mut bb, &b);
        }
        for e in subgraph_edges(src, si) {
            let ed = &g.edges[e];
            if let Some(spl) = &ed.spl {
                for b in spl {
                    for pt in &b.list {
                        crate::common::geom::expandbp(&mut bb, *pt);
                    }
                }
            }
            for l in [&ed.label, &ed.head_label, &ed.tail_label, &ed.xlabel]
                .into_iter()
                .flatten()
            {
                if l.set {
                    crate::common::splines::add_label_bb(&mut bb, l, false);
                }
            }
        }
        g.clusters[ci].bb = bb;
    }
}

/// Nodes of a subgraph including nested subgraphs, in sequence order.
pub fn subgraph_nodes(graph: &Graph, si: usize) -> Vec<usize> {
    let mut v: Vec<usize> = Vec::new();
    let mut stack = vec![si];
    while let Some(s) = stack.pop() {
        let sg = &graph.subgraphs[s];
        v.extend(sg.nodes.iter().map(|n| n.0 as usize));
        stack.extend(sg.subgraphs.iter().map(|c| c.0 as usize));
    }
    v.sort_unstable();
    v.dedup();
    v
}

/// Edges of a subgraph including nested subgraphs, in sequence order.
pub fn subgraph_edges(graph: &Graph, si: usize) -> Vec<usize> {
    let mut v: Vec<usize> = Vec::new();
    let mut stack = vec![si];
    while let Some(s) = stack.pop() {
        let sg = &graph.subgraphs[s];
        v.extend(sg.edges.iter().map(|e| e.0 as usize));
        stack.extend(sg.subgraphs.iter().map(|c| c.0 as usize));
    }
    v.sort_unstable();
    v.dedup();
    v
}

/// `doEdges(g)`: route edges without translating first.
fn do_edges(g: &mut LGraph<'_>, sub: &mut Sub, is_root: bool) {
    compute_bb(g, sub, is_root);
    spline_edges0(g, sub, is_root, true);
}

/// `neato_layout(g)`.
pub fn neato_layout(graph: &mut Graph) -> Result<()> {
    let attrs = &graph.graph_attrs;
    let no_translate = mapbool(attrs.get("notranslate").map(String::as_str));
    let inputscale = {
        let d = crate::common::utils::late_double(attrs, "inputscale", -1.0, 0.0);
        if d == 0.0 { 72.0 } else { d }
    };
    let outdim = late_int(attrs, "dimen", 2, 2);
    let ndim = late_int(attrs, "dim", outdim, 2).min(MAXDIM as i64) as usize;

    let src = graph.clone();
    let mut g = LGraph::new(&src, ndim, EdgeType::Line);
    let pos_declared = g.node_attr_declared("pos");
    for n in 0..g.nodes.len() {
        user_pos(&mut g, n, inputscale, pos_declared);
    }
    let layout_mode = neato_mode(&g);
    let am = graph_adjust_mode(&g, None);
    let model = neato_model(&g);
    let mut pinfo = PackInfo::default();
    let mode = get_pack_mode_info(g.gattr("packmode"), PackMode::Undef, &mut pinfo);
    let mut pack = get_pack(g.gattr("pack"), -1, CL_OFFSET);
    if mode == PackMode::Undef {
        if pack < 0 && layout_mode != MODE_KK {
            pack = CL_OFFSET;
        }
        pinfo.mode = PackMode::Node;
    } else if pack < 0 {
        pack = CL_OFFSET;
    }

    if pack >= 0 {
        let (mut ccs, pin) = pccomps(&g);
        if ccs.len() > 1 {
            for gc in ccs.iter_mut() {
                layout_and_route(&mut g, gc, false, layout_mode, model, &am, no_translate);
            }
            let mut fixed = None;
            if pin {
                let mut bp = vec![false; ccs.len()];
                bp[0] = true;
                fixed = Some(bp);
            }
            pinfo.margin = pack as u32;
            pinfo.fixed = fixed;
            pinfo.do_splines = true;
            pack_graphs(&mut g, &mut ccs, &pinfo);
        } else {
            let mut root = g.root_sub();
            layout_and_route(
                &mut g,
                &mut root,
                true,
                layout_mode,
                model,
                &am,
                no_translate,
            );
        }
        let mut root = g.root_sub();
        compute_bb(&mut g, &mut root, true);
        add_cluster(&mut g);
    } else {
        let mut root = g.root_sub();
        layout_and_route(
            &mut g,
            &mut root,
            true,
            layout_mode,
            model,
            &am,
            no_translate,
        );
    }
    gv_postprocess(&mut g, !no_translate);
    g.write_back(graph);
    Ok(())
}

fn layout_and_route(
    g: &mut LGraph<'_>,
    sub: &mut Sub,
    is_root: bool,
    layout_mode: i32,
    model: i32,
    am: &AdjustData,
    no_translate: bool,
) {
    neato_layout_component(g, sub, layout_mode, model);
    remove_overlap_with(g, sub, am);
    if no_translate {
        do_edges(g, sub, is_root);
    } else {
        spline_edges(g, sub, is_root);
    }
}
