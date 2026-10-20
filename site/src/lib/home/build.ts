/**
 * Build-time rendering with the same WASM build the browser loads (site/public/wasm, made by
 * scripts/site-wasm.sh before `astro build`). The front page's first drawing comes from here,
 * so it is the port's output before any script runs.
 */
import { createHash } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import { arch, platform } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

type Mod = { initSync: (o: { module: Buffer }) => unknown; render: (src: string, engine: string, format: string) => string };
let mod: Promise<Mod | null> | undefined;

function load() {
	mod ??= (async () => {
		const dir = join(process.cwd(), 'public/wasm');
		if (!existsSync(join(dir, 'gv_wasm_bg.wasm'))) {
			console.warn('[home] public/wasm is missing: run scripts/site-wasm.sh. The front page will have no static drawing.');
			return null;
		}
		const m = (await import(/* @vite-ignore */ pathToFileURL(join(dir, 'gv_wasm.js')).href)) as Mod;
		m.initSync({ module: readFileSync(join(dir, 'gv_wasm_bg.wasm')) });
		return m;
	})();
	return mod;
}

export interface BuildRender {
	svg: string;
	bytes: number;
	sha256: string;
}

export async function renderAtBuild(src: string, engine: string): Promise<BuildRender | null> {
	const m = await load();
	if (!m) return null;
	const svg = m.render(src, engine, 'svg');
	const buf = Buffer.from(svg, 'utf8');
	return { svg, bytes: buf.length, sha256: createHash('sha256').update(buf).digest('hex') };
}

const OS: Record<string, string> = { linux: 'Linux', darwin: 'macOS', win32: 'Windows' };
const ARCH: Record<string, string> = { x64: 'x86-64', arm64: 'arm64' };
/** Where this build ran, in the words the Cross-platform page uses. */
export const buildHost = `${OS[platform()] ?? platform()} ${ARCH[arch()] ?? arch()}`;
export const buildRuntime = `Node.js ${process.versions.node.split('.')[0]}`;
