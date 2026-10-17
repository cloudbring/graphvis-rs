# dotgen port (`crates/gv-layout/src/dotgen/`)

Faithful, function-by-function port of Graphviz 16.1.0 `lib/dotgen` plus
`lib/common/ns.c` and the parts of `lib/common` that dot uses. It is the active
`dot` engine (`crates/gv-layout/src/lib.rs`); `GV_DOT_LEGACY=1` selects the old
approximate engine (`dot.rs`) for comparison.

Functions keep their C names (snake_case) so the two can be read side by side.
C file-scope statics live as fields on `Dot` (`cmark`, `min_quit`, `max_iter`,
`remincross`, `weak_id`, ...).

## Module map

| Rust module | C source | Status |
|---|---|---|
| `types.rs` | `common/types.h`, `common/const.h` | `Agraphinfo_t`/`Agnodeinfo_t`/`Agedgeinfo_t`, `rank_t`, `port`, `textlabel_t`, `bezier` |
| `cg.rs` | `lib/cgraph` (subset) | iteration order, `agdelete` (node, recursive into subgraphs), `agsubedge`, `agsubnode`, strict `agedge`, `agsubg`, `agdegree` |
| `init.rs` | `dotinit.c` (`dot_init_subg`, `dot_init_node_edge`), `common/input.c` (`graph_init`, `do_graph_label`), `common/utils.c` (`common_init_edge`) | ported; node and label sizes come from `gv_geom::node_geometry` / `edge_label_geometry` / `graph_label_geometry`; ports via the shared `common::ports::chk_port`; nodes carry a `common::shapes::NodeShape` for `insidefn` clipping |
| `utils.rs` | `common/utils.c` (`late_int`, `late_double`, `mapBool`, `UF_*`, `is_a_cluster`), `class1.c` (`nonconstraint_edge`), `util/gv_math.h` (`scale_clamp`) | ported |
| `qsort.rs` | Apple Libc `qsort` | thin wrapper over the single implementation `crate::cutil::qsort_by` (qsort is unstable; tie order leaks into `TB_balance`, `medians`, `restore_best`, `do_ordering_node`, `edgecmp`). `qsort_vectors.txt` holds recorded system permutations |
| `fastgr.rs` | `fastgr.c` | ported (all functions) |
| `class1.rs` | `class1.c` | ported |
| `acyclic.rs` | `acyclic.c` | ported (dfs iterative, same visiting order) |
| `decomp.rs` | `decomp.c` | ported (incl. the `Cmark` static) |
| `rank.rs` | `rank.c` | `dot1_rank` (collapse_sets/clusters, rank=same/min/max/source/sink, minmax_edges, expand_ranksets, cleanup1) and `dot2_rank` (newrank: compile_samerank/nodes/edges/clusters, strong/weak, break_cycles, connect_components, readout_levels) ported |
| `ns.rs` | `common/ns.c` | full network simplex: init_rank, feasible_tree with STheap, leave/enter edge search order, update/treeupdate/invalidate_path/dfs_range, TB_balance (with `TBbalance=min|max`), LR_balance, `searchsize`, maxiter |
| `class2.rs` | `class2.c` (+ `ports_eq` from position.c) | ported |
| `cluster.rs` | `cluster.c` | ported (expand_cluster, map_path, interclexp, merge_ranks, build_skeleton, install_cluster, mark_clusters, mark_lowclusters) |
| `mincross.rs` | `mincross.c` | ported: ordered components, build_ranks (BFS, backwards walk for clusters), install_in_rank, transpose, reorder, medians, flat_mval, flat_breakcycles/flat_search/flat_rev, flat_reorder/postorder, mincross_clust recursion, remincross, `mclimit`, `ordering=in/out` (graph and node), rec_save/reset_vlists, fillRanks (newrank), checkLabelOrder/fixLabelOrder |
| `position.rs` | `position.c` | ported: set_ycoords (ranksep, `equally`, cluster ht1/ht2, clust_ht, adjustRanks/adjustSimple for flipped labels), expand_leaves (no-op in C), create_aux_edges (make_LR_constraints incl. self-loop space and flat labels, make_edge_pairs, pos_clusters, compress_graph), connectGraph, set_xcoords, dot_compute_bb/rec_bb, set_aspect (ratio fill/compress/expand/value/auto), make_lrvn, contain_nodes |
| `flat.rs` | `flat.c` | ported (flat_edges, flat_node, flat_limits, make_vn_slot, abomination, checkFlatAdjacent) |
| `sameport.rs` | `sameport.c` | ported (`shape_clip` is the shared one) |
| `conc.rs` | `conc.c` | ported: `dot_concentrate`, `mergevirtual`, `rebuild_vlists`, `infuse`, up/down candidates |
| `dotsplines.rs` | `dotsplines.c` | ported entirely (see "Phase 2" below) |
| `compound.rs` | `compound.c` | ported: `dot_compoundEdges`, `makeCompoundEdge`, `splineIntersectf`, `boxIntersectf`, cluster map |
| `splinegraph.rs` | glue | `SplineGraph`/`PortGraph`/`XLabelGraph`/`OrthoGraph` for `Dot` (the `ND_*`/`ED_*` view the shared `lib/common` and `lib/ortho` code uses), `record_path` (`pboxfn`), `arrow_flags` with `ED_conc_opp_flag` |
| `postproc.rs` | `common/postproc.c` (`gv_postprocess`, `place_graph_label`, `place_flip_graph_label`, `place_root_label`, `translate_drawing`, `map_edge`, `translate_bb`, `map_point`), `dotinit.c` (`removeFill`) | ported; `addXLabels` is the shared `common::xlabels::add_xlabels` |
| `output.rs` | `common/output.c` (`attach_attrs_and_arrows`, the parts the IR needs) | write-back into `gv_core::Graph`: positions, splines with `sflag`/`eflag`, `lp`/`head_lp`/`tail_lp`/`xlp` with C's emission rules, graph/cluster label positions (`nop::LABEL_POS_KEY`) |

Shared code outside `dotgen/` that dot uses (one implementation per C function):

| Module | C source |
|---|---|
| `common/splines.rs` | `common/splines.c`: `clip_and_install`, `arrow_clip`, `bezier_clip`, `shape_clip(0)`, `new_spline`, `beginpath`/`endpath`, `conc_slope`, `getsplinepoints`, `makeSelfEdge` + `selfRight/Left/Top/Bottom`, `place_portlabel`, `makePortLabels`, `edgeMidpoint`, `updateBB`; `emit.c` `update_bb_bz` |
| `common/routespl.rs` | `common/routespl.c`: `routesplines_`, `checkpath` (incl. C's stale-tail quirk after removing degenerate boxes), `limitBoxes`, `overlap`, `simpleSplineRoute`, `makeStraightEdges`, `bend`; `path`/`pathend_t` |
| `common/ports.rs` | `common/shapes.c`: `chkPort`, `poly_port`, `record_port`, `compassPort`, `compassPoint`, `closestSide`, `resolvePort`, `invflip_side/angle`; `geom.c` `cwrotatepf`/`ccwrotatepf` |
| `common/shapes.rs` | `insidefn`s incl. `poly_inside`'s `scalex/scaley` and `fixedsize=shape` (`polyBB`) |
| `common/arrows.rs` | `arrows.c` incl. `arrowOrthoClip` |
| `common/xlabels.rs` | `postproc.c` `addXLabels` + `lib/label` |
| `ortho/` | `lib/ortho` (all files) |
| `cdt.rs` | `lib/cdt` ordered set (`dttree.c`, `dtflatten.c`, `dtrestore.c`) |

### Data model

* Arenas on `Dot`: `nodes`, `edges`, `graphs`, `labels`, plus `bufs` (storage of
  `rank_t.v`/`av`). Real nodes/edges keep their input index; virtual ones are
  appended. Graph 0 is the root, then input subgraphs in creation order; dot's
  private cgraphs (newrank `Xg`, `checkLabelOrder`'s `lg`, `_new_rank`) are
  extra graphs in the same arena.
* `Rank { vbuf, voff, n, avbuf, an, ... }`: in C cluster rank arrays alias windows
  of the root arrays (`GD_rank(subg)[r].v = GD_rank(root)[r].v + pos`), and the
  root window moves per component (`init_mccomp`). `rv(g, r, i)` reads
  `bufs[vbuf][voff + i]`. `GD_rank(g)[r]` is stored at index `r + 1` because
  `abomination` creates rank -1.
* cgraph iteration order: nodes by sequence (= index); **out-edges of a node by
  (head sequence, edge sequence) and in-edges by (tail sequence, edge sequence)**
  (`agedgeseqcmpf`), subgraphs by creation order. `NodeInfo.cg_out/cg_in` hold
  these orders; subgraph membership is a `BTreeSet` per graph.
* Only the root has `rankdir` (`GD_flip(subg)` is false in C) and `nodesep`
  (`GD_nodesep(subg)` is 0, so cluster virtual nodes are not widened).
* `free_matrix` in `cleanup2` leaves a dangling non-NULL pointer that
  `flat_edges` tests; the port keeps the matrix to preserve that truthiness.

## Phase 2: edge routing (done)

`dot_splines_` is a direct port: edges are collected per rank (normal nodes and
virtual merge points: `ND_out`, `ND_flat_out`, `ND_other`; `rw`/`mval` swap),
classified with `setflags`, sorted with `edgecmp` through the Apple `qsort`, then
grouped into equivalence classes (`getmainedge` + ports, `portcmp` on
`makefwdedge` copies). C's temporary edge copies (`makefwdedge`, the
`fwdedgea/b` pairs of `make_regular_edge`) are pushed on the edge arena and
truncated when `dot_splines_` returns.

* Regular edges: `make_regular_edge` with `maximal_bbox`/`neighbor`/
  `pathscross`/`cl_bound`/`rank_box`, straight-segment mode (`straight_len`,
  `straight_path`), `completeregularpath`/`adjustregularpath` (size_t wrap
  arithmetic kept), `recover_slack`/`resize_vn`, multi-edge spreading by
  `Multisep`, `splines=line` (`makeLineEdge`) and `polyline`.
* Flat edges: `make_flat_edge`, `make_flat_labeled_edge`,
  `make_flat_bottom_edges`, `makeFlatEnd`/`makeBottomFlatEnd`, `makeSimpleFlat`,
  `makeSimpleFlatLabels` (`edgelblcmpfn`), and `make_flat_adj_edges` with ports,
  which lays the two nodes out in a nested `Dot` on a cloned graph (rankdir
  turned, `setState`'s ignored attributes stripped, `rank=source` subgraph,
  heavy edge `weight=10000`) and copies the splines back with `transformf`.
* Self loops (`makeSelfEdge`), `splines=curved` (`makeStraightEdges` with the
  cycle centroid), `splines=none|""`, `splines=ortho` (`lib/ortho`, with
  `setEdgeLabelPos`), edge labels (`place_vnlabel`, flat label vnodes), port
  labels (`place_portlabel` when `labelangle`/`labeldistance` exist, else
  `addXLabels`), `edge_normalize`/`swap_spline`.
* `conc.c` (from `dot_position`) and `compound.c` (after splines) run where C
  runs them. `GD_bb` grows with every installed Bézier (`update_bb_bz`) and
  label, so the translation offsets of phase 1 are gone.

Validation beyond the corpus: `splines=true|line|polyline|curved|none|false` ×
`rankdir=TB|LR|BT|RL`, `concentrate=true` and `splines=ortho` over
`graphs/directed`, `graphs/undirected` and `tests/graphs` of the 16.1.0 sources
match C (pos/lp/head_lp/tail_lp/xlp within 2pt) except where C itself exceeds a
60 s limit (curved and ortho on the largest graphs) and `b100.gv`, where C's
`%.5g` output rounding of coordinates above 10^5 is the only difference.

## Known deviations / unported

* `make_flat_adj_edges` with records returns without splines, as C (it only
  warns).
* `record_path` when no top-level field contains the port: C returns `side`
  with stale boxes; the port falls back to the node box.
* `boxIntersectf` failure (C asserts) returns the segment start.
* `dot_concentrate` failures (C aborts the layout with "concentrate=true may not
  work correctly") are ignored.
* `aspect.c` is not ported (16.1.0's `setAspect` only warns).
* Not ported (phase 3): `pack`/`packmode` component packing in `doDot`.
* `agcontains` of virtual nodes is false (they have ID 0 in C), as in C.

## Validation

* `cargo run --release -p gv-diff -- --baseline tests/golden/baseline.json`.
  At the switch to dotgen: nodes ok 73.5% (old engine 14.7%), edges 22%, bb 84%.
  After phase 2: exact 499/508, nodes ok 99.1%, edges ok 99.6%, bb ok 98.2%,
  clusters 98.9%, score 98.8%. The dot files left are the four `pack` files;
  the others are non-dot engines (fdp, osage).
* State dumps: `GVDUMP=1 dot-rs -Tdot f.gv` prints rank (`R`), mincross order
  (`M r: ...`), aux graph (`A idx name r= o= t= rw= lw=: head/minlen/weight`),
  final x (`P`), TB_balance (`TBPRE`, `TBORDER`, `TB`), decompose (`SC`) and label
  vnodes (`LV`). An instrumented C build with the same dumps lives in
  `~/src/gv-ref/build` (configure + make of the 16.1.0 sources with the
  `GVDUMP` prints in `dotinit.c`, `ns.c`, `decomp.c`, `position.c`, `class2.c`):
  run it with `GVDUMP=1 GVBINDIR=~/src/gv-ref/build/inst/lib/graphviz
  ~/src/gv-ref/build/inst/bin/dot -Tdot f.gv` and `diff` the two stderr outputs.
* Spline dumps (`GVDUMP=1`, both sides, `%.6f`): `RS boxes` (after `checkpath`),
  `RS ends` (start/end point, theta, constrained), `RS poly`, `RS sp`
  (`Pshortestpath`), `RS out` (`Proutespline`/`make_polyline`), in
  `routesplines_`. Ortho: `OE` (sorted edges), `OR` (routes), `OAS`
  (segment → channel), `OCG np|x` (channel graphs), `OPE`/`OSP`
  (parallel-segment decisions), `OT` (tracks). The C side is in
  `~/src/gv-ref/build/src/lib/common/routespl.c` and `lib/ortho/ortho.c`; the
  instrumented build now has expat (HTML labels).
* Note: C's `-Gphase=2` `order` attributes do not reflect the final in-rank
  order; compare `M` dumps instead.

## Handoff: phase 3

* `pack`/`packmode` in `doDot`: `cccomps` (components respecting clusters),
  `dotLayout` per component subgraph (`initSubg` copies nodesep/ranksep/rankdir),
  `attachPos`/`packSubgraphs`/`resetCoord`, `copyClusterInfo`. `crate::pack`
  (from neato) has `packSubgraphs`; the four corpus `pack` files need this.
* `lib/ortho` is generic (`ortho::OrthoGraph`); neato-family `splines=ortho`
  could use it.
* `GV_DOT_LEGACY=1` / `dot.rs` (old approximate engine) can be deleted once
  nobody compares against it.
