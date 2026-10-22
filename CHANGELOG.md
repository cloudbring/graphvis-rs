# Changelog

All notable changes are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow the
[versioning policy](docs/versioning.md) (Semantic Versioning, with output changes always noted).

## [Unreleased]

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

[Unreleased]: https://github.com/graphviz-pure/graphviz-pure/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/graphviz-pure/graphviz-pure/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/graphviz-pure/graphviz-pure/releases/tag/v0.1.0
