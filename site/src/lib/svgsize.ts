// Build-time only: the pixel size an <img> of a stored SVG will have (pt -> CSS px).
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

export function svgSize(publicPath: string): { width: number; height: number } {
	try {
		const head = readFileSync(join(process.cwd(), 'public', publicPath), 'utf8').slice(0, 2000);
		const m = head.match(/<svg width="([\d.]+)(pt|px)?" height="([\d.]+)(pt|px)?"/);
		if (!m) return { width: 400, height: 300 };
		const k = m[2] === 'px' ? 1 : 4 / 3;
		return { width: Math.round(+m[1] * k), height: Math.round(+m[3] * k) };
	} catch {
		return { width: 400, height: 300 };
	}
}
