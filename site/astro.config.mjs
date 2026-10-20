// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import starlightLinksValidator from 'starlight-links-validator';

export default defineConfig({
	site: 'https://cloudbring.github.io',
	base: '/graphvis-rs/',
	trailingSlash: 'always',
	integrations: [
		starlight({
			title: 'graphvis-rs',
			description:
				'A Rust + WebAssembly port of Graphviz 16.1.0, byte-identical to C Graphviz on its test corpora. The evidence, and what is left.',
			logo: { light: './src/assets/mark-light.svg', dark: './src/assets/mark-dark.svg', alt: '' },
			favicon: '/favicon.svg',
			social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/cloudbring/graphvis-rs' }],
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
					// Custom pages (src/pages) and rustdoc (copied into dist/api by CI) are not in
					// the docs collection; scripts/site-check-links.mjs checks every built link.
					exclude: ['/graphvis-rs/gallery/**', '/graphvis-rs/whats-left/**', '/graphvis-rs/playground/', '/graphvis-rs/cross-platform/', '/graphvis-rs/api/**', '/graphvis-rs/samples/**'],
				}),
			],
		}),
	],
});
