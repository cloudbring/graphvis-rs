import { issues, severityRank, ENGINES, type Issue } from './data';

const engineRank = (e: string) => {
	const i = ENGINES.indexOf(e);
	return i < 0 ? ENGINES.length : i;
};
export const sortIssues = (list: Issue[]) =>
	[...list].sort((a, b) => severityRank(a.severity) - severityRank(b.severity) || engineRank(a.engine) - engineRank(b.engine) || a.slug.localeCompare(b.slug));

export const portBugs = sortIssues(issues.filter((i) => !i.upstream_c));
export const upstream = sortIssues(issues.filter((i) => i.upstream_c));

/** Files named c.<ext> and rust.<ext> with the same extension, the first pair found. */
export function pair(i: Issue) {
	const order = ['svg', 'png', 'json', 'dot', 'xdot', 'txt', 'plain'];
	const exts = i.files.filter((f) => f.startsWith('c.') && !f.includes('stderr')).map((f) => f.slice(2));
	exts.sort((a, b) => (order.indexOf(a) + 99) % 99 - (order.indexOf(b) + 99) % 99);
	const ext = exts.find((e) => i.files.includes(`rust.${e}`));
	return ext ? { ext, kind: ext === 'svg' ? 'svg' : ext === 'png' ? 'png' : 'text' } as const : undefined;
}
