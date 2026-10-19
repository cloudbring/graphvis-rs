//! Shared layout infrastructure ported from Graphviz `lib/common`, used by the
//! non-dot engines (neato now; fdp, sfdp, twopi, circo, osage, patchwork next).
//!
//! - [`types`] — working graph state (`Agraphinfo_t`/`Agnodeinfo_t`/`Agedgeinfo_t`)
//! - [`init`] — `graph_init`, `common_init_node`, `common_init_edge`
//! - [`shapes`] — shape vertices and `insidefn`s for clipping (`shapes.c`)
//! - [`ports`] — `poly_port`, `record_port`, `compassPort`
//! - [`splines`] — `clip_and_install`, `beginpath`/`endpath`, self loops, port labels (`splines.c`)
//! - [`routespl`] — box-path routing: `routesplines`, `checkpath` (`routespl.c`)
//! - [`arrows`] — arrow flags and lengths (`arrows.c`)
//! - [`postproc`] — `gv_postprocess` (`postproc.c`)
//! - [`xlabels`] — external label placement (`lib/label`)
//! - [`geom`], [`utils`] — geometry macros, attribute parsing, `compute_bb`

// Faithful ports of numeric C loops index several arrays in lockstep.
#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

pub mod arrows;
pub mod cluster_edges;
pub mod geom;
pub mod init;
pub mod ns;
pub mod ports;
pub mod postproc;
pub mod routespl;
pub mod shapes;
pub mod splines;
pub mod types;
pub mod utils;
pub mod xlabels;
