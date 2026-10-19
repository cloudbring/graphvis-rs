// Loads the port's WASM build (site/public/wasm, built by scripts/site-wasm.sh) on demand.
type GvWasm = {
	default: (opts: { module_or_path: string }) => Promise<unknown>;
	renderAllBytes: (src: Uint8Array, engine: string, format: string) => Uint8Array;
	version: () => string;
};

let current: Promise<GvWasm> | undefined;
let generation = 0;
const base = import.meta.env.BASE_URL.replace(/\/?$/, '/');

export function loadWasm(): Promise<GvWasm> {
	current ??= (async () => {
		// After a trap, a new query string gives a new module and so a new instance.
		const q = generation ? `?i=${generation}` : '';
		const m = (await import(/* @vite-ignore */ `${base}wasm/gv_wasm.js${q}`)) as GvWasm;
		await m.default({ module_or_path: `${base}wasm/gv_wasm_bg.wasm` });
		return m;
	})();
	current.catch(() => (current = undefined));
	return current;
}

export interface RenderResult {
	ok: boolean;
	text: string;
	error?: string;
	ms: number;
}

/** Renders every graph in `src` to SVG the way `dot -K<engine> -Tsvg FILE` does. */
export async function renderBytes(src: Uint8Array, engine: string): Promise<RenderResult> {
	const m = await loadWasm();
	const t0 = performance.now();
	try {
		const out = m.renderAllBytes(src, engine, 'svg');
		return { ok: true, text: new TextDecoder().decode(out), ms: performance.now() - t0 };
	} catch (e) {
		// A Rust panic traps the instance; the next call gets a new one.
		if (e instanceof WebAssembly.RuntimeError) {
			current = undefined;
			generation++;
		}
		return { ok: false, text: '', error: e instanceof Error ? e.message : String(e), ms: performance.now() - t0 };
	}
}

export async function wasmVersion() {
	return (await loadWasm()).version();
}
