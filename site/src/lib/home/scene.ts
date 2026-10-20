/**
 * A Graphviz layout as something to draw and animate: the xdot drawing operations of a
 * `-Tjson` output (the same operations -Tsvg writes out), grouped per graph object, in
 * SVG coordinates (y down).
 *
 * Nothing here invents geometry. A frame between two layouts is an interpolation of the two
 * layouts' own operations; at rest the scene is exactly the engine's.
 */

export type RGBA = [number, number, number, number];

interface Paint {
	fill: RGBA | null;
	stroke: RGBA | null;
	/** pen width in points */
	w: number;
	/** 0 solid, 1 dashed, 2 dotted */
	dash: number;
}
/** A polygon, polyline or cubic Bézier path; pts is flat [x0, y0, x1, y1, …]. */
export interface Shape extends Paint {
	t: 'poly' | 'bez';
	pts: number[];
	closed: boolean;
}
export interface Ellipse extends Paint {
	t: 'ell';
	cx: number;
	cy: number;
	rx: number;
	ry: number;
}
export interface Text {
	t: 'text';
	x: number;
	y: number;
	/** -1 start, 0 middle, 1 end */
	anchor: number;
	size: number;
	face: string;
	color: RGBA;
	text: string;
}
export type Prim = Shape | Ellipse | Text;

export type Kind = 'graph' | 'cluster' | 'edge' | 'node';
export interface Item {
	key: string;
	kind: Kind;
	name: string;
	prims: Prim[];
	alpha: number;
	href?: string;
	tooltip?: string;
	/** node centre and half-size, for hit areas, focus rings and pinning */
	cx: number;
	cy: number;
	hw: number;
	hh: number;
}
export interface Scene {
	items: Item[];
	/** x0, y0, x1, y1 */
	bb: [number, number, number, number];
}

const KIND_ORDER: Record<Kind, number> = { graph: 0, cluster: 1, edge: 2, node: 3 };

function color(hex: string): RGBA | null {
	if (!hex || hex[0] !== '#') return null;
	const v = (i: number) => parseInt(hex.slice(i, i + 2), 16);
	const a = hex.length >= 9 ? v(7) / 255 : 1;
	return a === 0 ? null : [v(1), v(3), v(5), a];
}

type Op = { op: string; [k: string]: any };

/** Interprets one object's xdot operations the way Graphviz's renderers do. */
function interpret(ops: Op[] | undefined, out: Prim[]) {
	if (!ops) return;
	let pen: RGBA | null = [0, 0, 0, 1];
	let fill: RGBA | null = [0, 0, 0, 1];
	let w = 1;
	let dash = 0;
	let size = 14;
	let face = 'Times-Roman';
	const flat = (pts: number[][]) => pts.flatMap(([x, y]) => [x, -y]);
	for (const op of ops) {
		switch (op.op) {
			case 'c':
				pen = color(op.color ?? op.stops?.[0]?.color);
				break;
			case 'C':
				fill = color(op.color ?? op.stops?.[0]?.color);
				break;
			case 'S': {
				const s: string = op.style;
				if (s === 'solid') dash = 0;
				else if (s === 'dashed') dash = 1;
				else if (s === 'dotted') dash = 2;
				else if (s === 'bold') w = 2;
				else if (s.startsWith('setlinewidth(')) w = parseFloat(s.slice(13));
				break;
			}
			case 'F':
				size = op.size;
				face = op.face;
				break;
			case 'E':
			case 'e': {
				const [cx, cy, rx, ry] = op.rect;
				out.push({ t: 'ell', cx, cy: -cy, rx, ry, fill: op.op === 'E' ? fill : null, stroke: pen, w, dash });
				break;
			}
			case 'P':
			case 'p':
			case 'L': {
				const pts = flat(op.points);
				out.push({ t: 'poly', pts, closed: op.op !== 'L', fill: op.op === 'P' ? fill : null, stroke: pen, w, dash });
				break;
			}
			case 'B':
			case 'b': {
				const pts = flat(op.points);
				const n = pts.length;
				const closed = op.op === 'B' || (Math.abs(pts[0] - pts[n - 2]) < 0.01 && Math.abs(pts[1] - pts[n - 1]) < 0.01);
				out.push({ t: 'bez', pts, closed, fill: op.op === 'B' ? fill : null, stroke: pen, w, dash });
				break;
			}
			case 'T': {
				const [x, y] = op.pt;
				out.push({ t: 'text', x, y: -y, anchor: op.align === 'l' ? -1 : op.align === 'r' ? 1 : 0, size, face, color: pen ?? [0, 0, 0, 1], text: op.text });
				break;
			}
		}
	}
}

const DRAW_KEYS = ['_draw_', '_ldraw_', '_hdraw_', '_tdraw_', '_hldraw_', '_tldraw_'];

/** Builds a scene from `render(src, engine, 'json')`, moved by (dx, dy) in SVG coordinates. */
export function sceneFromJson(text: string, dx = 0, dy = 0): Scene {
	const g = JSON.parse(text);
	const items: Item[] = [];
	const prims = (o: any) => {
		const out: Prim[] = [];
		for (const k of DRAW_KEYS) interpret(o[k], out);
		return out;
	};
	const [x0, y0, x1, y1] = String(g.bb).split(',').map(Number);
	items.push({ key: 'graph', kind: 'graph', name: g.name, prims: prims(g), alpha: 1, cx: 0, cy: 0, hw: 0, hh: 0 });
	const subCount: number = g._subgraph_cnt ?? 0;
	const names: string[] = [];
	for (const o of g.objects ?? []) {
		names[o._gvid] = o.name;
		const isNode = o._gvid >= subCount;
		if (!isNode) {
			if (!String(o.name).startsWith('cluster')) continue;
			items.push({ key: `c:${o.name}`, kind: 'cluster', name: o.name, prims: prims(o), alpha: 1, cx: 0, cy: 0, hw: 0, hh: 0 });
			continue;
		}
		const [px, py] = String(o.pos ?? '0,0').split(',').map(Number);
		items.push({
			key: `n:${o.name}`,
			kind: 'node',
			name: o.name,
			prims: prims(o),
			alpha: 1,
			href: o.URL || o.href || undefined,
			tooltip: o.tooltip || undefined,
			cx: px,
			cy: -py,
			hw: (parseFloat(o.width) * 72) / 2,
			hh: (parseFloat(o.height) * 72) / 2,
		});
	}
	for (const e of g.edges ?? []) {
		if (String(e.style ?? '').includes('invis')) continue;
		items.push({ key: `e:${e._gvid}`, kind: 'edge', name: `${names[e.tail]}->${names[e.head]}`, prims: prims(e), alpha: 1, cx: 0, cy: 0, hw: 0, hh: 0 });
	}
	items.sort((a, b) => KIND_ORDER[a.kind] - KIND_ORDER[b.kind]);
	const scene: Scene = { items, bb: [x0, -y1, x1, -y0] };
	return dx || dy ? translate(scene, dx, dy) : scene;
}

export function translate(s: Scene, dx: number, dy: number): Scene {
	const mv = (p: Prim): Prim => {
		if (p.t === 'text') return { ...p, x: p.x + dx, y: p.y + dy };
		if (p.t === 'ell') return { ...p, cx: p.cx + dx, cy: p.cy + dy };
		return { ...p, pts: p.pts.map((v, i) => v + (i & 1 ? dy : dx)) };
	};
	return {
		items: s.items.map((it) => ({ ...it, cx: it.cx + dx, cy: it.cy + dy, prims: it.prims.map(mv) })),
		bb: [s.bb[0] + dx, s.bb[1] + dy, s.bb[2] + dx, s.bb[3] + dy],
	};
}

/** The bounding box of what is drawn (a drag can move things outside the engine's bb). */
export function inkBox(s: Scene): [number, number, number, number] {
	let x0 = Infinity,
		y0 = Infinity,
		x1 = -Infinity,
		y1 = -Infinity;
	const add = (x: number, y: number) => {
		if (x < x0) x0 = x;
		if (x > x1) x1 = x;
		if (y < y0) y0 = y;
		if (y > y1) y1 = y;
	};
	for (const it of s.items) {
		if (it.kind === 'graph' || it.alpha < 0.05) continue;
		for (const p of it.prims) {
			if (p.t === 'text') add(p.x, p.y);
			else if (p.t === 'ell') {
				add(p.cx - p.rx, p.cy - p.ry);
				add(p.cx + p.rx, p.cy + p.ry);
			} else for (let i = 0; i < p.pts.length; i += 2) add(p.pts[i], p.pts[i + 1]);
		}
	}
	return x0 === Infinity ? s.bb : [x0, y0, x1, y1];
}

/* ---------- Interpolation ---------- */

const lerp = (a: number, b: number, t: number) => a + (b - a) * t;
function lerpColor(a: RGBA | null, b: RGBA | null, t: number): RGBA | null {
	if (!a && !b) return null;
	const A = a ?? ([b![0], b![1], b![2], 0] as RGBA);
	const B = b ?? ([a![0], a![1], a![2], 0] as RGBA);
	return [lerp(A[0], B[0], t), lerp(A[1], B[1], t), lerp(A[2], B[2], t), lerp(A[3], B[3], t)];
}

/** Flattens a cubic Bézier path into a polyline. */
function flatten(p: Shape): number[] {
	if (p.t === 'poly') return p.closed ? [...p.pts, p.pts[0], p.pts[1]] : p.pts;
	const q = p.pts;
	const out = [q[0], q[1]];
	for (let i = 2; i + 5 < q.length; i += 6) {
		const [ax, ay, bx, by, cx, cy, dx, dy] = [q[i - 2], q[i - 1], q[i], q[i + 1], q[i + 2], q[i + 3], q[i + 4], q[i + 5]];
		for (let k = 1; k <= 8; k++) {
			const t = k / 8,
				u = 1 - t;
			out.push(u * u * u * ax + 3 * u * u * t * bx + 3 * u * t * t * cx + t * t * t * dx, u * u * u * ay + 3 * u * u * t * by + 3 * u * t * t * cy + t * t * t * dy);
		}
	}
	return out;
}

/** n points spaced evenly along a polyline. */
function resample(line: number[], n: number): number[] {
	const seg: number[] = [0];
	for (let i = 2; i < line.length; i += 2) seg.push(seg[seg.length - 1] + Math.hypot(line[i] - line[i - 2], line[i + 1] - line[i - 1]));
	const total = seg[seg.length - 1] || 1;
	const out: number[] = [];
	let j = 1;
	for (let k = 0; k < n; k++) {
		const d = (total * k) / (n - 1);
		while (j < seg.length - 1 && seg[j] < d) j++;
		const s0 = seg[j - 1],
			s1 = seg[j] ?? s0;
		const t = s1 > s0 ? (d - s0) / (s1 - s0) : 0;
		out.push(lerp(line[2 * j - 2], line[2 * j] ?? line[2 * j - 2], t), lerp(line[2 * j - 1], line[2 * j + 1] ?? line[2 * j - 1], t));
	}
	return out;
}

const RING = 48;
/** A closed outline as RING points, clockwise on screen, starting at the top. */
function ring(p: Shape | Ellipse): number[] {
	if (p.t === 'ell') {
		const out: number[] = [];
		for (let k = 0; k < RING; k++) {
			const a = -Math.PI / 2 + (2 * Math.PI * k) / RING;
			out.push(p.cx + p.rx * Math.cos(a), p.cy + p.ry * Math.sin(a));
		}
		return out;
	}
	let pts = resample(flatten({ ...p, closed: true }), RING + 1).slice(0, RING * 2);
	let area = 0,
		cx = 0,
		cy = 0;
	for (let i = 0; i < RING; i++) {
		const j = (i + 1) % RING;
		area += pts[2 * i] * pts[2 * j + 1] - pts[2 * j] * pts[2 * i + 1];
		cx += pts[2 * i];
		cy += pts[2 * i + 1];
	}
	cx /= RING;
	cy /= RING;
	if (area < 0) {
		const r: number[] = [];
		for (let i = RING - 1; i >= 0; i--) r.push(pts[2 * i], pts[2 * i + 1]);
		pts = r;
	}
	let best = 0,
		bestA = Infinity;
	for (let i = 0; i < RING; i++) {
		const a = Math.abs(Math.atan2(pts[2 * i + 1] - cy, pts[2 * i] - cx) + Math.PI / 2);
		if (a < bestA) {
			bestA = a;
			best = i;
		}
	}
	return [...pts.slice(best * 2), ...pts.slice(0, best * 2)];
}

const isClosed = (p: Prim) => p.t === 'ell' || (p.t !== 'text' && p.closed);

function morphable(a: Prim, b: Prim) {
	if (a.t === 'text' || b.t === 'text') return a.t === b.t && (a as Text).text === (b as Text).text;
	return isClosed(a) === isClosed(b);
}

function lerpPaint(a: Paint, b: Paint, t: number): Paint {
	return { fill: lerpColor(a.fill, b.fill, t), stroke: lerpColor(a.stroke, b.stroke, t), w: lerp(a.w, b.w, t), dash: t < 0.5 ? a.dash : b.dash };
}

function lerpPrim(a: Prim, b: Prim, t: number): Prim {
	if (a.t === 'text' && b.t === 'text')
		return { ...b, x: lerp(a.x, b.x, t), y: lerp(a.y, b.y, t), size: lerp(a.size, b.size, t), color: lerpColor(a.color, b.color, t) ?? b.color };
	const pa = a as Shape | Ellipse,
		pb = b as Shape | Ellipse;
	const paint = lerpPaint(pa, pb, t);
	if (pa.t === 'ell' && pb.t === 'ell')
		return { t: 'ell', ...paint, cx: lerp(pa.cx, pb.cx, t), cy: lerp(pa.cy, pb.cy, t), rx: lerp(pa.rx, pb.rx, t), ry: lerp(pa.ry, pb.ry, t) };
	if (pa.t !== 'ell' && pb.t !== 'ell' && pa.t === pb.t && pa.pts.length === pb.pts.length)
		return { t: pa.t, closed: pb.closed, ...paint, pts: pa.pts.map((v, i) => lerp(v, pb.pts[i], t)) };
	const closed = isClosed(pa);
	const A = closed ? ring(pa) : resample(flatten(pa as Shape), 32);
	const B = closed ? ring(pb) : resample(flatten(pb as Shape), 32);
	return { t: 'poly', closed, ...paint, pts: A.map((v, i) => lerp(v, B[i], t)) };
}

function compatible(a: Item, b: Item) {
	return a.prims.length === b.prims.length && a.prims.every((p, i) => morphable(p, b.prims[i]));
}

/**
 * The frame t (0..1) of the way from scene a to scene b. Objects in both morph; objects whose
 * drawings cannot morph (a different label, say) cross-fade; the rest fade in or out.
 */
export function between(a: Scene, b: Scene, t: number): Scene {
	if (t >= 1) return b;
	const byKey = new Map(a.items.map((it) => [it.key, it]));
	const items: Item[] = [];
	for (const ib of b.items) {
		const ia = byKey.get(ib.key);
		byKey.delete(ib.key);
		if (!ia) {
			items.push({ ...ib, alpha: ib.alpha * t });
		} else if (compatible(ia, ib)) {
			items.push({
				...ib,
				alpha: lerp(ia.alpha, ib.alpha, t),
				cx: lerp(ia.cx, ib.cx, t),
				cy: lerp(ia.cy, ib.cy, t),
				hw: lerp(ia.hw, ib.hw, t),
				hh: lerp(ia.hh, ib.hh, t),
				prims: ib.prims.map((p, i) => lerpPrim(ia.prims[i], p, t)),
			});
		} else {
			// One ghost per object: an older one still fading out is dropped.
			byKey.delete(ia.key + '~');
			items.push({ ...ia, key: ia.key + '~', alpha: ia.alpha * (1 - t), href: undefined });
			items.push({ ...ib, alpha: ib.alpha * t, cx: lerp(ia.cx, ib.cx, t), cy: lerp(ia.cy, ib.cy, t) });
		}
	}
	const taken = new Set(items.map((it) => it.key));
	for (const ia of byKey.values()) {
		const alpha = ia.alpha * (1 - t);
		if (alpha > 0.01 && !taken.has(ia.key)) items.push({ ...ia, alpha, href: undefined });
	}
	items.sort((x, y) => KIND_ORDER[x.kind] - KIND_ORDER[y.kind]);
	return { items, bb: [lerp(a.bb[0], b.bb[0], t), lerp(a.bb[1], b.bb[1], t), lerp(a.bb[2], b.bb[2], t), lerp(a.bb[3], b.bb[3], t)] };
}

/* ---------- Drawing ---------- */

export const css = (c: RGBA | null) => (c ? (c[3] >= 1 ? `rgb(${c[0] | 0},${c[1] | 0},${c[2] | 0})` : `rgba(${c[0] | 0},${c[1] | 0},${c[2] | 0},${c[3].toFixed(3)})`) : 'none');
const r2 = (v: number) => Math.round(v * 100) / 100;

/** SVG path data for a shape. */
export function pathData(p: Shape): string {
	const q = p.pts;
	let d = `M${r2(q[0])},${r2(q[1])}`;
	if (p.t === 'bez') for (let i = 2; i + 5 < q.length; i += 6) d += `C${r2(q[i])},${r2(q[i + 1])} ${r2(q[i + 2])},${r2(q[i + 3])} ${r2(q[i + 4])},${r2(q[i + 5])}`;
	else for (let i = 2; i < q.length; i += 2) d += `L${r2(q[i])},${r2(q[i + 1])}`;
	return p.closed ? d + 'Z' : d;
}

/** Graphviz's PostScript faces, with metric-compatible fallbacks (Tinos for Times). */
export const TIMES = "Times, Tinos, 'Times New Roman', serif";
export function fontFamily(face: string) {
	if (/^Courier/i.test(face)) return "Courier, 'Courier New', monospace";
	if (/^(Helvetica|Arial)/i.test(face)) return 'Helvetica, Arial, sans-serif';
	return TIMES;
}

/** Draws a scene on a 2D canvas (the -Tpng mode). */
export function drawCanvas(ctx: CanvasRenderingContext2D, s: Scene, minWidth = 0) {
	for (const it of s.items) {
		ctx.globalAlpha = Math.max(0, Math.min(1, it.alpha));
		for (const p of it.prims) {
			if (p.t === 'text') {
				ctx.fillStyle = css(p.color);
				ctx.font = `${p.size}px ${fontFamily(p.face)}`;
				ctx.textAlign = p.anchor < 0 ? 'left' : p.anchor > 0 ? 'right' : 'center';
				ctx.fillText(p.text, p.x, p.y);
				continue;
			}
			ctx.beginPath();
			if (p.t === 'ell') ctx.ellipse(p.cx, p.cy, p.rx, p.ry, 0, 0, Math.PI * 2);
			else {
				const q = p.pts;
				ctx.moveTo(q[0], q[1]);
				if (p.t === 'bez') for (let i = 2; i + 5 < q.length; i += 6) ctx.bezierCurveTo(q[i], q[i + 1], q[i + 2], q[i + 3], q[i + 4], q[i + 5]);
				else for (let i = 2; i < q.length; i += 2) ctx.lineTo(q[i], q[i + 1]);
				if (p.closed) ctx.closePath();
			}
			if (p.fill) {
				ctx.fillStyle = css(p.fill);
				ctx.fill();
			}
			if (p.stroke) {
				ctx.strokeStyle = css(p.stroke);
				ctx.lineWidth = Math.max(p.w, minWidth);
				ctx.setLineDash(p.dash === 1 ? [5, 2] : p.dash === 2 ? [1, 5] : []);
				ctx.stroke();
			}
		}
	}
	ctx.globalAlpha = 1;
}

/**
 * Takes the antialiasing back out: a smoothed pixel is a blend of two of the drawing's
 * colours, so it becomes whichever of the two covers more of it. The result has hard
 * edges and only the drawing's own colours, the look of Graphviz's gd-based -Tpng.
 */
export function quantize(img: ImageData, palette: RGBA[], bg?: RGBA | null) {
	const d = img.data;
	const pal = palette.map((c) => [c[0], c[1], c[2]]);
	// Like gd, a line that touches a pixel at all marks it: against the paper, ink wins early.
	const paper = bg ? pal.findIndex((c) => c[0] === (bg[0] | 0) && c[1] === (bg[1] | 0) && c[2] === (bg[2] | 0)) : -1;
	const cache = new Map<number, number>();
	const pick = (r: number, g: number, b: number) => {
		let best = 0,
			bd = Infinity;
		for (let i = 0; i < pal.length; i++) {
			const [ar, ag, ab] = pal[i];
			for (let j = i; j < pal.length; j++) {
				const [br, bg, bb] = pal[j];
				const ur = br - ar,
					ug = bg - ag,
					ub = bb - ab;
				const len = ur * ur + ug * ug + ub * ub;
				let t = len ? ((r - ar) * ur + (g - ag) * ug + (b - ab) * ub) / len : 0;
				t = t < 0 ? 0 : t > 1 ? 1 : t;
				const er = ar + ur * t - r,
					eg = ag + ug * t - g,
					eb = ab + ub * t - b;
				// Grays are collinear: prefer the shorter blend (white to gray60 over white to black).
				const dist = er * er + eg * eg + eb * eb + len * 1e-4;
				if (dist < bd) {
					bd = dist;
					const cut = i === paper ? 0.3 : j === paper ? 0.7 : 0.5;
					best = t < cut ? i : j;
				}
			}
		}
		return best;
	};
	// Most pixels are paper: compare whole 32-bit pixels first, then remember the last colour.
	const px = new Uint32Array(d.buffer, d.byteOffset, d.length >> 2);
	const packed = pal.map(([r, g, b]) => (((255 << 24) | (b << 16) | (g << 8) | r) >>> 0));
	const paperPx = paper >= 0 ? packed[paper] : -1;
	let lastIn = -1,
		lastOut = 0;
	for (let i = 0; i < px.length; i++) {
		const v = px[i];
		if (v === paperPx) continue;
		if (v === lastIn) {
			px[i] = lastOut;
			continue;
		}
		const a = v >>> 24;
		let out: number;
		if (a < 128) out = 0;
		else {
			const key = v & 0xffffff;
			let k = cache.get(key);
			if (k === undefined) cache.set(key, (k = pick(v & 255, (v >>> 8) & 255, (v >>> 16) & 255)));
			out = packed[k];
		}
		lastIn = v;
		lastOut = out;
		px[i] = out;
	}
}

export function paletteOf(s: Scene): RGBA[] {
	const seen = new Map<string, RGBA>();
	const add = (c: RGBA | null) => {
		if (c && c[3] > 0.5) seen.set(`${c[0] | 0},${c[1] | 0},${c[2] | 0}`, c);
	};
	for (const it of s.items)
		for (const p of it.prims) {
			if (p.t === 'text') add(p.color);
			else {
				add(p.fill);
				add(p.stroke);
			}
		}
	return [...seen.values()];
}
