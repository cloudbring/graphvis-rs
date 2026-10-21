# Differential fuzzing against C Graphviz 16.1.0

The corpus ratchets (`gv-diff`, `gv-diff e2e`) only cover inputs someone thought of. The
fuzzer generates random DOT inputs, runs C `dot` and the port on each, and minimises
every difference to a small reproducer. Each bug it found was fixed against the C
source, and one reproducer per bug is kept in the corpus (`tests/wild/fuzz_*.gv`,
`tests/<engine>/fuzz_*.gv`, with C goldens and e2e hashes).

## Running it

```sh
cargo build --release -p graphviz-pure-cli --bin graphviz-pure && cargo build -p graphviz-pure-cli --bin graphviz-pure
python3 -I scripts/fuzz-run.py run --seed 1 --count 2000 --out /tmp/fuzz/r1   # parallel
python3 -I scripts/fuzz-run.py summary /tmp/fuzz/r1
python3 -I scripts/fuzz-run.py min /tmp/fuzz/r1/diff/s17.gv -o /tmp/fuzz/s17.min.gv
python3 -I scripts/fuzz-run.py check -q /tmp/fuzz/r1/min/*.gv                   # re-check
python3 -I scripts/fuzz-gen.py --seed 17                                        # one case
```

* `scripts/fuzz-gen.py` is the generator. A case is a pure function of its seed (and
  optional `--engine`, `--malformed` rate), so a seed is a complete bug report. It draws
  the engine (dot 30%, then neato, fdp, sfdp, twopi, circo, osage and patchwork), a size
  class (1–300 nodes), a topology (tree, DAG, cycle, random, dense, chain, star, grid,
  disconnected, bipartite, multi-edges, plus self-loops and duplicates), strict or not,
  directed or not, nested, empty and overlapping clusters, `rank=` sets, chains and
  subgraph edge endpoints, and attributes from the Graphviz attribute table. The
  attributes cover all shapes (records and Mrecords with ports, HTML tables with
  ports/spans/fonts), compass ports, labels and xlabels, head and tail labels, `\N \G \E
  \T \H \L` escapes, multi-line and Unicode text, fonts, colors (lists, weighted lists,
  HSV, schemes), styles, arrows, `dir`, `penwidth`, URLs, ids and classes. Per-engine
  graph attributes include rankdir, nodesep/ranksep, splines, concentrate, compound and
  lhead/ltail, ordering, newrank, pack/packmode, size/ratio/margin/pad/rotate,
  overlap/sep/mode/model/start, K, mindist, root, area and sortv. About 8% of cases are
  mutated into malformed input: dropped or stray braces and tokens, truncation, bad
  attribute values, bad HTML, bad records, wrong edge operators, empty files.
  `start=random` is always seeded, and fdp never gets `start=regular|self`, because C
  seeds those from `time(NULL)`.
* `scripts/fuzz-run.py` runs C (`dot -Tsvg`, `GVBINDIR=~/src/gv-ref/lutplugins`) and both
  port builds (release, and debug for overflow checks) with timeouts (30 s for C, longer
  for the port). It compares stdout minus the generator comment, and whether the exit
  status is zero, and records a stderr difference as a side note. A difference is
  re-run under C to rule out C instability. Outcomes: `same`, `svg_diff`, `exit_diff`,
  `port_crash`, `port_timeout`, `debug_diff` (the debug build panicked or disagreed),
  `c_unstable`, `c_timeout`, `c_crash`. Actionable cases are copied to `OUT/diff/`.
* `min` delta-debugs a case while its outcome stays the same. It drops whole
  subgraphs, then chunks of statements (ddmin), then subgraph braces, then single
  attributes.

## Rounds

Identity is the share of comparable cases (excluding `c_unstable`, `c_timeout` and
`c_crash`) whose stdout and exit status match C.

| Round | Seeds | Cases | dot | neato | fdp | sfdp | twopi | circo | osage | patchwork | all |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 0 (pilot) | 1–200 | 200 | 50.8 | 69.2 | 54.2 | 40.0 | 71.4 | 62.5 | 85.7 | 26.3 | 55.2 |
| 1 | 1–2000 | 2000 | 88.5 | 92.0 | 81.9 | 94.3 | 91.3 | 95.5 | 91.2 | 99.4 | 90.9 |
| 2 | 10001–13000 | 3000 | 96.3 | 98.9 | 95.2 | 99.2 | 99.7 | 98.6 | 100 | 100 | 98.1 |
| 3 | 20001–25000 | 5000 | 96.8 | 98.6 | 97.3 | 99.2 | 99.6 | 99.2 | 99.8 | 100 | 98.4 |
| 4 | 30001–36000 | 6000 | 98.5 | 98.8 | 96.1 | 99.8 | 99.5 | 99.5 | 99.8 | 100 | 98.8 |

Each round ran on the fixes from the rounds before it. Rounds 3 and 4 also hit inputs
that C crashes on (61 and 55 cases), mostly `SIGSEGV` in dot with nodes in two
non-nested clusters, plus assertions in neato/sfdp/fdp (`bitarray_get` out of bounds,
`gv_list_get_` index, `StressMajorizationSmoother2_new: nz > 0`) and twopi's
`splines.c:1277: claimed unreachable code`. Those cases are skipped.

### After all fixes

All 382 actionable cases from rounds 1–4 (from `OUT/diff/`, not minimised) were
re-checked with the final binaries:

| | count |
|---|---:|
| now identical | 256 |
| now `c_unstable` or `c_crash` on re-run | 16 |
| still different (106 SVG, 3 exit status, 1 port panic) | 110 |

How the remaining 110 break down, by input feature:

| Feature | Cases | Status |
|---|---:|---|
| node in two non-nested clusters (any engine) | 14 | C error path, see below |
| `pack`/`packmode` (dot 13, fdp 9, neato 8, sfdp 1, osage 1) | 32 | mostly with clusters or unplaced labels, see below |
| `start=random`/`regular` from the first generator version (rounds 1–2) | ~8 | time-seeded in C |
| circo/neato `splines=spline|true` with record ports | ~12 | open, see below |
| neato/fdp/twopi/circo overlap removal (`ortho`, `scale`, `ipsep`, `vpsc`, ...) | 11 | open |
| dot samehead/sametail, concentrate and others | 14 | open |
| fdp, sfdp and twopi others | 19 | open; fdp mostly clusters (C unstable run to run) |


## Bugs found and fixed

Parser and input
* An edge's `tailport`/`headport` attribute wins over the `n:port` endpoint, which wins
  over an edge default (grammar.y `newedge`: `mkport` before `applyattrs`); an existing
  reversed undirected edge takes the ports swapped. This was the largest single class
  in round 0.
* A repeated edge of a strict graph takes only the statement's attributes, not the
  defaults again.
* Attributes of a list are declared in source order, which fixes cgraph's `sym->id`.
* An attribute list holds `atom=atom` items with at most one separator each.
* An unquoted keyword (`node`, `edge`, `graph`, `digraph`, `subgraph`, `strict`, in any
  case) is not an `atom`, so as an attribute name or value, or as a port or compass
  point, it is a syntax error, as in C (`grammar.y` `atom : T_atom | qatom`). The quoted
  forms still parse. This was open case `020-parser-keyword-as-attribute-value`; its
  checks are in `crates/graphviz-pure-cli/tests/cli.rs`
  (`unquoted_keyword_atoms_are_syntax_errors`, `quoted_keyword_atoms_still_parse`) and
  the parser's unit tests.
* cgraph reads input a line at a time and each graph gets a new scanner: the rest of
  the line after a graph's closing `}` is dropped, line numbers restart, and the graphs
  read before a syntax error are rendered before dot exits 1.
* A graph attribute first declared in a subgraph has value `""` at the root (so
  `splines` set in a subgraph turns the root's edges off).
* Graphs without nodes keep their label (dot postprocesses them; twopi, circo and
  patchwork leave labels at the origin).

Errors and exit status
* `graphviz_pure_core::diag` keeps C's error level: any `agerrorf` makes the exit status 1 while
  the output is still written. It is reported where C reports it: routesplines and
  checkpath failures, lost edges, fdp's non-comparable clusters, bad record labels,
  rejected HTML labels (expat's messages with `error_context` and `in label of ...`;
  yacc's syntax error wins over a later lexer error, as C lexes on demand), neato
  positions that are not two doubles, `trouble in init_rank`, `contain_nodes`, and
  concentrate's `rebuild_vlists`. Pathplan's `prerror` lines, textspan_lut's metric
  fallback warnings, `addXLabels`' count warning and `deriveClusters`' message are
  printed too.
* A DOT syntax error reads as scan.l's `aagerror` writes it: `syntax error in line N near
  'TOKEN'` (the rejected token's `yytext`: `"` for a quoted string, `>` for an HTML string),
  or, at end of input, nothing or the scanner state (`scanning a quoted string ...` with the
  first 80 bytes of the string, HTML string, comment). N is the line where the rejected
  token ends, renumbered by `# N "file"` directives, which also rename the file. The lexer
  no longer fails: an unmatched character is a token the grammar rejects, `@` ends the
  input, and an unterminated string or comment ends it in that state. This was open case
  `022-stderr-syntax-error-message` (issue #31); the tests are in
  `crates/graphviz-pure-parser/tests/syntax_errors.rs`. Not yet ported: `chkNum`'s
  "badly delimited number" warning.
* A concentrate failure, or an `install_in_rank` failure, abandons the layout as in C:
  no splines, no postprocessing. C then renders the half-made drawing (zero bounding
  box, empty cluster boxes, labels at the origin), and so does the port.

Labels, HTML, records
* `\L` in xlabels, head/tail labels and record fields is the object's main label text,
  empty if that label is empty.
* An HTML table without `COLOR` takes the object's `pencolor`, else `color`, before
  positioning, so cells inherit it.
* An HTML text label's `lp->text` (the default tooltip) is its source string, with
  entities undecoded.
* An HTML label's `space` starts at (0, 0); with `fixedsize=shape` it moves labelloc=t/b
  labels.
* A record label is HTML exactly when it is an HTML string. A record field's sides come
  from its position in the record, not from geometry, so a port touching all four
  sides resolves to the field centre.
* A node with an empty name has an empty `\N`.
* `placeLabels` loads the R-tree in the order of C's `Dtobag` (the `cutil::DtTree` splay
  tree), not a sort: a duplicate Hilbert key lands just before the equal element the splay
  search stops at, which is not always reverse insertion order. This was open case
  `009-xlabels-dtobag-equal-key-order` (7 neato fuzz cases); its repro is kept in
  `crates/graphviz-pure-cli/tests/cli.rs` (`xlabels_equal_hilbert_keys_follow_dtobag_order`).

Rendering
* A color list whose first entry is a known SVG name is written out whole
  (`stroke="red:blue"`), because `gvrender_resolve_color` keeps the caller's pointer.
* HSV colors are read with `sscanf` semantics, so a weighted entry such as
  `0.1,0.9,0.7;0.3` resolves.
* `outputorder` carries over to later graphs of the same run (job flags are ORed).
* NaN prints as `nan`, as macOS libc does.
* Tapered edges subdivide with clang's fused multiply-add.
* `round_corners` shapes (rounded, diagonals, note, tab, folder, box3d, component, the
  arrows and SBOL shapes) interpolate their corner points with clang's fused
  multiply-add, as `alloc_interpolation_points` does. Was open case
  `018-render-interpolate-points-fma` (issue #27); its repro is kept in
  `crates/graphviz-pure-cli/tests/cli.rs` (`round_corners_interpolation_is_fused_like_clang`).
* `\E` uses the layout's final port names, which dot's `sameport` clears.

dot
* A lost edge (routing failed) still draws its placed label.
* With `splines=curved`, a node with self-loops is drawn with the loop space: a double
  `ND_rw`/`ND_mval` swap.
* `fixedsize=shape` clipping swaps extents under a flipped rankdir.
* `GD_ranksep(g)/2` in beginpath/endpath is integer division.
* The nested layout of adjacent flat edges with ports or labels (`make_flat_adj_edges`)
  now matches C:
  * `setState` leaves `N_penwidth`, `E_arrowsz` and `E_penwidth` pointing at the main
    graph's symbols, whose ids select another attribute of `auxg` (ProtoGraph's
    `label`, then the main attributes in name order, then the ports and `weight`).
  * `N_fixed` is looked up as `fixed`, while `arrowhead`/`arrowtail` are found by name.
  * The heavy edge takes the default ports.
  * `quantum` and `charset` are copied to `auxg`.
  * An `n:port` endpoint declares the `tailport`/`headport` edge attribute when the edge
    is made (grammar.y `mkport`), so the symbol ids those stale lookups use count it.
    This was open case `007-dot-port-endpoint-attr-sym-id` (issue #16); its repro is kept
    in `crates/graphviz-pure-cli/tests/cli.rs`
    (`port_endpoint_declares_its_edge_attribute_like_mkport`). 9 of its 11 fuzz cases now
    match C. Seeds 10107 and 33649 still differ in one flat-edge spline (C warns
    `Unable to reclaim box space` on 33649), a separate cause: issue #84.
  * `E_label_float` is looked up as `label_float`, so `labelfloat` is ignored there (was
    open case `012-dot-flat-adj-labelfloat`, issue #21).
* In the pack branch, edges are mapped only when a component routed splines.
* An edge without a spline keeps its unset label's position (moved by `shiftEdge` in
  the pack branch), which `edge_in_box` tests without looking at `set`: with `pack` and
  `splines=none` (or `splines=""`) the label is off the page and the edge is not emitted,
  and `-Tplain` writes the shifted position. This was open case
  `015-dot-pack-unplaced-edge-label-shifted-off-page` (issue #24, 6 fuzz cases); its repro is
  kept in `crates/graphviz-pure-cli/tests/cli.rs`
  (`dot_pack_unplaced_edge_label_off_page_skips_edge`).
* A pack component is not the root for `is_a_cluster`, so newrank ignores its top-level
  sink/source sets.
* A pack component whose layout fails (`install_in_rank`) still hands its node and edge
  records to the root, so the abandoned drawing has C's node sizes; components are named
  `_cc_<i>` in messages, and `mark_clusters` prints C's "already in a rankset" warning.
  This was open case `014-dot-pack-failed-component-not-absorbed`; its repro is kept in
  `crates/graphviz-pure-cli/tests/cli.rs` (`failed_pack_component_keeps_its_node_sizes`).
* In a pack component, `clip_and_install` and `addEdgeLabels` grow the root's bounding
  box (`agraphof(tail)`), not the component's, so a later curved edge bends towards the
  component's box without the earlier splines (open case
  `017-dot-pack-curved-edge-subpoint`, issue #26).
* A pack component shares the root's attribute dictionaries (it is an `agsubg` of the
  root), so `make_flat_adj_edges`' stale symbols resolve to the same ids as without pack.
  This was open case `013-dot-pack-component-missing-attr-dicts` (seed 23917); its repro is
  in `crates/graphviz-pure-cli/tests/cli.rs` (`pack_component_keeps_root_attribute_dictionaries`).
* With `pack` and one component (or a `ratio`), the root is laid out while the `_cc_<i>`
  subgraphs `cccomps` made are still its children, as in C's `doDot`: they inherit the
  root's `rank` and hold clones of the clusters. This was open case
  `010-dot-pack-single-component-ignores-cc-subgraph` (issue #19); its tests are
  `pack_single_component_*` in `crates/graphviz-pure-cli/tests/cli.rs`. `dot_mincross`
  now prints C's "removing empty cluster" warning.
* In the pack branch, clusters are drawn from the root's `GD_clust` as `copyClusterInfo`
  builds it: in component order, and once per component that kept the cluster, so a
  cluster sharing a node with a non-nested cluster is drawn twice. The renderer used to
  rebuild the list from the subgraph tree. This was open case
  `016-dot-pack-cluster-list-from-subgraph-tree` (issue #25); its two reproducers are in
  `crates/graphviz-pure-cli/tests/cli.rs`
  (`packed_dot_draws_the_root_cluster_list_of_copy_cluster_info`). Four of its five fuzz
  cases now match C. Seed 1548 still differs: its nodes of two non-nested clusters are in
  two components, so the components' boxes take in each other's edges (issue #100) and
  share node and edge records in C (issue #101). Seeds 1062, 11551 and 35883 still write
  some edges in a different subgraph in `-Tdot` (issue #102).

neato family
* `resolvePorts` drops `tailclip=false` on `_` ports.
* Label positions are written only for edges with splines (`xlp` only when placed);
  unplaced head and tail labels are not drawn; an empty registered cluster is drawn.
* fdp's overlap removal sees each node's real shape (derived nodes share `ND_shape`;
  cluster nodes are boxes) and a record's field box.
* The spline router's record obstacle is the field box `fld->b` too, not `ND_height` with
  record_init's +1pt (`makeObstacle`), and genroute prints C's `Could not create control
  points for multiple spline` warning. This was open case
  `004-neato-router-record-obstacle-height` (issue #13; 18 of its 20 seeds now match, the
  other two also need the star/cylinder-obstacle and multispline face-order fixes). Its
  repros are kept in `crates/graphviz-pure-cli/tests/cli.rs`
  (`neato_record_obstacle_uses_field_box`, `circo_record_obstacle_falls_back_like_c`).
* patchwork places xlabels around the shape size `common_init_node` gives the node, not
  its patch.
* fdp places a cluster's boundary ports with separate `cos` and `sin` calls, as C does,
  instead of letting LLVM merge them into `__sincos_stret`, whose sine can differ from
  `sin()` in the last bit. This was open case `006-fdp-cluster-port-sincos-fusion` (issue
  #15, 12 fuzz cases); its repro is now `tests/fdp/fuzz_cluster_port_sincos.gv`.
* `placeLabels` orders the Hilbert keys as C's `int` (`HDict_t.key`, `icompare`), not as
  unsigned, so drawings over 32768pt build C's R-tree. This was open case
  `005-xlabels-hilbert-key-signed-order` (16 fuzz cases); its fdp and circo repros are in
  the corpora as `fuzz_xlabels_hilbert_signed_key.gv`, and
  `crates/graphviz-pure-cli/tests/cli.rs` (`xlabel_hilbert_keys_are_ordered_as_signed_int`).

Robustness (debug overflow and panics)
* pack's perimeter sum wraps like C's int; a one-point spline no longer panics
  `fillEdge`, and an empty one (on which C crashes) is skipped. The second was open case
  `003-pack-filledge-zero-point-spline-panic`; with no C output to compare, its repro is kept
  in `crates/graphviz-pure-cli/tests/cli.rs` (`packed_edges_with_empty_splines_do_not_panic`).
* Unknown shapes are boxes for compass ports (`IS_BOX`).

## Differences that remain: the open issues

The remaining differences are triaged into one issue draft per root cause. The drafts are
in [`docs/issues/manifest.json`](../issues/manifest.json) (title, labels, severity,
seeds and the issue body). Each one has a directory under
[`tests/fuzz-open/`](../../tests/fuzz-open) with the smallest reproducer (`repro.gv`),
C's and the port's output (`c.*`, `rust.*`), the command (`cmd.txt`) and the fuzz cases
it covers (`cases.txt`, with seeds).

How the triage was done:

* All 126 cases that were not identical after round 4 (`final_recheck.txt`: 106 SVG,
  3 exit status, 1 port panic, 15 `c_unstable`, 1 `c_crash`) were re-run against the
  port at HEAD with `scripts/fuzz-triage.py`. That script runs C 10 times per case, so
  C instability is measured rather than guessed from two runs. Two cases (seeds 1324
  and 12131, both earlier `c_unstable`) now match C and were dropped. The other 124
  still differ.
* Each case was re-minimised against HEAD, and every cluster was checked against the
  C source. Where the fix is small, a patched copy of the port was built, and a case
  joins an issue only if that fix makes it match C. This was checked for the
  record-obstacle, star/cylinder-obstacle, Hilbert-key-sign and fdp `sincos` issues.
  Five cases need two fixes. Four need both obstacle fixes, and one needs the record
  fix plus the upstream multispline face order. Their issue bodies say which.
* Nine of the 15 cases that the two-run check called `c_unstable` are stable over 10
  C runs. Only 13 of the 124 cases vary in C over 10 runs, and they split three ways:
  * Three are time-seeded (`start=random` without a seed, or fdp `start=regular`).
  * Four are C-side issues: fdp address dependence, and sfdp `smoothing=rng`.
  * Six vary slightly but are port bugs. With the fdp `sincos` or star/record-obstacle
    fix applied, the port matches C's most frequent output.

  Seed 33861 crashes C on some runs (`mode=KK model=circuit`).
* Every fuzz seed regenerates byte for byte with `python3 -I scripts/fuzz-gen.py --seed N`.
  The exceptions are the 11 time-seeded cases from rounds 1–2, which the current
  generator no longer produces.

Of the 124 cases, 100 fall into 17 port issues. An 18th port issue, `019`, is a
separate difference that appeared while minimising one of them. The other 24 fall into 8 upstream-C
issues: undefined behaviour, crashes, address- or clock-dependent output. These are
proposed as wontfix, with an explanation of what the port does instead. Further issues
cover gaps that are not from fuzzing: stderr wording, Linux/WASM
portable math, the twopi arrows graphs and Latin-1 `-Tplain`. One correction to earlier
notes: the circo/neato "record port 0.1pt off" difference is the record obstacle being
1pt taller in the port (it uses `ND_height` instead of the field box), not the port point
itself. The ortho variant also no longer has a gap: with a longer timeout, C finishes
`b100`, `b103` and `b104`, and their outputs match the port's.

| Issue | Severity | Fuzz cases | Status |
|---|---|---:|---|
| [`001-dot-cluster-rank-max-build-skeleton-panic`](../../tests/fuzz-open/001-dot-cluster-rank-max-build-skeleton-panic) | high | 1 | port |
| [`002-dot-ortho-concentrate-ignored-edge`](../../tests/fuzz-open/002-dot-ortho-concentrate-ignored-edge) | high | 1 | port |
| [`008-neato-router-star-cylinder-no-obstacle`](../../tests/fuzz-open/008-neato-router-star-cylinder-no-obstacle) | medium | 8 | port |
| [`011-dot-render-node-lw-rw-ht`](../../tests/fuzz-open/011-dot-render-node-lw-rw-ht) | medium | 3 | port |
| [`019-neato-polyline-compass-port-endpoint`](../../tests/fuzz-open/019-neato-polyline-compass-port-endpoint) | low | — | port |
| [`021-stderr-missing-warnings`](../../tests/fuzz-open/021-stderr-missing-warnings) | low | — | port |
| [`023-neato-circuit-kk-agseq-index-crash`](../../tests/fuzz-open/023-neato-circuit-kk-agseq-index-crash) | high | 1 | upstream C, wontfix |
| [`024-neato-ipsep-node-in-two-clusters-size-max-alloc`](../../tests/fuzz-open/024-neato-ipsep-node-in-two-clusters-size-max-alloc) | high | 1 | upstream C, wontfix |
| [`025-fdp-cluster-address-dependent-layout`](../../tests/fuzz-open/025-fdp-cluster-address-dependent-layout) | medium | 3 | upstream C, wontfix |
| [`026-neato-multispline-face-order`](../../tests/fuzz-open/026-neato-multispline-face-order) | medium | 3 | upstream C, wontfix |
| [`027-dot-sameport-record-uninit-pathend`](../../tests/fuzz-open/027-dot-sameport-record-uninit-pathend) | medium | 2 | upstream C, wontfix |
| [`028-pack-filledge-one-point-spline`](../../tests/fuzz-open/028-pack-filledge-one-point-spline) | medium | 2 | upstream C, wontfix |
| [`029-neato-fdp-start-time-seeded`](../../tests/fuzz-open/029-neato-fdp-start-time-seeded) | low | 11 | upstream C, wontfix |
| [`030-sfdp-smoothing-rng-c-unstable`](../../tests/fuzz-open/030-sfdp-smoothing-rng-c-unstable) | low | 1 | upstream C, wontfix |
| [`031-platform-libm-apple`](../../tests/fuzz-open/031-platform-libm-apple) | medium | — | wontfix (platform) |
| [`032-twopi-arrows-c-nondeterministic-routing`](../../tests/fuzz-open/032-twopi-arrows-c-nondeterministic-routing) | medium | — | upstream C, wontfix |
| [`033-latin1-plain-freed-label`](../../tests/fuzz-open/033-latin1-plain-freed-label) | low | — | upstream C, wontfix |
| [`034-html-cell-dashed-style-overread`](../../tests/fuzz-open/034-html-cell-dashed-style-overread) | low | — | upstream C, wontfix |

The C nondeterminism noted above still applies to the corpus. fdp and neato with
clusters, twopi with samehead arrows, and VPSC (`mode=ipsep`, which iterates a
`std::set` of pointers) depend on heap addresses. `gen-golden.sh` and
`gv-diff e2e --generate` rewrite those corpus files on every run, so their committed
goldens and hashes are kept.
