/// <reference lib="webworker" />
/**
 * Runs the port's WASM build off the main thread, so a layout never costs the page a frame.
 * Same files as src/lib/wasm.ts (site/public/wasm, built by scripts/site-wasm.sh); the HTTP
 * cache serves both.
 */
type Mod = {
	default: (opts: { module_or_path: string }) => Promise<unknown>;
	render: (src: string, engine: string, format: string) => string;
	renderAllBytes: (src: Uint8Array, engine: string, format: string) => Uint8Array;
	version: () => string;
};

export type Request =
	| { id: number; op: 'render'; src: string; engine: string; format: string }
	| { id: number; op: 'bytes'; src: Uint8Array; engine: string; format: string }
	| { id: number; op: 'version' };
export type Response = { id: number; ok: true; out: string | Uint8Array; ms: number } | { id: number; ok: false; error: string; ms: number };

const base = import.meta.env.BASE_URL.replace(/\/?$/, '/');
let mod: Promise<Mod> | undefined;
let generation = 0;

function load() {
	mod ??= (async () => {
		// After a trap, a new query string gives a new module and so a new instance.
		const q = generation ? `?i=${generation}` : '';
		const m = (await import(/* @vite-ignore */ `${base}wasm/gv_wasm.js${q}`)) as Mod;
		await m.default({ module_or_path: `${base}wasm/gv_wasm_bg.wasm` });
		return m;
	})();
	mod.catch(() => (mod = undefined));
	return mod;
}

self.onmessage = async (ev: MessageEvent<Request>) => {
	const r = ev.data;
	let t0 = performance.now();
	try {
		const m = await load();
		t0 = performance.now();
		const out = r.op === 'version' ? m.version() : r.op === 'bytes' ? m.renderAllBytes(r.src, r.engine, r.format) : m.render(r.src, r.engine, r.format);
		(self as unknown as Worker).postMessage({ id: r.id, ok: true, out, ms: performance.now() - t0 } satisfies Response);
	} catch (e) {
		// A Rust panic traps the instance; the next call gets a new one.
		if (e instanceof WebAssembly.RuntimeError) {
			mod = undefined;
			generation++;
		}
		(self as unknown as Worker).postMessage({ id: r.id, ok: false, error: e instanceof Error ? e.message : String(e), ms: performance.now() - t0 } satisfies Response);
	}
};
