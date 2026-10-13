<img src="assets/icon.svg" width="64" height="64" alt="">

# graphvis-rs

**Pure Rust + WebAssembly port of Graphviz 16.1.0** (`#forbid(unsafe_code)`, zero C/FFI dependencies), running natively on desktop/server and directly inside the browser via `wasm32-unknown-unknown`.

---

## Highlights & Performance vs. C Graphviz 16.1.0

| Benchmark Suite (20 Upstream `.gv` Fixtures) | Pure Rust (`aarch64-apple-darwin`) | Browser WASM (`wasm32-unknown-unknown`) | C Graphviz 16.1.0 (`/opt/homebrew/bin/dot`) | Rust Speedup |
| :--- | :---: | :---: | :---: | :---: |
| **DOT Parse + Canonical Write (`-Tcanon`)** | `426.64 µs` (`21.3 µs/graph`, `56.4 MB/s`) | `~590 µs` (`29.5 µs/graph`) | `41,241.06 µs` (`2,062.1 µs/graph`) | **96.7x faster** |
| **End-to-End Hierarchical Layout + SVG (`dot -Tsvg`)** | `5,348.35 µs` (`267.4 µs/graph`) | `~3,280 µs` (`164.0 µs/graph` on `fsm.gv`) | `43,795.50 µs` (`2,189.8 µs/graph`) | **8.2x faster** |
| **End-to-End Layout + Graphviz JSON (`dot -Tjson`)** | `8,214.13 µs` (`410.7 µs/graph`) | `~4,900 µs` (`245.0 µs/graph`) | `57,119.82 µs` (`2,856.0 µs/graph`) | **7.0x faster** |
| **Font Metrics LUT 3-Line Measurement** | `198.43 ns/op` | `~260 ns/op` | Built-in LUT + Pango | Zero C font deps |
| **X11/SVG/Brewer Color + Hex/HSV Resolution** | `34.92 ns/op` | `~48 ns/op` | `colxlate.c` | Binary-searched |

- **WASM Binary Footprint (`web/pkg/gv_wasm_bg.wasm`)**:
  - `wasm-opt -O3` raw binary: **718 KB** (`735,587` bytes)
  - Gzipped transfer size: **276 KB** (`282,837` bytes)

---

## Crate Architecture

- [`crates/gv-core`](crates/gv-core): Arena-backed `Graph` IR, geometry primitives (`Point`, `BoundingBox`, `Spline`), Graphviz 16.1.0 built-in font metrics LUT (`font.rs`), and X11/SVG/Brewer color tables (`color.rs`).
- [`crates/gv-parser`](crates/gv-parser): Re-entrant DOT lexer (`lexer.rs`), recursive-descent parser with cluster/subgraph lexical attribute inheritance (`parser.rs`), and canonical serializer (`canon.rs`).
- [`crates/gv-geom`](crates/gv-geom): Node shape sizing and perimeter ray clipping (`shapes.rs`, including recursive `record`/`Mrecord`, HTML `<TABLE>` labels, polygons, stars, cylinders, and compass ports) + Bezier spline routing and arrowheads (`splines.rs`).
- [`crates/gv-layout`](crates/gv-layout): All 8 Graphviz layout engines:
  - `dot`: 5-stage hierarchical layout (`acyclic` -> Network Simplex `rank` -> 24-pass `mincross` + `transpose` -> `position` -> `dotsplines`)
  - `neato`: Kamada-Kawai spring/stress majorization
  - `fdp` & `sfdp`: Spring-electrical Fruchterman-Reingold force-directed layout
  - `twopi`: Concentric radial layout
  - `circo`: Circular layout
  - `osage`: Clustered array grid packing
  - `patchwork`: Squarified treemap layout
- [`crates/gv-render`](crates/gv-render): Graphviz output renderers (`svg`, `json`, `json0`, `dot`, `xdot`, `canon`, `plain`).
- [`crates/gv-engine`](crates/gv-engine): High-level pipeline orchestrator (`render_dot(dot, engine, format)`).
- [`crates/gv-wasm`](crates/gv-wasm): `wasm-bindgen` browser & JS/TS bindings (`render`, `renderSvg`, `renderJson`, `inspectGraph`).
- [`crates/gv-cli`](crates/gv-cli): Drop-in `dot-rs` CLI binary and differential benchmark suite (`bench_phase1`, `bench_phase2`).

---

## Browser Usage (`@graphvis-rs/wasm`)

```js
import init, { renderSvg, inspectGraph } from '@graphvis-rs/wasm';

await init();
const svg = renderSvg('digraph G { a -> b [label="fast"]; }', 'dot');
document.getElementById('diagram').innerHTML = svg;
```
