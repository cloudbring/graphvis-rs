# circo and twopi ports (Graphviz 16.1.0)

`layout=circo` / `-Kcirco` and `layout=twopi` / `-Ktwopi` now run
function-by-function ports of `lib/circogen` and `lib/twopigen`. The old
approximations (`force_circular::layout_circo`, `layout_twopi`) are removed.

Entry points: `gv_layout::circogen::circularinit::circo_layout(&mut Graph)` and
`gv_layout::twopigen::twopiinit::twopi_layout(&mut Graph)`.

Both reuse the shared neato-family infrastructure (`common::LGraph`, `pack`,
`neatogen::adjust::adjust_nodes`, `neatogen::neatosplines::spline_edges`,
`common::postproc::gv_postprocess`, `cutil::qsort_by`) as C does.

## Fidelity

`gv-diff` against C 16.1.0 goldens (built-in text metrics, see
`scripts/gen-golden.sh`):

| corpus | before (force_circular) | after |
|---|---|---|
| `tests/circo` (280 files) | 70 exact, score 100.3/280 (35.8%), 78/5128 nodes | 279 exact, score 279.6/280, 5128/5128 nodes, 6816/6818 edges |
| `tests/twopi` (287 files) | 89 exact, score 120.8/287 (42.1%), 326/5334 nodes | 280 exact, score 286.9/287, 5334/5334 nodes, 7004/7014 edges |
| main corpus (508 files) | 250 exact, 408.9 | no regressions; the shared fixes below add exact files |

`-Tdot` output is byte-identical to C on 279/280 circo and 279/287 twopi files.

Remaining differences, all in shared code:

- `splines=ortho` now routes through `lib/ortho` (see `neato.md`); circo's
  arrows graphs with `-Gsplines=ortho` (4 files) still differ by one track
  position.
- twopi `splines=true` on the arrows graphs (7 files): `i`, `Z` and `I` are
  exactly vertically aligned, and which side the pathplan router passes an
  obstacle vertex on is a floating-point tie. The Homebrew build (the goldens),
  the local C build in `~/src/gv-ref/build-circo` and the port each break it
  differently, so it is not a port bug
  that can be fixed against the goldens.

## Module ↔ C file map

### `gv_layout::twopigen`

| Rust | C |
|---|---|
| `circle::circle_layout` | `circleLayout` |
| `circle::{init_layout, is_leaf, set_n_steps_to_leaf, find_center_node}` | `initLayout`, `isLeaf`, `setNStepsToLeaf`, `findCenterNode` |
| `circle::{set_n_steps_to_center, set_parent_nodes, set_subtree_size}` | `setNStepsToCenter` (BFS, skips `weight="0"`), `setParentNodes`, `setSubtreeSize` |
| `circle::{set_subtree_spans, set_child_subtree_spans, set_positions, set_child_positions}` | same names |
| `circle::{get_rankseps, set_absolute_pos}` | `getRankseps` (`ranksep` list, `MIN_RANKSEP`), `setAbsolutePos` |
| `circle::RData` | `rdata` (`ND_alg`) |
| `twopiinit::twopi_layout` | `twopi_layout`, `twopi_init_graph`, `findRootNode` |

### `gv_layout::circogen`

| Rust | C |
|---|---|
| `circularinit::circo_layout` | `circo_layout`, `circo_init_graph` |
| `circularinit::circo_layout_main` | `circoLayout` |
| `circularinit::{derived_graph, component}` | `circomps`, `makeDerivedNode` |
| `circularinit::copy_posns`, `pack_derived` | `copyPosns`, `packSubgraphs` on the derived graph |
| `circular::circular_layout` | `circularLayout` |
| `circular::{init_graph_attrs, create_one_block}` | `initGraphAttrs`, `createOneBlock` |
| `circular::{CData, Circ}` | `cdata` (`ND_alg`), `circ_state`, derived `ND_pos` |
| `blocktree::{dfs, find_blocks, create_blocktree}` | same names (`addNode`, `makeBlock` inlined) |
| `blockpath::layout_block` | `layout_block` |
| `blockpath::{clone_graph, get_list, cmp_degree, find_pair_edges, remove_pair_edges}` | same names (`deglist` is the qsorted list) |
| `blockpath::{spanning_tree, dfs, find_longest_path, measure_distance}` | same names |
| `blockpath::{place_residual_nodes, place_node, block_graph, largest_nodesize}` | same names |
| `blockpath::{count_all_crossings, reduce, reduce_edge_crossings}` | same names |
| `circpos::{circ_pos, do_block, position, position_children, get_info, set_info, get_rotation, apply_delta}` | `circPos`, `doBlock`, … |
| `block`, `nodelist`, `edgelist` | `block.c`, `nodelist.c`, `edgelist.c` |
| `graph::{CGraph, EdgeArena}` | the cgraph operations circo uses (see below) |

`deglist.c` no longer exists in 16.1.0; its job is the qsorted `deglist_t` in
`blockpath.c`.

## Notes and deliberate modelling choices

- **Derived graph.** circo works on a strict, undirected, loop-free copy of
  the graph. `graph::CGraph` models it and every graph built from it (block
  subgraphs, the `_clone_` subgraph and root copy, the spanning tree) with
  cgraph's iteration order: nodes by sequence, `agfstout` by head sequence,
  `agfstedge` = out-edges then non-loop in-edges. Node ids are original node
  indices (derived and clone nodes are created in that order).
- **`cdata` union.** The union members are separate fields; each pass writes
  its fields before reading them, so the aliasing is unobservable.
- **Edge halves in `count_all_crossings`.** C keys the open-edge set by
  `Agedge_t *`; an edge opened from its tail (out-half) is never found when
  removed from its head (in-half), so closed edges stay open and keep being
  counted. `edgelist` keys by `(edge, half)` to reproduce the counts, which
  drive crossing reduction.
- **Pointer comparisons.** `find_pair_edges` counts each neighbour pair once
  via `n1 < n2` on addresses; any strict order gives the same count and
  deletes the same skeleton edge, so node ids are compared.
- **Packing.** C packs the derived graph's components. The port packs the
  original nodes with the adjacency temporarily reduced to the derived edges
  (first-seen orientation, no loops or duplicates), which gives the same
  polyomino cells. `adjustNodes(sg)` on derived components is a no-op in C
  (the derived graph declares no `overlap`), and so in the port.
- **twopi `root`.** `root=""` or an unknown name sets `root` to the chosen
  centre in the output (also in the root attribute dictionary, as `agset`
  does); a declared node `root` attribute is set to `1` on chosen centres.
  The C crash on a node `root` attribute with isolated nodes is avoided in
  the corpus, not reproduced.
- **Messages.** C's "use of weight=0 creates disconnected component" error
  is not printed; positions stay at the origin as in C.

## Shared fixes made on this branch

- `gv-geom` `poly_init`: the polygon vertex walk and `1 - SQR(r)` use the
  fused multiply-adds clang emits (checked against the libgvc disassembly);
  `ND_ht` was one ulp off for shapes like `invhouse`, moving pack cells.
- `neatogen::delaunay`: exact-sign `orient2d`/`incircle`
  (`neatogen::robust`: Shewchuk's float filter, exact big-integer fallback)
  and GTS's `gts_triangle_enclosing` triangle. The inexact Bowyer–Watson
  produced non-planar edge sets on co-circular circo/twopi positions, so
  prism overlap removal diverged.
- `common::shapes` `record_inside` clips against the record root field box,
  not `ND_ht` (which carries `record_init`'s `+1`).
- `LGraph::write_back` hands placed graph/cluster label positions to the
  renderer (fixes `lp` for `rankdir` graphs in all neato-family engines) and
  marks the graph so `-Tdot` does not apply dot's `mark_lowclusters` edge
  placement.

## Validation

- `cargo run --release -p gv-diff -- --golden tests/golden/circo --baseline tests/golden/circo-baseline.json tests/circo`
  and the same for `twopi` (ratchet tests: `crates/gv-diff/tests/circo_fidelity.rs`,
  `twopi_fidelity.rs`).
- Corpora: `scripts/gen-circo-corpus.py` and `scripts/gen-twopi-corpus.py`
  force `layout=` on the fixtures and undirected wild graphs and add
  hand-written cases: trees, cycles, complete graphs, biconnected blocks, cut
  vertices, chains of cycles, disconnected graphs with `pack`/`packmode`,
  `root` (graph and node), `ranksep` lists, `mindist`, `oneblock`, `overlap`,
  `splines`, weights (`weight=0` for twopi), loops and multi-edges.
  Goldens: `scripts/gen-golden.sh` writes `tests/golden/{circo,twopi}/`.
- An instrumentable C copy is in `~/src/gv-ref/build-circo` (`run.sh`; set
  `CIRCO_DBG=1` for block, crossing, pack and triangulation traces).
