//! Port of the Graphviz emit pipeline: `lib/common/emit.c`, `lib/gvc/gvrender.c`, the
//! `*_gencode` half of `lib/common/shapes.c`, `lib/common/arrows.c`, the emit half of
//! `lib/common/htmltable.c` and `emit_label` from `lib/common/labels.c`.
//!
//! [`model::Model`] gathers the laid-out graph; [`job::Emitter`] walks it exactly like
//! `emit_graph` and drives a renderer implementing [`job::Engine`].

pub(crate) mod arrows;
pub(crate) mod attrs;
pub(crate) mod corners;
#[allow(clippy::module_inception)]
pub(crate) mod emit;
pub(crate) mod geom;
pub(crate) mod html;
pub(crate) mod job;
pub(crate) mod model;
pub(crate) mod num;
pub(crate) mod output;
pub(crate) mod shapes;
pub(crate) mod usershape;

use job::{Emitter, Engine, Job, LoadImage};
use model::Model;

thread_local! {
    /// `Y_invert` (`dot -y`): the writers flip every y coordinate about the bounding box.
    static Y_INVERT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// `job->flags`: gvRenderJobs ORs each graph's `chkOrder` into the job, which `dot`
    /// reuses for every graph it renders, so an `outputorder` stays for later graphs.
    static ORDER_FLAGS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    /// `job->loadimage.engine` of each `-T` job, by its place in the run's job list: `dot`
    /// keeps its jobs, and what they selected, from graph to graph.
    static LOADIMAGE: std::cell::RefCell<std::collections::HashMap<usize, LoadImage>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Sets `Y_invert`, as `dot -y` does for the whole run.
pub fn set_y_invert(on: bool) {
    Y_INVERT.with(|c| c.set(on));
}

/// `setYInvert(g)`: the `Y` of `offsets_t`, `ymin + ymax` of the bounding box when `Y_invert`
/// is set, else 0.
pub(crate) fn y_offset(bb: graphviz_pure_core::BoundingBox) -> f64 {
    if Y_INVERT.with(std::cell::Cell::get) {
        bb.ur.y + bb.ll.y
    } else {
        0.0
    }
}

/// `yDir(y, Y_off)` (`lib/common/output.c`).
pub(crate) fn y_dir(y: f64, y_off: f64) -> f64 {
    if Y_INVERT.with(std::cell::Cell::get) {
        y_off - y
    } else {
        y
    }
}

/// Output order flags from the graph's `outputorder` (`chkOrder`), accumulated.
fn order_flags(m: &Model<'_>) -> u32 {
    let f = match m.root_attrs().get("outputorder") {
        Some(s) if s.starts_with('n') && s == "nodesfirst" => job::EMIT_SORTED,
        Some(s) if s.starts_with('e') && s == "edgesfirst" => job::EMIT_EDGE_SORTED,
        _ => 0,
    };
    ORDER_FLAGS.with(|c| {
        c.set(c.get() | f);
        c.get()
    })
}

/// How a job runs `emit_graph`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RunJob {
    /// The job's place in the run's job list, whose loadimage plugin it keeps from graph to
    /// graph; `None` for a job of its own each time (the one `json_begin_graph` renders the
    /// graph as xdot with), which starts with none.
    pub job: Option<usize>,
    /// Whether the text C writes straight into the output file is kept
    /// ([`Engine::write_output_file`]): only when there is one, in the CLI.
    pub output_file: bool,
}

/// Runs `emit_graph` over `m` with `eng` as job 0 without output file, returning the engine.
pub(crate) fn run<E: Engine>(m: &Model<'_>, eng: E) -> E {
    let opts = RunJob {
        job: Some(0),
        output_file: false,
    };
    run_job(m, eng, opts).0
}

/// Runs `emit_graph` over `m` with `eng`, returning the engine and the text written
/// straight into the output file before the engine's output ([`Engine::write_output_file`]).
pub(crate) fn run_job<E: Engine>(m: &Model<'_>, eng: E, opts: RunJob) -> (E, String) {
    let features = eng.features();
    let flags = order_flags(m);
    job::check_pagedir(m.root_attrs().nonempty("pagedir").unwrap_or("BL"));
    let job = Job::new(m, features, flags);
    let loadimage = opts
        .job
        .and_then(|k| LOADIMAGE.with(|l| l.borrow().get(&k).copied()));
    let mut em = Emitter {
        job,
        eng,
        loadimage,
        output_file: opts.output_file.then(String::new),
    };
    em.emit_graph();
    if let Some(k) = opts.job
        && let Some(li) = em.loadimage
    {
        LOADIMAGE.with(|l| l.borrow_mut().insert(k, li));
    }
    (em.eng, em.output_file.unwrap_or_default())
}

/// Resets process-wide counters and caches (gradient and anchor ids, the loadimage plugins,
/// the files and epsf numbers of [`graphviz_pure_core::usershape`]) before a new run.
pub fn reset_render_state() {
    graphviz_pure_core::usershape::reset_run();
    crate::svg_engine::reset_gradient_ids();
    html::reset_anchor_ids();
    ORDER_FLAGS.with(|c| c.set(0));
    LOADIMAGE.with(|l| l.borrow_mut().clear());
    graphviz_pure_core::font::textfont_dict_reset();
}
