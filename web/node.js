// The package entry for Node.js (and Deno or Bun when they install it from npm), selected by the
// "node" export condition. The wasm-bindgen glue's `init()` fetches the .wasm next to it, and
// Node's fetch does not read file: URLs, so with no argument this reads the file instead. Every
// export, and `init` given an argument, is the glue's own.
import { readFile } from 'node:fs/promises';
import init from './pkg/graphviz_pure_wasm.js';

export * from './pkg/graphviz_pure_wasm.js';

export default async function (options) {
  return init(options ?? { module_or_path: await readFile(new URL('./pkg/graphviz_pure_wasm_bg.wasm', import.meta.url)) });
}
