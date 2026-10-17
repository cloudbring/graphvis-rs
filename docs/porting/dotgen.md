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
| `init.rs` | `dotinit.c` (`dot_init_subg`, `dot_init_node_edge`), `common/input.c` (`graph_init`, `do_graph_label`), `common/utils.c` (`common_init_edge`), `common/shapes.c` (`poly_port`, `record_port`, `compassPort`, `invflip_*`) | ported; node and label sizes come from `gv_geom::node_geometry` / `edge_label_geometry` / `graph_label_geometry` |
| `utils.rs` | `common/utils.c` (`late_int`, `late_double`, `mapBool`, `UF_*`, `is_a_cluster`), `class1.c` (`nonconstraint_edge`), `util/gv_math.h` (`scale_clamp`) | ported |
| `qsort.rs` | Apple Libc `stdlib/FreeBSD/qsort.c` + `heapsort.c` | exact port of the libc the goldens were produced with (qsort is unstable; tie order leaks into `TB_balance`, `medians`, `restore_best`, `do_ordering_node`). Verified against recorded system permutations (`qsort_vectors.txt`) |
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
| `sameport.rs` | `sameport.c` | ported; `shape_clip` approximated with `gv_geom::clip_ray_to_node_boundary` |
| `postproc.rs` | `common/postproc.c` (`gv_postprocess`, `place_graph_label`, `place_flip_graph_label`, `place_root_label`, `translate_drawing`, `translate_bb`, `map_point`), `dotinit.c` (`removeFill`) | ported except `addXLabels` |
| `output.rs` | phase-1 stand-in for `dotsplines.c` | temporary routing + write-back into `gv_core::Graph` (see below) |

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

## Known deviations / unported

* **Splines (phase 2)**: `output.rs::phase1_splines` routes each real edge with
  `gv_geom::route_edge_spline` and bends it through the virtual node chain
  (Catmull-Rom). Only self-loop splines and edge labels grow `GD_bb`, because the
  approximate router overshoots where C stays inside the node boxes. This is the
  dominant remaining error: on the corpus 2143 of the 2286 misplaced nodes are
  exactly right up to a translation, i.e. only C's spline extents (left/right
  loops, flat edges, self loops with labels, ports) differ.
* Edge label positions: chain labels use `place_vnlabel`; non-adjacent flat
  labels use the label vnode centre (C refines in `make_flat_labeled_edge`);
  others use the router's guess. Head/tail labels are not placed by layout.
* Graph and cluster label `lp` are still computed by the renderer
  (`gv-render/src/dot_out.rs`), not from `TextLabel.pos` (wrong for
  `labeljust`/`labelloc` in flipped graphs).
* `compassPoint` (non-box compass ports) and `shape_clip` use
  `clip_ray_to_node_boundary` instead of `bezier_clip` with the shape's inside
  function; HTML port boxes come from `gv_geom::html_table_layout`.
* Not ported (phase 3): `conc.c` (concentrate), `compound.c` (`compound=true`),
  `aspect.c`, `pack`/`packmode` component packing in `doDot`, `addXLabels`,
  `clusterrank=global` is wired but untested.
* `agcontains` of virtual nodes is false (they have ID 0 in C), as in C.

## Validation

* `cargo run --release -p gv-diff -- --baseline tests/golden/baseline.json`.
  At the switch: nodes ok 73.5% (old engine 14.7%), ranks ok 493/508 (460),
  order ok 493/508 (360), clusters 82%, bb ok 84%, edges 22%. The rank/order
  misses left are the four `pack` files.
* State dumps: `GVDUMP=1 dot-rs -Tdot f.gv` prints rank (`R`), mincross order
  (`M r: ...`), aux graph (`A idx name r= o= t= rw= lw=: head/minlen/weight`),
  final x (`P`), TB_balance (`TBPRE`, `TBORDER`, `TB`), decompose (`SC`) and label
  vnodes (`LV`). An instrumented C build with the same dumps lives in
  `~/src/gv-ref/build` (configure + make of the 16.1.0 sources with the
  `GVDUMP` prints in `dotinit.c`, `ns.c`, `decomp.c`, `position.c`, `class2.c`):
  run it with `GVDUMP=1 GVBINDIR=~/src/gv-ref/build/inst/lib/graphviz
  ~/src/gv-ref/build/inst/bin/dot -Tdot f.gv` and `diff` the two stderr outputs.
* Note: C's `-Gphase=2` `order` attributes do not reflect the final in-rank
  order; compare `M` dumps instead.

## Handoff: phase 2 (splines)

Port `dotsplines.c`, `common/splines.c` (clip_and_install, beginpath/endpath,
makeSelfEdge, selfRightSpace already in `position.rs`), `common/routespl.c` and
`lib/pathplan`, replacing `output.rs::phase1_splines`:

* Everything `dot_splines` reads is in place: `GD_rank` arrays with final
  `coord`, `ND_lw/rw/ht` (call `reset_rw` first, as C's `resetRW`), virtual node
  chains (`ED_to_virt`, `ND_out(vn)[0]`), `ED_edge_type`, `ED_tree_index`-free
  edges, `ND_other` lists (merged multi-edges, self loops), `ND_flat_out` /
  `ED_adjacent` / `ED_dist` / label vnodes with `ND_alg = e`, ports
  (`Port.p/theta/side/constrained/clip/dyna/bp`), cluster `GD_bb`, `GD_ln/rn`,
  `rank_t.ht1/ht2`, `GD_ranksep/nodesep`, labels (`TextLabel.dimen/pos/set`).
* Store results in `Dot.spl[e]` (layout space, `Vec<Bezier>` with `sp`/`ep`)
  and update `graphs[0].bb` with `update_bb_bz`/label boxes (helpers exist in
  `output.rs`); `dotneato_postprocess` then maps them like `map_edge`.
* `removeFill` and `dot_sameports` already run before the splines step.
* Remove the self-loop-only bb restriction once routing is faithful.

## Handoff: phase 3

* `conc.c` (`dot_concentrate`, called from `dot_position` when `Concentrate`;
  class2 already marks `IGNORED`/`conc_opp_flag`).
* `compound.c` (`dot_compoundEdges` after splines when `compound=true`).
* `pack`/`packmode` in `doDot` (ccomps + `packSubgraphs`), `aspect.c`.
* `addXLabels` (xlabels placement, `lib/label`).
* Renderer: take graph/cluster `lp` from layout.
