import fidelityJson from '../data/fidelity.json';
import samplesJson from '../data/samples.json';
import issuesJson from '../data/issues.json';

export const BASE = import.meta.env.BASE_URL.replace(/\/?$/, '/');
/** A root-relative URL under the site's base: url('gallery/') -> /graphvis-rs/gallery/ */
export const url = (path = '') => BASE + path.replace(/^\//, '');
export const REPO = 'https://github.com/cloudbring/graphvis-rs';
export const blob = (path: string, commit = 'master') => `${REPO}/blob/${commit}/${path}`;

export type Fidelity = typeof fidelityJson;
export const fidelity: Fidelity = fidelityJson;

export interface Sample {
	name: string;
	title: string;
	engine: string;
	flags: string[];
	source: string;
	features: string[];
	identical: boolean;
	identical_portable: boolean;
	wasm_input: string;
	sha256: { c: string; rust: string };
	bytes: { input: number; c: number; rust: number };
}
export const samples: Sample[] = samplesJson as Sample[];

export interface Issue {
	slug: string;
	dir: string;
	title: string;
	labels: string[];
	engine: string;
	category: string;
	severity: string;
	seeds: (number | string)[];
	case_count: number;
	upstream_c: boolean;
	close_as_wontfix: boolean;
	body: string;
	files: string[];
	issue_url?: string;
}
export const issues: Issue[] = issuesJson as Issue[];

export const ENGINES = ['dot', 'neato', 'fdp', 'sfdp', 'circo', 'twopi', 'osage', 'patchwork'];
const SEVERITY = ['crash', 'critical', 'high', 'major', 'medium', 'minor', 'low', 'cosmetic'];
export const severityRank = (s: string) => {
	const i = SEVERITY.indexOf((s || '').toLowerCase());
	return i < 0 ? SEVERITY.length : i;
};

export const fmt = (n: number) => n.toLocaleString('en-US');
export const pct = (a: number, b: number, digits = 1) =>
	b === 0 ? '—' : `${((100 * a) / b).toFixed(a === b ? 0 : digits)}%`;
export const sourceUrl = (s: Sample) =>
	s.source.startsWith('graphviz-16.1.0/')
		? `https://gitlab.com/graphviz/graphviz/-/blob/16.1.0/${s.source.slice('graphviz-16.1.0/'.length)}`
		: blob(s.source);
export const cmdline = (s: Sample, bin = 'dot') =>
	[bin, `-K${s.engine}`, ...s.flags, '-Tsvg', 'input.gv'].join(' ');
