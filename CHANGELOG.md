# Changelog

All notable changes are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow the
[versioning policy](docs/versioning.md) (Semantic Versioning, with output changes always noted).

## [Unreleased]

## [0.1.2] - 2026-10-22

Fixes for the remaining crashes and hangs from the fuzzer, each made at its cause: where C
Graphviz draws an input the port draws it the same way, and where C crashes the port takes C's
nearest own failure path instead of panicking.

### Fixed

- `splines=ortho` no longer panics, hangs or overflows inside the orthogonal router, whatever
  its input: every internal failure ends at one place and is handled as C handles its own
  failures at that stage. A failed maze abandons ortho routing like C's `mkMaze` returning NULL;
  a failed route leaves the edges unrouted (straight in neato, fdp, twopi, circo and osage) with
  a warning. Pins about 1e8 inches or more apart no longer overflow the shortest-path distances
  (#229), and NaN pins no longer reach an out-of-bounds read in the partition (#228). This
  includes the earlier single-site fixes: a trapezoid with no upper neighbour (#199), touching
  tiny nodes in `mark_small` (#202), and circo and twopi edges whose start the router cannot
  reach, where C segfaults in `convertSPtoRoute` (#186).
- dot with `splines=ortho`: when the maze cannot be built (tiny touching nodes that leave no
  free cell, or a failed trapezoidation), the port no longer panics or fails with
  `orthoEdges: cannot build the maze`. As in C, the layout is abandoned and the nodes are drawn
  without edges, exit 0 (#203, #200, #202).
- dot: a labelled flat edge on rank 0 that is merged behind a parallel or reversed flat edge now
  gets its label rank, as on lower ranks, instead of panicking (C segfaults in `flat_node`), also
  on the unnormalised top rank left by a failed ranking; with `concentrate=true`, a cluster edge
  routed apart from its concentrated chain no longer panics in `conc_slope` or `straight_len`
  (C segfaults) (#230, #234, #241).
- dot: when a cluster's own ranking fails (`trouble in init_rank` inside the cluster), the layout
  fails with an error where C fails an assertion, instead of panicking. `install_in_rank` has
  all of C's rank-capacity checks, so an overflowing rank abandons the layout with C's error
  (#232, #233).
- dot: when mincross leaves a node unplaced after `trouble in init_rank`, the layout fails with
  a `make_regular_edge` error instead of panicking in `maximal_bbox` (C segfaults in mincross's
  `furthestnode`) (#114).
- dot: a cluster whose nodes a rank set or an earlier cluster takes after it was ranked no
  longer makes the port panic where C crashes: when such a cluster has no nodes left, the
  layout fails with an error (#231).
- Extreme node positions no longer hang or panic where C hangs or asserts: curved edges between
  nodes pinned about 1e16 inches apart (the bounding-box refinement in `update_bb_bz` now stops
  after a per-segment cutoff, #227), packing a component with an infinite pin (#239), and
  `overlap=vpsc` with NaN, infinite or huge positions (#240).
- neato `mode=ipsep` with an infeasible VPSC system (for example a node in two clusters under
  `overlap=ipsep`) no longer runs without bound in time and memory: the VPSC solver reuses the
  blocks it no longer needs, and after more than 500 VPSC cycle errors, where C aborts at the
  first, the port warns and draws the layout it has (#197, #222).
- neato `mode=ipsep overlap=vpsc` with clusters no longer panics. The final overlap-removal pass
  sizes its boxes as C does, so graphs C renders now lay out like C. Where C segfaults (fewer
  nodes than twice the number of clusters), the pass also includes the cluster boundary
  variables (#185).
- neato, fdp, sfdp, circo and twopi no longer hang routing `splines=true`/`polyline` edges when
  the router's triangle path never reaches the tail, for example a compass port on a rotated
  box. C aborts in `mkPoly` or `edgeToSeg`; the edge now falls back to ordinary routing (#138).
- fdp with `splines=compound`: a compound edge whose head cannot be reached among the cluster
  obstacles (seen after `overlap=ipsep`) no longer panics in `pobspath`. The port warns and
  routes the edges without compound edges. C loops forever in `Pobspath` on this input (#198).
- `neato -n2` no longer panics on an edge `pos` that ends in a multi-byte character. Like C, it
  rejects the spline and routes the edge (#181).
- fdp no longer panics in debug builds on nodes pinned near 1e17: the grid neighbour index in
  `gridRepulse` wraps at the `int` limit as C's does (#196).
- An edge label, `headlabel` or `xlabel` on a graph with an infinite bounding box (a cluster
  repeated across subgraphs) no longer panics in debug builds on an overflow of the Hilbert
  order in xlabel placement. The order is computed as C's `xlhorder` does; C fails an assertion
  in `xlhdxload` on these inputs, and the port exits cleanly (#191).
- The crates on crates.io now include `LICENSE` and `NOTICE`, and `graphviz-pure-core` the MIT
  licence of the CORE-MATH `pow` port; CI and the release check that every release archive, the
  npm/JSR package and the crates carry their licence files (#139).

### Changed

- Output change: dot with `splines=ortho`, where the maze cannot be built, now exits 0 and
  writes C's drawing (the nodes, no edges, coordinates untranslated) instead of exiting 1 with
  `orthoEdges: cannot build the maze` (#200, #203).
- Output change: dot now fails with `expand_cluster: cluster X has no nodes left` (exit 1) on a
  graph where a rank set or an earlier cluster empties a nested cluster after it was ranked.
  Some of these inputs drew before; C segfaults on all of them (#231).
- Output change: neato and fdp with `overlap=portho*` or `ortho*` and nodes pinned about 3e6
  inches or more apart now lay out as C does, wrapping coordinates in 32-bit arithmetic like C
  (#240).
- Output change: a curved edge segment that would need more than 2^28 splits in `update_bb_bz`
  is now taken whole, so the bounding box (and SVG's `translate`) can be slightly larger than
  C's on inputs where C spends several seconds there. Inputs under the cutoff are unchanged
  (#227).
- Output change: neato `mode=ipsep` layouts that reach more than 500 VPSC cycle errors (two
  corpus files under `diredgeconstraints=true`) now stop at the cap with a warning instead of
  running on for tens of seconds and gigabytes; neither matched C before (#197).
- New warnings on stderr, as C prints them: `neato -n2` reports a rejected edge `pos` with C's
  `doesn't have 3n+1 points` or `syntax error in pos attribute`, once per run (#181), and fdp
  reports a `pos` it cannot read with `position ..., expected two floats` (#240).
- CI fails when the layout or geom crates gain an `unwrap`, `expect`, index or
  integer-overflow site: `scripts/panic-sites.sh` counts them per file against
  `tests/panic-sites.tsv` (#236).

## [0.1.1] - 2026-10-21

Fixes for the crashes and hangs that the coverage-guided fuzzer (#162) found in its first runs.

### Fixed

- The command line no longer panics on an argument that is not valid UTF-8, such as a Latin-1
  file name, or on a flag such as `-é`. Like C, it takes its arguments as bytes: the file opens,
  and other flag values keep their bytes (#152).
- Every engine panicked on a subgraph name whose 7th byte falls inside a multi-byte character,
  such as `subgraph "aaaaaaé"`. The cluster-name test now compares bytes, as C's `strncasecmp`
  does (#165).
- neato, fdp, sfdp, twopi and circo no longer panic on an `overlap` value that starts with
  `pris` followed by a multi-byte character, or on a `diredgeconstraints` value that starts with
  `hie` followed by one (#166). An unrecognized `overlap` value now prints C's
  `Unrecognized overlap value` warning, except under `neato -n` (#149).
- circo no longer panics (`no entry found for key` in blockpath) on a graph with a block whose
  nodes share no edge, such as a cycle whose articulation points belong to other blocks; it
  draws the graph as C does (#167).
- neato `mode=ipsep`: a graph whose clusters sit in different connected components no longer
  panics; each component is laid out with the cluster members it contains. C Graphviz fails an
  assertion on this input (#168).
- neato with `mode=hier` no longer panics when every edge `len` in a component is below 1
  (#169). C Graphviz fails an assertion on this input.
- circo and twopi with `splines=ortho`: when a route segment lies in no channel of the ortho
  maze, the port no longer panics. It warns and draws the edges as straight lines; C Graphviz
  aborts on a failed assertion in `chanSearch` (#170).
- `splines=ortho` no longer panics in the partition when a vertex needs more than four
  monotone-chain slots (osage with a repeated cluster, #171). Where C Graphviz fails `chkSgraph`,
  the edges fall back to straight lines (dot reports a layout error, #200); graphs that C routes
  are routed the same way.
- osage no longer overflows its stack when a cluster is repeated under several parents and
  edges leave its nodes inside a later copy (#173). That copy gets a NaN box, as in C, and the
  edges that inherit it no longer make `update_bb_bz` split their Bézier segments forever.
  C Graphviz crashes on this input.
- Packing components no longer hangs when one is enormous (`height="1e30"`, `quantum="1e40"`)
  or pinned near 1e30, where C Graphviz never finishes either. When the polyomino grid would
  need more than 2^24 cells, the packer warns `components too large to pack; packing skipped`
  and leaves the components where the layout put them; C still packs some of these after up to
  about 30 seconds. Inputs that need fewer cells pack as before (#174).
- Output change: packing now wraps the grid-step multiplication in 32-bit arithmetic, as C does,
  so a very large step (`pack=2000000000`, or a component around 1e11 points wide) places the
  components where C does (#174).

## [0.1.0] - 2026-10-21

The first release.

### Added

- A function-by-function Rust port of Graphviz 16.1.0: the DOT parser, all eight layout engines
  (dot, neato, fdp, sfdp, twopi, circo, osage, patchwork) and the `svg`, `json`, `json0`, `dot`,
  `xdot`, `canon`, `plain` and `plain-ext` renderers. On macOS with Apple Silicon, `-Tsvg` is
  byte-identical to C Graphviz on all 2,590 inputs of the test corpora; on every other platform
  the output is one set of bytes, the same everywhere.
- `-Tpng` and `-Tjpg` raster output, drawn from the SVG with resvg and bundled fonts.
- The `graphviz-pure` command line, compatible with `dot`'s options and exit status.
- `@graphviz-pure/wasm`, the WebAssembly build for browsers, Node.js, Deno, Bun and Workers,
  on npm and JSR. `await init()` with no argument works in all of them but Workers, which import
  `@graphviz-pure/wasm/graphviz_pure_wasm_bg.wasm` and call `initSync`; CI checks each one.
- `THIRD-PARTY-LICENSES.md`: the licence text and copyright notices of every Rust crate compiled
  into the `graphviz-pure` binary and `@graphviz-pure/wasm`.
- Release archives include `THIRD-PARTY-LICENSES.md` and a `LICENSES/` folder with the licence
  texts of third-party code compiled into the binary: the MIT licence of the CORE-MATH `pow` port
  and the OFL licences of the bundled Tinos, Arimo and Cousine fonts.
- A container image, `ghcr.io/graphviz-pure/graphviz-pure` (linux/amd64 and linux/arm64).

[Unreleased]: https://github.com/graphviz-pure/graphviz-pure/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/graphviz-pure/graphviz-pure/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/graphviz-pure/graphviz-pure/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/graphviz-pure/graphviz-pure/releases/tag/v0.1.0
