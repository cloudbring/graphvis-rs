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
use crate::common::utils::{atoi, compute_bb, late_int, mapbool};
use crate::cutil::Drand48;
use crate::pack::{PackInfo, PackMode, get_pack, get_pack_mode_info, pack_graphs, pccomps};
use graphviz_pure_core::math;
use graphviz_pure_core::strtod::{Scanned, scan_lf, sscanf};
use graphviz_pure_core::{Graph, Point, Result};

/// `CL_OFFSET`: default margin around packed components, in points.
pub const CL_OFFSET: i64 = 8;

/// `user_pos(posptr, pinptr, np, nG)`: parse a node's `pos`/`pin` (and, with `dim > 2`, its
/// `z`).
fn user_pos(
    g: &mut LGraph<'_>,
    n: usize,
    inputscale: f64,
    pos_declared: bool,
    z_declared: bool,
) -> bool {
    if !pos_declared {
        return false;
    }
    let Some(p) = g.nattr(n, "pos") else {
        return false;
    };
    let z = if z_declared { g.nattr(n, "z") } else { None };
    let pin = mapbool(g.nattr(n, "pin"));
    let ng = g.nodes.len();
    let name = &g.src.nodes[n].name;
    let node = &mut g.nodes[n];
    match user_pos_values(p, z, pin, name, inputscale, ng, &mut g.drand, &mut node.pos) {
        Some(pinned) => {
            node.pinned = pinned;
            true
        }
        None => false,
    }
}

/// The body of `user_pos` on a node's `pos`, `z` (`None` when `z` is not declared) and
/// `pin`: fills `pvec` (`ND_pos`, of length `Ndim`) and returns `ND_pinned`, or `None` when
/// `pos` is empty or unreadable (after C's error message).
pub(crate) fn user_pos_values(
    p: &str,
    z: Option<&str>,
    pin: bool,
    name: &str,
    inputscale: f64,
    ng: usize,
    drand: &mut Drand48,
    pvec: &mut [f64],
) -> Option<u8> {
    if p.is_empty() {
        return None;
    }
    let ndim = pvec.len();
    // sscanf assigns each value it reads, even when it then fails.
    let assign = |pvec: &mut [f64], r: &Scanned<'_>, max: usize| {
        let k = usize::try_from(r.count).unwrap_or(0).min(max);
        for (p, v) in pvec.iter_mut().zip(r.values).take(k) {
            *p = v;
        }
    };
    let scale = |pvec: &mut [f64]| {
        if inputscale > 0.0 {
            for v in pvec.iter_mut() {
                *v /= inputscale;
            }
        }
    };
    let pinned = |c: Option<u8>| if c == Some(b'!') || pin { P_PIN } else { P_SET };
    if ndim >= 3 {
        let r = sscanf(p, "%lf,%lf,%lf%c");
        assign(pvec, &r, 3);
        if r.count >= 3 {
            scale(pvec);
            if ndim > 3 {
                jitter_d(pvec, ng, 3, drand);
            }
            return Some(pinned(r.c));
        }
    }
    let r = sscanf(p, "%lf,%lf%c");
    assign(pvec, &r, 2);
    if r.count >= 2 {
        scale(pvec);
        if ndim > 2 {
            if let Some((z, _)) = z.and_then(scan_lf) {
                if let Some(p) = pvec.get_mut(2) {
                    *p = if inputscale > 0.0 { z / inputscale } else { z };
                }
                jitter_d(pvec, ng, 3, drand);
            } else {
                jitter_d(pvec, ng, 2, drand);
            }
        }
        return Some(pinned(r.c));
    }
    graphviz_pure_core::diag::agerrorf(&format!(
        "node {name}, position {p}, expected two doubles\n"
    ));
    None
}

/// `jitter_d(np, nG, n)` (`jitter3d` is `jitter_d(np, nG, 2)`): random coordinates from
/// dimension `from` on.
fn jitter_d(pvec: &mut [f64], ng: usize, from: usize, drand: &mut Drand48) {
    for v in pvec.iter_mut().skip(from) {
        *v = ng as f64 * drand.drand48();
    }
}

/// `addZ(g)`: with `dim >= 3` and a declared `z`, every node's `z` becomes its third
/// coordinate in points, printed with `%lf`.
fn add_z(g: &LGraph<'_>, z_declared: bool, graph: &mut Graph) {
    if g.ndim >= 3 && z_declared {
        for (nd, n) in graph.nodes.iter_mut().zip(&g.nodes) {
            set_z(nd, n.pos.get(2).copied().unwrap_or(0.0));
        }
    }
}

/// `addZ`'s `agxset(n, N_z, "%lf" of POINTS_PER_INCH * ND_pos(n)[2])`.
pub(crate) fn set_z(node: &mut graphviz_pure_core::NodeData, z: f64) {
    node.attrs.insert("z".to_string(), c_lf(72.0 * z));
}

/// `printf("%lf", v)`.
fn c_lf(v: f64) -> String {
    // The macOS libc prints a NaN as `nan` whatever its sign.
    if v.is_nan() {
        "nan".to_string()
    } else if v.is_infinite() {
        if v < 0.0 { "-inf" } else { "inf" }.to_string()
    } else {
        format!("{v:.6}")
    }
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
pub fn set_seed(g: &LGraph<'_>, dflt: i32, seed: &mut i64) -> i32 {
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
        g.nodes[n].pos[0] = ng as f64 * math::cos(a);
        g.nodes[n].pos[1] = ng as f64 * math::sin(a);
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
    am: &AdjustData,
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
    let lgap = crate::common::utils::late_double(&g.src.graph_attrs, "levelsgap", 0.0, -f64::MAX);
    let rv = if mode == MODE_IPSEP {
        let mut opt = super::quad_prog_vpsc::IpsepOptions {
            edge_gap: lgap,
            nsize: sub
                .nodes
                .iter()
                .map(|&v| (g.nodes[v].width, g.nodes[v].height))
                .collect(),
            clusters: super::cola::cluster_map(g, sub),
            ..Default::default()
        };
        let s = g.gattr("diredgeconstraints");
        opt.diredges = if mapbool(s) {
            1
        } else if s.is_some_and(|s| {
            s.as_bytes()
                .get(..4)
                .is_some_and(|p| p.eq_ignore_ascii_case(b"hier"))
        }) {
            2
        } else {
            0
        };
        opt.noverlap = match am.mode {
            super::adjust::AdjustMode::Ipsep => 1,
            super::adjust::AdjustMode::Vpsc => 2,
            _ => 0,
        };
        let margin = super::adjust::sep_factor(g);
        opt.gap = if margin.do_add {
            (2.0 * (margin.x / 72.0), 2.0 * (margin.y / 72.0))
        } else {
            let d = 2.0 * (super::adjust::DFLT_MARGIN / 72.0);
            (d, d)
        };
        super::cola::stress_majorization_cola(
            g,
            p,
            &mut drand,
            &mut crand,
            &gp,
            nv,
            &mut coords,
            &nodes,
            dim,
            model,
            p.max_iter,
            &mut opt,
        )
    } else if mode == MODE_HIER {
        super::digcola::stress_majorization_with_hierarchy(
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
            lgap,
        )
    } else {
        stress_majorization_kd_mkernel(
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
        )
    };
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
fn neato_layout_component(
    g: &mut LGraph<'_>,
    sub: &Sub,
    layout_mode: i32,
    model: i32,
    am: &AdjustData,
) {
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
        majorization(g, sub, ng, layout_mode, model, dim, &p, am);
    }
}

pub(crate) fn is_a_cluster(sg: &graphviz_pure_core::SubgraphData) -> bool {
    graphviz_pure_core::has_cluster_prefix(&sg.name)
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
            crate::common::init::graph_label(&sg.graph_attrs, &sg.name, false, &src.html_strings);
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
        let mut bb = graphviz_pure_core::BoundingBox::new(m, m, -m, -m);
        for &n in &nodes {
            let nd = &g.nodes[n];
            let c = crate::common::utils::coord(g, n);
            let b = graphviz_pure_core::BoundingBox::new(
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
    super::stuff::reduce_graph(graph);
    let attrs = &graph.graph_attrs;
    let no_translate = mapbool(attrs.get("notranslate").map(String::as_str));
    let inputscale = crate::common::utils::get_inputscale(attrs);
    let outdim = late_int(attrs, "dimen", 2, 2);
    let ndim = late_int(attrs, "dim", outdim, 2).min(MAXDIM as i64) as usize;

    let src = graph.clone();
    let mut g = LGraph::new(&src, ndim, EdgeType::Line);
    let pos_declared = g.node_attr_declared("pos");
    let z_declared = g.node_attr_declared("z");
    for n in 0..g.nodes.len() {
        user_pos(&mut g, n, inputscale, pos_declared, z_declared);
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
        add_z(&g, z_declared, graph);
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
        // C adds z before routing the edges, which does not move the nodes.
        add_z(&g, z_declared, graph);
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
    neato_layout_component(g, sub, layout_mode, model, am);
    remove_overlap_with(g, sub, am);
    if no_translate {
        do_edges(g, sub, is_root);
    } else {
        spline_edges(g, sub, is_root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_pos_of(p: &str, z: Option<&str>, ndim: usize) -> (Option<u8>, Vec<f64>, Drand48) {
        let mut drand = Drand48::default();
        let mut pvec = vec![0.0; ndim];
        let pinned = graphviz_pure_core::diag::quietly(|| {
            user_pos_values(p, z, false, "n", -1.0, 4, &mut drand, &mut pvec)
        });
        (pinned, pvec, drand)
    }

    // Issue #245: neatoinit.c's user_pos with Ndim > 2 and two values in `pos`.
    #[test]
    fn user_pos_takes_z_or_a_random_third_coordinate() {
        let mut fresh = Drand48::default();
        // `z` is read and no random number is drawn.
        let (pinned, pvec, drand) = user_pos_of("1,2", Some("3"), 3);
        assert_eq!(
            (pinned, pvec, drand),
            (Some(P_SET), vec![1.0, 2.0, 3.0], fresh)
        );
        // No usable `z`: jitter3d, nG * drand48().
        let (_, pvec, _) = user_pos_of("1,2!", Some("x"), 3);
        assert_eq!(pvec, vec![1.0, 2.0, 4.0 * fresh.drand48()]);
        // Three values in `pos`: `z` is not read; with Ndim 4, jitter_d from the fourth on.
        let mut fresh = Drand48::default();
        let (pinned, pvec, _) = user_pos_of("0x1p3,1e1,nan(1)!", Some("3"), 4);
        assert_eq!(pinned, Some(P_PIN));
        assert_eq!(pvec[..2], [8.0, 10.0]);
        assert!(pvec[2].is_nan());
        assert_eq!(pvec[3], 4.0 * fresh.drand48());
    }

    // sscanf assigns what it read before failing: `pos="5"` leaves ND_pos[0] at 5.
    #[test]
    fn user_pos_keeps_the_values_of_a_failed_scan() {
        let (pinned, pvec, _) = user_pos_of("5", None, 2);
        assert_eq!((pinned, pvec), (None, vec![5.0, 0.0]));
    }
}
