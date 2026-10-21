# Changelog

All notable changes are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow the
[versioning policy](docs/versioning.md) (Semantic Versioning, with output changes always noted).

## [Unreleased]

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

[Unreleased]: https://github.com/graphviz-pure/graphviz-pure/commits/master
