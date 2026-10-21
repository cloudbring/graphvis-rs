// Node.js, Deno and Bun: loads the package one way and renders the samples. run.mjs runs it as
//   <runtime> local.mjs npm     import '@graphviz-pure/wasm' (the packed tarball in node_modules), init()
//   <runtime> local.mjs bytes   the same import, init({ module_or_path: <the .wasm file's bytes> })
//   <runtime> local.mjs jsr     the module jsr.json exports, as a file: URL, init()
// Each form loads the package once per process, so each is its own run.
import fs from 'node:fs';
import process from 'node:process';
import { differing, loadSamples, report } from './samples.mjs';

const how = process.argv[2];
let gv;
if (how === 'npm' || how === 'bytes') {
  gv = await import('@graphviz-pure/wasm');
  if (how === 'npm') await gv.default();
  else {
    const wasm = fs.readFileSync(new URL(import.meta.resolve('@graphviz-pure/wasm/graphviz_pure_wasm_bg.wasm')));
    await gv.default({ module_or_path: wasm });
  }
} else if (how === 'jsr') {
  const jsr = JSON.parse(fs.readFileSync(new URL('../../jsr.json', import.meta.url), 'utf8'));
  gv = await import(new URL(jsr.exports, new URL('../../', import.meta.url)).href);
  await gv.default();
} else {
  console.error('usage: local.mjs npm|bytes|jsr');
  process.exit(2);
}

const samples = loadSamples();
report(how, samples, await differing(samples, (input, engine) => gv.renderAllBytes(input, engine, 'svg')));
