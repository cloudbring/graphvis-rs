#!/usr/bin/env node
// Copies the porting notes into the site at build time, adding Starlight frontmatter.
//
//   node scripts/site-sync-docs.mjs
//
// docs/porting/*.md and docs/c-bugs-ledger.md -> site/src/content/docs/porting/*.md
// (git-ignored: docs/ stays the single source). The first `# ` heading becomes the page
// title, and a note links back to the source file on GitHub.
import { mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, posix, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const out = join(root, 'site/src/content/docs/porting');
const repo = 'https://github.com/cloudbring/graphvis-rs/blob/master/';

// Sidebar order and short labels; files not listed here sort after them by name.
const order = {
	'dotgen.md': ['dot', 'How dotgen maps to the C sources, and what differs.'],
	'neato.md': ['neato', 'The neatogen solvers, overlap removal and packing.'],
	'sfdp.md': ['sfdp', 'Multilevel force-directed layout.'],
	'fdp-osage-patchwork.md': ['fdp, osage, patchwork', 'Cluster-aware force-directed layout, cluster packing and treemaps.'],
	'circo-twopi.md': ['circo, twopi', 'Circular and radial layouts.'],
	'render.md': ['Renderers', 'The emit code and output plugins.'],
	'libm.md': ['Math off macOS', 'Why Linux and WASM use a portable math library, and what that costs.'],
	'fuzzing.md': ['Differential fuzzing', 'Random graphs against C, round by round.'],
	'c-bugs-ledger.md': ['C bug ledger', 'C bugs and hazards found while porting.'],
};

const sources = [
	...readdirSync(join(root, 'docs/porting'))
		.filter((f) => f.endsWith('.md'))
		.map((f) => ['docs/porting/' + f, f]),
	['docs/c-bugs-ledger.md', 'c-bugs-ledger.md'],
];
const keys = Object.keys(order);
sources.sort(([, a], [, b]) => {
	const ia = keys.indexOf(a), ib = keys.indexOf(b);
	return (ia < 0 ? 99 : ia) - (ib < 0 ? 99 : ib) || a.localeCompare(b);
});

const yaml = (s) => JSON.stringify(s); // JSON strings are valid YAML scalars

// Relative links in the docs point into the repository. On the site, a ported doc links to
// its page, an issue directory (tests/fuzz-open/NNN-slug) to its What's left page, and
// anything else to the file on GitHub.
const tree = 'https://github.com/cloudbring/graphvis-rs/tree/master/';
const ported = new Set(sources.map(([r]) => r));
function relink(text, rel) {
	return text.replace(/\]\(([^)\s]+)\)/g, (all, href) => {
		if (/^[a-z]+:|^#|^\//i.test(href)) return all;
		const [path, hash = ''] = href.split('#');
		const target = posix.normalize(posix.join(posix.dirname(rel), path)).replace(/\/$/, '');
		const frag = hash ? '#' + hash : '';
		const issue = target.match(/^tests\/fuzz-open\/\d+-([^/]+)$/);
		if (issue) return `](/graphvis-rs/whats-left/${issue[1]}/${frag})`;
		if (ported.has(target)) {
			const page = posix.basename(target, '.md');
			return `](/graphvis-rs/porting/${page}/${frag})`;
		}
		return `](${tree}${target}${frag})`;
	});
}

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
sources.forEach(([rel, file], i) => {
	let text = readFileSync(join(root, rel), 'utf8').replace(/\r\n/g, '\n');
	const m = text.match(/^# (.+)\n/m);
	if (!m) throw new Error(`${rel}: no top-level heading`);
	const title = m[1].replace(/`/g, '').trim();
	text = text.slice(0, m.index) + text.slice(m.index + m[0].length);
	const [label, description] = order[file] ?? [title, ''];
	const fm = [
		'---',
		`title: ${yaml(title)}`,
		...(description ? [`description: ${yaml(description)}`] : []),
		'sidebar:',
		`  label: ${yaml(label)}`,
		`  order: ${i + 1}`,
		'editUrl: false',
		'---',
		'',
		`<p class="source-note">Copied at build time from <a href="${repo}${rel}"><code>${rel}</code></a>.</p>`,
		'',
	].join('\n');
	writeFileSync(join(out, file), fm + relink(text, rel).replace(/^\s*\n/, ''));
});
console.log(`site-sync-docs: ${sources.length} pages -> site/src/content/docs/porting/`);
