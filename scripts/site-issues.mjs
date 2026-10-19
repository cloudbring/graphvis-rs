#!/usr/bin/env node
// Copies the open-issue manifest into the site.
//
//   node scripts/site-issues.mjs [--manifest docs/issues/manifest.json] [--filed docs/issues/filed.json]
//
// Reads the manifest (an array of {slug, dir, title, labels, engine, category, severity,
// seeds, case_count, upstream_c, close_as_wontfix, body}), writes it to
// site/src/data/issues.json with `files` (what was copied) and, when filed.json maps the
// slug to a URL, `issue_url`, and copies each entry's directory into
// site/public/whats-left/<slug>/.
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const opt = (name, dflt) => {
	const i = args.indexOf(name);
	return i >= 0 ? resolve(args[i + 1]) : dflt;
};

const manifestPath = opt('--manifest', join(root, 'docs/issues/manifest.json'));
const filedPath = opt('--filed', join(root, 'docs/issues/filed.json'));
const outData = join(root, 'site/src/data/issues.json');
const outPublic = join(root, 'site/public/whats-left');

const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
if (!Array.isArray(manifest)) throw new Error(`${manifestPath}: expected an array`);
const filed = existsSync(filedPath) ? JSON.parse(readFileSync(filedPath, 'utf8')) : {};

const required = ['slug', 'dir', 'title', 'body'];
const seen = new Set();
rmSync(outPublic, { recursive: true, force: true });
const out = manifest.map((issue) => {
	for (const k of required) if (!issue[k]) throw new Error(`issue ${issue.slug ?? '?'}: missing ${k}`);
	if (!/^[a-z0-9][a-z0-9-]*$/.test(issue.slug)) throw new Error(`bad slug ${issue.slug}`);
	if (seen.has(issue.slug)) throw new Error(`duplicate slug ${issue.slug}`);
	seen.add(issue.slug);
	const src = resolve(root, issue.dir);
	const dst = join(outPublic, issue.slug);
	mkdirSync(dst, { recursive: true });
	const files = existsSync(src)
		? readdirSync(src).filter((f) => !f.startsWith('.') && statSync(join(src, f)).isFile()).sort()
		: [];
	if (!files.length) console.warn(`warning: ${issue.slug}: no files in ${issue.dir}`);
	for (const f of files) copyFileSync(join(src, f), join(dst, f));
	// The body is written for GitHub; on the site its own page is the visual diff.
	const body = issue.body.replace(/^Visual diff: \{\{SITE\}\}\S*\n?/m, '').replace(/\{\{SITE\}\}/g, '/graphvis-rs');
	const entry = { ...issue, body, files };
	if (filed[issue.slug]) entry.issue_url = filed[issue.slug];
	return entry;
});

mkdirSync(dirname(outData), { recursive: true });
writeFileSync(outData, JSON.stringify(out, null, 1) + '\n');
console.log(
	`site-issues: ${out.length} issues from ${relative(root, manifestPath)}` +
		`${Object.keys(filed).length ? `, ${out.filter((i) => i.issue_url).length} filed` : ''}`,
);
