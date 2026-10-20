/**
 * The page's one connection to the WASM worker. `latest` keeps at most one request per
 * channel waiting: while a layout runs, newer drag positions replace older ones, so the
 * engine always works on where the pointer is now.
 */
import type { Request, Response } from './worker';

type Done = { ok: true; out: string | Uint8Array; ms: number } | { ok: false; error: string; ms: number } | { ok: false; stale: true; error: string; ms: number };
type Body = Request extends infer R ? (R extends { id: number } ? Omit<R, 'id'> : never) : never;

let worker: Worker | undefined;
let seq = 0;
const waiting = new Map<number, (r: Response) => void>();

function get(): Worker {
	if (!worker) {
		worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module', name: 'graphviz-pure' });
		worker.onmessage = (ev: MessageEvent<Response>) => {
			waiting.get(ev.data.id)?.(ev.data);
			waiting.delete(ev.data.id);
		};
		worker.onerror = (ev) => {
			for (const [id, f] of waiting) f({ id, ok: false, error: ev.message || 'The WASM worker failed to start', ms: 0 });
			waiting.clear();
		};
	}
	return worker;
}

export function call(body: Body): Promise<Done> {
	const id = ++seq;
	return new Promise((resolve) => {
		waiting.set(id, (r) => resolve(r));
		get().postMessage({ ...body, id });
	});
}

const channels = new Map<string, { busy: boolean; next?: { body: Body; resolve: (d: Done) => void } }>();

export function latest(channel: string, body: Body): Promise<Done> {
	let ch = channels.get(channel);
	if (!ch) channels.set(channel, (ch = { busy: false }));
	const c = ch;
	if (!c.busy) {
		c.busy = true;
		const run = (b: Body): Promise<Done> =>
			call(b).then((d) => {
				const nx = c.next;
				c.next = undefined;
				if (nx) run(nx.body).then(nx.resolve);
				else c.busy = false;
				return d;
			});
		return run(body);
	}
	return new Promise((resolve) => {
		c.next?.resolve({ ok: false, stale: true, error: 'superseded', ms: 0 });
		c.next = { body, resolve };
	});
}

export const render = (src: string, engine: string, format: string) => call({ op: 'render', src, engine, format });
