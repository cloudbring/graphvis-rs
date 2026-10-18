# Differential fuzzing against C Graphviz 16.1.0

The corpus ratchets (`gv-diff`, `gv-diff e2e`) only cover inputs someone thought of. The
fuzzer generates random DOT inputs, runs C `dot` and the port on each, and minimises
every difference to a small reproducer. Each bug it found was fixed against the C
source, and one reproducer per bug is kept in the corpus (`tests/wild/fuzz_*.gv`,
`tests/<engine>/fuzz_*.gv`, with C goldens and e2e hashes).

## Running it

```sh
cargo build --release -p gv-cli --bin dot-rs && cargo build -p gv-cli --bin dot-rs
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
* cgraph reads input a line at a time and each graph gets a new scanner: the rest of
  the line after a graph's closing `}` is dropped, line numbers restart, and the graphs
  read before a syntax error are rendered before dot exits 1.
* A graph attribute first declared in a subgraph has value `""` at the root (so
  `splines` set in a subgraph turns the root's edges off).
* Graphs without nodes keep their label (dot postprocesses them; twopi, circo and
  patchwork leave labels at the origin).

Errors and exit status
* `gv_core::diag` keeps C's error level: any `agerrorf` makes the exit status 1 while
  the output is still written. It is reported where C reports it: routesplines and
  checkpath failures, lost edges, fdp's non-comparable clusters, bad record labels,
  rejected HTML labels (expat's messages with `error_context` and `in label of ...`;
  yacc's syntax error wins over a later lexer error, as C lexes on demand), neato
  positions that are not two doubles, `trouble in init_rank`, `contain_nodes`, and
  concentrate's `rebuild_vlists`. Pathplan's `prerror` lines, textspan_lut's metric
  fallback warnings, `addXLabels`' count warning and `deriveClusters`' message are
  printed too.
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

Rendering
* A color list whose first entry is a known SVG name is written out whole
  (`stroke="red:blue"`), because `gvrender_resolve_color` keeps the caller's pointer.
* HSV colors are read with `sscanf` semantics, so a weighted entry such as
  `0.1,0.9,0.7;0.3` resolves.
* `outputorder` carries over to later graphs of the same run (job flags are ORed).
* NaN prints as `nan`, as macOS libc does.
* Tapered edges subdivide with clang's fused multiply-add.
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
* In the pack branch, edges are mapped only when a component routed splines.
* A pack component is not the root for `is_a_cluster`, so newrank ignores its top-level
  sink/source sets.

neato family
* `resolvePorts` drops `tailclip=false` on `_` ports.
* Label positions are written only for edges with splines (`xlp` only when placed);
  unplaced head and tail labels are not drawn; an empty registered cluster is drawn.
* fdp's overlap removal sees each node's real shape (derived nodes share `ND_shape`;
  cluster nodes are boxes) and a record's field box.
* patchwork places xlabels around the shape size `common_init_node` gives the node, not
  its patch.

Robustness (debug overflow and panics)
* pack's perimeter sum wraps like C's int; a one-point spline no longer panics
  `fillEdge`.
* Unknown shapes are boxes for compass ports (`IS_BOX`).

## Differences that remain, and why

* **C nondeterminism.** fdp and neato with clusters, and twopi with samehead arrows,
  depend on heap addresses. VPSC (`mode=ipsep`) iterates a `std::set` of pointers. These
  are the `c_unstable` cases (C's output changed between two runs). Several corpus
  files are in the same state: `gen-golden.sh` and `gv-diff e2e --generate` rewrite them
  on every run, so their committed goldens and hashes are kept.
* **C undefined behaviour or crash paths.** A node in two non-nested clusters, with
  `pack`, or with newrank and clusters: C reports the conflict and lays out from
  inconsistent shared node records (often a crash, see above). The port cannot
  reproduce the stale state. The minimised `rank=max` + cluster + `pack` case
  (`seed 24180`) panics in `build_skeleton` where C reads a NULL rank leader, and C
  crashes on the same graph without `pack`. `fillEdge` on a one-point spline reads past
  the list in C. `mode=ipsep` with overlapping clusters makes C abort on a `SIZE_MAX`
  allocation.
* **Open differences.** These were found but not fixed in this pass. Each has a
  minimised reproducer under the fuzz run directories, not in the corpus:
  * circo and neato `splines=spline` edges to a record port. C's `Pshortestpath` fails
    ("destination point not in any triangle") and falls back to a straight polyline,
    while the port routes a spline: the record port point differs by 0.1pt.
  * dot `pack` with `splines=""|none` and edge labels: C leaves the labels unplaced at
    component coordinates outside the page, so `edge_in_box` drops the edge; the port
    emits the edge group.
  * Some samehead/sametail and concentrate combinations, and neato `overlap=ortho|scale`
    with pinned nodes.
* **stderr.** Exit status and the error messages above match. Not reproduced:
  `printpath` dumps, the remaining `agwarningf` texts (port and HTML attribute warnings,
  "no position for edge with label"), and the exact line numbers of DOT syntax errors.
  The port's syntax-error message text also differs (`DOT syntax error at line L,
  column C: ...` instead of `syntax error in line L near 'tok'`).
