// The cross-platform check's outputs for the WASM build (see scripts/cross-platform-render.sh):
// renders each site gallery sample with renderAllBytes, as the browser does, and writes
// OUT/samples/NAME.svg and OUT/platform.json. The corpus hashes come from
//   node scripts/wasm-e2e.mjs --pkg PKG --hashes OUT/hashes.json
// Usage: node scripts/cross-platform-wasm.mjs PKG OUT
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';

const [pkg, out] = process.argv.slice(2);
const require = createRequire(import.meta.url);
const wasm = require(path.resolve(pkg, 'gv_wasm.js'));
const root = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
fs.mkdirSync(path.join(out, 'samples'), { recursive: true });
const samples = JSON.parse(fs.readFileSync(path.join(root, 'site/src/data/samples.json'), 'utf8'));
for (const s of samples) {
  const src = fs.readFileSync(path.join(root, 'site/public/samples', s.name, s.wasm_input));
  const bytes = Buffer.from(wasm.renderAllBytes(new Uint8Array(src), s.engine, 'svg'));
  fs.writeFileSync(path.join(out, 'samples', `${s.name}.svg`), bytes);
}
fs.writeFileSync(
  path.join(out, 'platform.json'),
  JSON.stringify({ id: 'wasm32', label: 'WebAssembly (Node.js)', os: 'wasm32', arch: 'wasm32', runtime: `Node.js ${process.version}`, build: 'wasm' }, null, 1),
);
console.log(`wrote ${samples.length} samples to ${out}`);
