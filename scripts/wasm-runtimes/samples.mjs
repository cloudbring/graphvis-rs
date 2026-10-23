// The gallery samples, with their committed output
// (site/public/samples/NAME/rust.svg), as scripts/release-smoke-wasm.mjs checks them. Uses only
// node:fs, which Node.js, Deno and Bun all provide.
import fs from 'node:fs';
import process from 'node:process';

const root = new URL('../../', import.meta.url);

export function loadSamples() {
  const all = JSON.parse(fs.readFileSync(new URL('site/src/data/samples.json', root), 'utf8'));
  return all.map((s) => {
    const dir = new URL(`site/public/samples/${s.name}/`, root);
    return {
      name: s.name,
      engine: s.engine,
      input: new Uint8Array(fs.readFileSync(new URL(s.wasm_input, dir))),
      expected: new Uint8Array(fs.readFileSync(new URL('rust.svg', dir))),
    };
  });
}

export function sameBytes(a, b) {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

// Renders every sample with `render(input, engine)` (sync or async, returning bytes) and returns
// the names of the samples whose output differs from rust.svg.
export async function differing(samples, render) {
  const bad = [];
  for (const s of samples) {
    const out = await render(s.input, s.engine);
    if (!sameBytes(new Uint8Array(out), s.expected)) bad.push(s.name);
  }
  return bad;
}

// One line for run.mjs to read; exits non-zero when a sample differs.
export function report(label, samples, bad) {
  console.log(`${label}: ${samples.length - bad.length}/${samples.length} samples byte-identical`);
  if (bad.length) {
    console.error(`${label}: differ: ${bad.join(', ')}`);
    process.exit(1);
  }
}
