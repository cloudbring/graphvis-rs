/**
 * The repository's CHANGELOG.md as data, read at build time. The file is the single source:
 * the same file always gives the same page. Releases are the `## [x.y.z] - date` sections of
 * Keep a Changelog; a release with no entries (an empty Unreleased) is left out.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { marked } from 'marked';
import { REPO } from './data';

export type Release = {
	version: string;
	date: string | null;
	/** Long form of `date`, as `22 October 2026`. */
	day: string | null;
	slug: string;
	/** The release page on GitHub, or the compare view for Unreleased. */
	href: string | null;
	/** The diff against the previous release, from the file's link definitions. */
	diff: string | null;
	/** The paragraph before the first `###`, as HTML. */
	summary: string;
	sections: { title: string; html: string }[];
};

const text = readFileSync(join(process.cwd(), '../CHANGELOG.md'), 'utf8').replace(/\r\n/g, '\n');

const defs = new Map<string, string>();
for (const m of text.matchAll(/^\[([^\]]+)\]: (\S+)$/gm)) defs.set(m[1], m[2]);

const MONTHS = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];
const longDate = (d: string) => {
	const [y, m, day] = d.split('-').map(Number);
	return `${day} ${MONTHS[m - 1]} ${y}`;
};

// Issue and pull-request numbers link to GitHub (which redirects an issue number to its pull
// request); links relative to the repository root go to the file on GitHub.
function prepare(md: string) {
	return md
		.replace(/^\[[^\]]+\]: \S+$/gm, '')
		.replace(/(^|[\s(,;])#(\d+)\b/g, (_, pre, n) => `${pre}[#${n}](${REPO}/issues/${n})`)
		.replace(/\]\((?![a-z]+:|#|\/)([^)\s]+)\)/gi, (_, path) => `](${REPO}/blob/master/${path})`);
}
const html = (md: string) => marked.parse(prepare(md).trim(), { async: false }) as string;

export const releases: Release[] = text
	.split(/^## /m)
	.slice(1)
	.map((chunk) => {
		const nl = chunk.indexOf('\n');
		const head = chunk.slice(0, nl);
		const body = chunk.slice(nl + 1);
		const m = head.match(/^\[([^\]]+)\](?: - (\d{4}-\d{2}-\d{2}))?/);
		if (!m) throw new Error(`CHANGELOG.md: cannot read the release heading "## ${head}"`);
		const [, version, date = null] = m;
		const parts = body.split(/^### /m);
		const sections = parts.slice(1).map((p) => {
			const i = p.indexOf('\n');
			return { title: p.slice(0, i).trim(), html: html(p.slice(i + 1)) };
		});
		const diff = defs.get(version) ?? null;
		const unreleased = version === 'Unreleased';
		return {
			version,
			date,
			day: date && longDate(date),
			slug: unreleased ? 'unreleased' : 'v' + version.replace(/\./g, '-'),
			href: unreleased ? diff : `${REPO}/releases/tag/v${version}`,
			diff: diff && diff.includes('/compare/') ? diff : null,
			summary: prepare(parts[0]).trim() ? html(parts[0]) : '',
			sections,
		};
	})
	.filter((r) => r.summary || r.sections.length);

/** The newest release with a date: the version the site describes. */
export const latest = releases.find((r) => r.date) ?? null;
