//! Faithful, function-by-function port of Graphviz 16.1.0 `lib/dotgen`
//! (plus `lib/common/ns.c` and the parts of `lib/common` that dot uses).
//!
//! See `docs/porting/dotgen.md` for the module <-> C file map, status and the
//! phase-2/3 handoff notes. Each submodule mirrors one C file; functions keep
//! their C names (snake_case) so the two can be read side by side.

pub mod acyclic;
pub mod cg;
pub mod class1;
pub mod class2;
pub mod cluster;
pub mod decomp;
pub mod fastgr;
pub mod flat;
pub mod init;
pub mod mincross;
pub mod ns;
pub mod output;
pub mod position;
pub mod postproc;
pub mod qsort;
pub mod rank;
pub mod sameport;
pub mod types;
pub mod utils;

use gv_core::{BoundingBox, Graph, Result};
use types::*;

/// All dot layout state: arenas mirroring the cgraph objects and their
/// `Agraphinfo_t`/`Agnodeinfo_t`/`Agedgeinfo_t` records, plus the C file-scope
/// statics.
pub struct Dot<'a> {
    pub input: &'a Graph,
    pub nodes: Vec<NodeInfo>,
    pub edges: Vec<EdgeInfo>,
    pub graphs: Vec<GraphInfo>,
    pub labels: Vec<TextLabel>,
    /// Backing storage of `rank_t.v`/`av` arrays.
    pub bufs: Vec<Vec<Option<N>>>,
    pub n_real_nodes: usize,
    pub edge_seq: u64,
    // globals
    pub cl_type: i32,
    pub concentrate: bool,
    // decomp.c statics
    pub cmark: usize,
    pub decomp_last: Option<N>,
    // rank.c (newrank) statics
    pub xg_last: Option<N>,
    pub weak_id: i32,
    // mincross.c statics
    pub min_quit: i32,
    pub max_iter: i32,
    pub global_min_rank: i32,
    pub global_max_rank: i32,
    pub remincross: bool,
    /// `ED_spl(e)` for real edges, in layout space until postprocessing.
    pub spl: Vec<Option<Vec<Bezier>>>,
}

impl<'a> Dot<'a> {
    fn empty(input: &'a Graph) -> Self {
        Dot {
            input,
            nodes: Vec::new(),
            edges: Vec::new(),
            graphs: Vec::new(),
            labels: Vec::new(),
            bufs: Vec::new(),
            n_real_nodes: 0,
            edge_seq: 0,
            cl_type: LOCAL,
            concentrate: false,
            cmark: 0,
            decomp_last: None,
            xg_last: None,
            weak_id: 0,
            min_quit: 8,
            max_iter: 24,
            global_min_rank: 0,
            global_max_rank: 0,
            remincross: false,
            spl: Vec::new(),
        }
    }

    // ---- rank_t accessors (GD_rank(g)[r]) ----

    #[inline]
    pub fn rk(&self, g: G, r: i32) -> &Rank {
        &self.graphs[g].rank[(r + 1) as usize]
    }

    #[inline]
    pub fn rk_mut(&mut self, g: G, r: i32) -> &mut Rank {
        &mut self.graphs[g].rank[(r + 1) as usize]
    }

    /// `GD_rank(g)[r].n`, 0 when the rank slot does not exist.
    #[inline]
    pub fn rank_n(&self, g: G, r: i32) -> i32 {
        self.graphs[g]
            .rank
            .get((r + 1) as usize)
            .map_or(0, |x| x.n)
    }

    /// `GD_rank(g)[r].v[i]`
    #[inline]
    pub fn rv(&self, g: G, r: i32, i: i32) -> Option<N> {
        if i < 0 {
            return None;
        }
        let rk = self.graphs[g].rank.get((r + 1) as usize)?;
        self.bufs
            .get(rk.vbuf)?
            .get(rk.voff + i as usize)
            .copied()
            .flatten()
    }

    /// `GD_rank(g)[r].v[i] = n`
    #[inline]
    pub fn set_rv(&mut self, g: G, r: i32, i: i32, n: Option<N>) {
        let rk = &self.graphs[g].rank[(r + 1) as usize];
        let (b, o) = (rk.vbuf, rk.voff + i as usize);
        let buf = &mut self.bufs[b];
        if o >= buf.len() {
            buf.resize(o + 1, None);
        }
        buf[o] = n;
    }

    /// Allocates a zeroed `node_t*` array of `len` slots.
    pub fn new_buf(&mut self, len: usize) -> usize {
        self.bufs.push(vec![None; len]);
        self.bufs.len() - 1
    }

    /// `dotLayout` (dotinit.c), without the splines phase.
    pub fn dot_layout(&mut self) -> std::result::Result<(), String> {
        let maxphase = utils::late_int(self.agget(0, "phase"), -1, 1);
        self.dot_init_subg();
        self.dot_init_node_edge();
        self.dot_rank();
        if maxphase == 1 {
            return Ok(());
        }
        self.dot_mincross()?;
        if maxphase == 2 {
            return Ok(());
        }
        self.dot_position()?;
        if maxphase == 3 {
            return Ok(());
        }
        if self.graphs[0].flags & NEW_RANK != 0 {
            self.remove_fill();
        }
        self.dot_sameports();
        Ok(())
    }
}

/// Lays out `graph` with the dotgen port.
pub fn layout_dot(graph: &mut Graph) -> Result<()> {
    let rankdir = graph
        .graph_attrs
        .get("rankdir")
        .map(String::as_str)
        .unwrap_or("TB")
        .to_string();
    let rankdir = match rankdir.as_str() {
        "LR" | "BT" | "RL" => rankdir,
        _ => "TB".to_string(),
    };
    let name = graph.name.clone();
    for node in &mut graph.nodes {
        gv_geom::compute_node_dimensions_with_rankdir(node, &name, &rankdir);
    }
    if graph.nodes.is_empty() {
        graph.bb = BoundingBox::new(0.0, 0.0, 0.0, 0.0);
        return Ok(());
    }
    let result = {
        let mut d = Dot::new(graph);
        d.graph_init();
        let maxphase = utils::late_int(d.agget(0, "phase"), -1, 1);
        d.dot_layout()
            .map_err(gv_core::GraphvizError::LayoutError)?;
        if (1..=3).contains(&maxphase) {
            output::Output::phase(&d, maxphase)
        } else {
            d.dotneato_postprocess();
            output::Output::collect(&d)
        }
    };
    result.apply(graph);
    Ok(())
}
