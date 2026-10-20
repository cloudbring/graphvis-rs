# @graphviz-pure/wasm

A Rust + WebAssembly port of [Graphviz](https://graphviz.org) 16.1.0: the DOT parser, all eight
layout engines (dot, neato, fdp, sfdp, twopi, circo, osage, patchwork) and the svg, json, dot,
xdot and plain renderers, with no C. Its output is the same bytes in every browser and on every
platform. graphviz-pure is an independent port, not affiliated with or endorsed by the Graphviz
project.

```js
import init, { render } from '@graphviz-pure/wasm';

await init();
const svg = render('digraph G { a -> b [label="fast"]; }', 'dot', 'svg');
```

`render(dot, engine, format)` lays out the first graph; `renderAll` returns one output per
graph; `renderAllBytes` takes and returns raw bytes, including non-UTF-8 input, as the
command line does.

Each release is built by GitHub Actions and published with npm provenance; check it with
`npm audit signatures`. Documentation, fidelity evidence and the source:
https://graphviz-pure.github.io/ and https://github.com/graphviz-pure/graphviz-pure. Licence: EPL-2.0.
