// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import starlightLinksValidator from 'starlight-links-validator';

// Where the site is served: the root of https://graphviz-pure.github.io/ (the graphviz-pure.github.io
// repo publishes the build). SITE_BASE serves it under a path instead, e.g. /graphviz-pure/.
const SITE_URL = process.env.SITE_URL ?? 'https://graphviz-pure.github.io';
const BASE = process.env.SITE_BASE ?? '/';

export default defineConfig({
	site: SITE_URL,
	base: BASE,
	trailingSlash: 'always',
	integrations: [
		starlight({
			title: 'graphviz-pure',
			description:
				'A pure-Rust port of Graphviz 16.1.0, native and WebAssembly, that writes the same output on every platform. The evidence, and what is left.',
			logo: { light: './src/assets/mark-light.svg', dark: './src/assets/mark-dark.svg', alt: '' },
			favicon: '/favicon.svg',
			social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/graphviz-pure/graphviz-pure' }],
			customCss: [
				'@fontsource-variable/archivo/wdth.css',
				'@fontsource/fragment-mono/400.css',
				'./src/styles/proof.css',
			],
			components: {
				Footer: './src/components/overrides/Footer.astro',
				Hero: './src/components/overrides/Hero.astro',
			},
			expressiveCode: {
				themes: ['github-dark-dimmed', 'github-light'],
				styleOverrides: {
					borderRadius: '4px',
					borderWidth: '0.5px',
					borderColor: 'var(--rule-default)',
					codeFontFamily: "'Fragment Mono', ui-monospace, monospace",
					codeFontSize: '0.8125rem',
					uiFontFamily: 'var(--face)',
					frames: {
						frameBoxShadowCssValue: 'none',
						editorActiveTabIndicatorTopColor: 'var(--register)',
						editorActiveTabIndicatorHeight: '1px',
					},
				},
			},
			lastUpdated: false,
			pagination: false,
			sidebar: [
				{
					label: 'The claim',
					items: [
						{ label: 'Overview', link: '/' },
						{ label: 'Fidelity', link: '/fidelity/' },
						{ label: 'Verify it yourself', link: '/verify/' },
					],
				},
				{
					label: 'Evidence',
					items: [
						{ label: 'Gallery: C and Rust', link: '/gallery/' },
						{ label: 'Cross-platform', link: '/cross-platform/' },
						{ label: 'Playground', link: '/playground/' },
					],
				},
				{ label: "What's left", link: '/whats-left/' },
				{ label: 'Porting notes', items: [{ autogenerate: { directory: 'porting' } }] },
				{ label: 'API (rustdoc)', link: '/api/', attrs: { 'data-external-ish': '' } },
			],
			plugins: [
				starlightLinksValidator({
					// Content links are relative, so they work under any base path.
					errorOnRelativeLinks: false,
					// Custom pages (src/pages) and rustdoc (copied into dist/api by CI) are not in
					// the docs collection; scripts/site-check-links.mjs checks every built link.
					exclude: ['gallery/**', 'whats-left/**', 'playground/', 'cross-platform/', 'api/**', 'samples/**'].map((p) => BASE + p),
				}),
			],
		}),
	],
});
