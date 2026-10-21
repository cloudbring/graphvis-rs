// A Cloudflare Worker using the package as the README shows: wrangler bundles the .wasm as a
// compiled WebAssembly.Module, and initSync instantiates it. POST a DOT file to /?engine=NAME
// and get the SVG back.
import { initSync, renderAllBytes } from '@graphviz-pure/wasm';
import wasm from '@graphviz-pure/wasm/graphviz_pure_wasm_bg.wasm';

initSync({ module: wasm });

export default {
  async fetch(request) {
    const engine = new URL(request.url).searchParams.get('engine') ?? 'dot';
    try {
      const svg = renderAllBytes(new Uint8Array(await request.arrayBuffer()), engine, 'svg');
      return new Response(svg, { headers: { 'content-type': 'image/svg+xml' } });
    } catch (e) {
      // The full error goes to the Worker's log; the response carries only its message.
      console.error(e);
      return new Response(e instanceof Error ? e.message : 'render failed', { status: 500 });
    }
  },
};
