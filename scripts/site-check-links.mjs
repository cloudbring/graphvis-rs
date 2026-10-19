#!/usr/bin/env node
// Checks every internal link in the built site: href, src, and the data-* URLs the
// comparison components fetch (data-c, data-r, data-input, data-load).
//
//   node scripts/site-check-links.mjs site/dist
//
// Links under /graphvis-rs/api/ are only checked when dist/api exists (CI copies rustdoc
// there after the Astro build).
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';

const dist = resolve(process.argv[2] ?? 'dist');
const BASE = '/graphvis-rs/';
const haveApi = existsSync(join(dist, 'api'));

function* html(dir) {
	for (const e of readdirSync(dir, { withFileTypes: true })) {
		const p = join(dir, e.name);
		if (e.isDirectory()) {
			if (p === join(dist, 'api')) continue;
			yield* html(p);
		} else if (e.name.endsWith('.html')) yield p;
	}
}

const attr = /\s(?:href|src|data-c|data-r|data-input|data-load)="([^"]+)"/g;
const bad = [];
let checked = 0;
for (const file of html(dist)) {
	const text = readFileSync(file, 'utf8');
	const page = '/' + relative(dist, file).replace(/index\.html$/, '');
	for (const m of text.matchAll(attr)) {
		let link = m[1].replace(/&amp;/g, '&');
		if (/^(https?:|mailto:|data:|blob:|#|javascript:)/.test(link)) continue;
		if (!link.startsWith('/')) link = new URL(link, 'http://x' + BASE + page.slice(1)).pathname;
		link = decodeURIComponent(link.split('#')[0].split('?')[0]);
		if (!link.startsWith(BASE)) {
			bad.push(`${page}: ${m[1]} (outside ${BASE})`);
			continue;
		}
		if (link.startsWith(BASE + 'api/') && !haveApi) continue;
		const target = join(dist, link.slice(BASE.length));
		const ok =
			(existsSync(target) && (statSync(target).isFile() || existsSync(join(target, 'index.html')))) ||
			existsSync(target + '.html');
		checked++;
		if (!ok) bad.push(`${page}: ${m[1]}`);
	}
}
if (bad.length) {
	console.error(`site-check-links: ${bad.length} broken of ${checked}:\n  ` + [...new Set(bad)].join('\n  '));
	process.exit(1);
}
console.log(`site-check-links: ${checked} internal links ok${haveApi ? '' : ' (api/ not built, skipped)'}`);
