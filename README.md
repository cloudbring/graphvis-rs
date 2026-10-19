<img src="assets/icon.svg" width="64" height="64" alt="">

# graphvis-rs

**A Rust + WebAssembly port of Graphviz 16.1.0** (`#![forbid(unsafe_code)]`, no C or FFI dependencies). It runs natively and in the browser (`wasm32-unknown-unknown`), and on macOS its output is byte-identical to C Graphviz on every input in the project's test corpora.

---

## Status

- All eight layout engines (`dot`, `neato`, `fdp`, `sfdp`, `twopi`, `circo`, `osage`, `patchwork`) are function-by-function ports of the Graphviz 16.1.0 C code, together with the libraries they use: network simplex, `pathplan`, `ortho`, `pack`, `vpsc`, the `neatogen` solvers, `lib/label` and the shape, label and HTML-table code. `docs/porting/` maps each Rust module to its C file and lists every known difference.
- Output formats: `svg`, `json`, `json0`, `dot`, `xdot`, `canon`, `plain`, `plain-ext`. `-n`/`-n2` (use positions from the input) is supported.
- Text is measured with Graphviz's built-in font metric tables, which is what C does when built without Pango. The reference outputs come from C 16.1.0 run that way (see `scripts/gen-golden.sh`).
- C behaviour is reproduced where output depends on it, including C bugs and platform details: the macOS `qsort` and `rand`, `drand48`, clang's fused multiply-adds, cdt's splay trees, and C's error counting and exit status. `docs/c-bugs-ledger.md` lists the reproduced C bugs.

### Fidelity against C Graphviz 16.1.0

Measured with `gv-diff`, which `cargo test --workspace` runs as ratchets that fail on any regression:

| Check | macOS (Apple libm, as C uses) | Linux and WASM |
| :--- | :---: | :---: |
| Full `-Tsvg` byte identity (`gv-diff e2e`), 9 corpora | 2,589 / 2,589 | 2,470 / 2,589 |
| Layout geometry, main corpus (565 files: wild graphs, upstream fixtures, fuzz reproducers) | 565 / 565 exact | 564 / 565 |
| Per-engine corpora, exact | neato 442/442, sfdp 297/297, circo 280/280, fdp 386/386, osage 180/180, patchwork 141/141, twopi 282/289 | see `tests/golden/*.portable.json` |
| Renderers given C's layout (`gv-diff render`, svg/xdot/dot/json) | all identical | all identical |
| C's own test graphs (`graphs/`, `tests/graphs/`, `tests/regression_tests/`), dot with base, LR, ortho, concentrate and newrank variants | 379 / 379 per variant (376 for ortho, where C times out on 3) | not measured |
| Differential fuzzing (6,000 random graphs, all engines, final round) | 98.8% identical | not measured |

- **Linux and WASM** use a portable math library. Apple's `pow`, `sin`, `cos` and `atan2` are closed source and are not reproduced bit for bit (see `docs/porting/libm.md`). Linux on aarch64 and x86_64 and the WASM build give exactly the same output as each other.
- **twopi:** the 7 inexact files are a floating-point tie in edge routing that C itself breaks differently from build to build.
- **Fuzzing:** the remaining differences are documented in `docs/porting/fuzzing.md`. Most are C nondeterminism (results that depend on memory addresses) and C crashes.

## Checking fidelity yourself

```sh
cargo test --workspace                                         # every ratchet
cargo run --release -p gv-diff -- e2e --list                   # full-SVG identity, list differences
cargo run --release -p gv-diff -- --filter clust --worst 20    # layout geometry, a subset
cargo run --release -p gv-diff -- render                       # renderers on C's own layouts
node scripts/wasm-e2e.mjs --pkg <wasm-bindgen nodejs output>   # the same check under WASM
scripts/update-baselines.sh [--portable]                       # after an improvement
```

Regenerating the C references needs Graphviz 16.1.0 installed: `scripts/gen-golden.sh`, `scripts/gen-render-golden.sh` and `GVBINDIR=<pango-free plugin dir> gv-diff e2e --generate`.

## Performance

Measured with one process per run on both sides on the same machine (Apple M-series), taking the minimum of 3 runs of `-Tsvg`. The machine was shared with other work, so treat these as rough:

| Graph | C `dot` | `dot-rs` |
| :--- | ---: | ---: |
| `b100` (Graphviz test graph) | 4.5 s | 4.7 s |
| `b103` | 0.73 s | 0.69 s |
| random DAG, 2,000 nodes | 0.56 s | 0.46 s |
| random DAG, 8,000 nodes | 24.5 s | 20.0 s |
| `b100` with `splines=ortho` | over 300 s | 20.1 s |

Output is identical in every row. The ortho speedup comes from data structures with better complexity than C's in a few places, not from changing the algorithm. The WASM package is 1.7 MB (about 700 KB gzipped).

---

## Crates

- [`gv-core`](crates/gv-core): graph IR, geometry, font metric tables, colour tables with `colorxlate` semantics, charset handling, and the math module (`math::PROFILE` selects Apple or portable).
- [`gv-parser`](crates/gv-parser): DOT lexer and parser with cgraph's attribute, subgraph and ordering semantics, and the canonical writer.
- [`gv-geom`](crates/gv-geom): node sizing (`shapes.c`, records, HTML tables, `labels.c`) and `pathplan`.
- [`gv-layout`](crates/gv-layout): the engines (`dotgen`, `neatogen`, `fdpgen`, `sfdpgen`, `twopigen`, `circogen`, `osage`, `patchwork`), the shared `lib/common` code, plus `ortho`, `pack`, `vpsc`, `sparse` and no-layout mode.
- [`gv-render`](crates/gv-render): a port of C's emit code and output plugins, and of cgraph's writer.
- [`gv-engine`](crates/gv-engine): the pipeline (`render_dot`, `render_dot_all`, `render_dot_all_partial`).
- [`gv-cli`](crates/gv-cli): `dot-rs`, a drop-in `dot` command line (`-T`, `-K`, `-o`, `-O`, `-G`/`-N`/`-E`, `-n`, several files, every graph in a file, C's exit status).
- [`gv-diff`](crates/gv-diff): the differential harness and ratchets.
- [`gv-wasm`](crates/gv-wasm): `wasm-bindgen` bindings.

---

## Browser usage (`@graphvis-rs/wasm`)

```js
import init, { render, renderAll } from '@graphvis-rs/wasm';

await init();
const svg = render('digraph G { a -> b [label="fast"]; }', 'dot', 'svg');
document.getElementById('diagram').innerHTML = svg;
```

`render` lays out the first graph in the input. `renderAll` returns one output per graph, and `renderAllBytes` takes and returns raw bytes the way the command line does, including non-UTF-8 input. Each call starts from fresh state, so repeated calls give identical output. `web/index.html` is a playground: `cd web && npm run build:wasm && npm run serve`.

## Licence

[EPL-2.0](LICENSE), the licence of Graphviz. The port follows the Graphviz C sources closely and is a derivative work of Graphviz; [NOTICE](NOTICE) credits the Graphviz authors and lists the third-party code in the math module (MIT), whose provenance is detailed in `docs/porting/libm.md`.
