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

use job::{Emitter, Engine, Job};
use model::Model;

thread_local! {
    /// `Y_invert` (`dot -y`): the writers flip every y coordinate about the bounding box.
    static Y_INVERT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// `job->flags`: gvRenderJobs ORs each graph's `chkOrder` into the job, which `dot`
    /// reuses for every graph it renders, so an `outputorder` stays for later graphs.
    static ORDER_FLAGS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
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

/// Runs `emit_graph` over `m` with `eng`, returning the engine.
pub(crate) fn run<E: Engine>(m: &Model<'_>, eng: E) -> E {
    let features = eng.features();
    let flags = order_flags(m);
    let job = Job::new(m, features, flags);
    let mut em = Emitter { job, eng };
    em.emit_graph();
    em.eng
}

/// Resets process-wide counters (gradient and anchor ids) before a new run.
pub fn reset_render_state() {
    crate::svg_engine::reset_gradient_ids();
    html::reset_anchor_ids();
    ORDER_FLAGS.with(|c| c.set(0));
}
