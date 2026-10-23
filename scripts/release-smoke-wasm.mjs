// Release smoke test for the npm package: the WASM build in web/pkg must reproduce, byte for
// byte, every gallery sample, as committed in site/public/samples/NAME/rust.svg (as
// release-smoke.py does for the binaries; every build writes the same bytes).
// Usage: node scripts/release-smoke-wasm.mjs
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const pkg = path.join(root, 'web/pkg');
const gv = await import(pathToFileURL(path.join(pkg, 'graphviz_pure_wasm.js')).href);
await gv.default({ module_or_path: fs.readFileSync(path.join(pkg, 'graphviz_pure_wasm_bg.wasm')) });
const samples = JSON.parse(fs.readFileSync(path.join(root, 'site/src/data/samples.json'), 'utf8'));
const bad = [];
for (const s of samples) {
  const d = path.join(root, 'site/public/samples', s.name);
  const out = Buffer.from(gv.renderAllBytes(new Uint8Array(fs.readFileSync(path.join(d, s.wasm_input))), s.engine, 'svg'));
  if (!out.equals(fs.readFileSync(path.join(d, 'rust.svg')))) bad.push(s.name);
}
console.log(`${samples.length - bad.length}/${samples.length} samples byte-identical`);
if (bad.length) { console.error('differ: ' + bad.join(', ')); process.exit(1); }
