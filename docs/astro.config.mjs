// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import beancount from '@shikijs/langs/beancount';
import { satteri } from '@astrojs/markdown-satteri';

const base = '/pactole';

// Markdown links are authored as site-root-relative paths. Prefix them at
// render time so they continue to work when the site is served from the
// repository subpath on GitHub Pages.
const baseLinks = {
	name: 'base-links',
	element: {
		filter: ['a'],
		visit(node, ctx) {
			const href = node.properties?.href;
			if (typeof href === 'string' && href.startsWith('/') && !href.startsWith('//')) {
				ctx.setProperty(node, 'href', base + href);
			}
		},
	},
};

// https://astro.build/config
export default defineConfig({
	site: 'https://etienne-monier.github.io',
	base,
	markdown: {
		processor: satteri({ hastPlugins: [baseLinks] }),
	},
	integrations: [
		starlight({
			title: 'Pactole',
			description:
				'Pactole is a plain-text double-entry accounting ledger tool, in the spirit of Beancount and Ledger-cli.',
			logo: {
				src: './src/assets/pactole-logo.svg',
				alt: 'Pactole',
			},
			// Pactole has no dedicated Shiki grammar yet. Its syntax is close
			// enough to Beancount's that we reuse that grammar, aliased to
			// `pactole` so that ```pactole fences get highlighted.
			expressiveCode: {
				shiki: {
					langs: [beancount],
					langAlias: { pactole: 'beancount' },
				},
			},
			editLink: {
				baseUrl: 'https://github.com/etienne-monier/pactole/edit/develop/docs/',
			},
			social: [
				{
					icon: 'github',
					label: 'GitHub',
					href: 'https://github.com/etienne-monier/pactole',
				},
			],
			sidebar: [
				{
					label: 'Tutorials',
					items: [{ autogenerate: { directory: 'tutorials' } }],
				},
				{
					label: 'How-to guides',
					items: [{ autogenerate: { directory: 'how-to' } }],
				},
				{
					label: 'Reference',
					items: [{ autogenerate: { directory: 'reference' } }],
				},
				{
					label: 'Explanation',
					items: [{ autogenerate: { directory: 'explanation' } }],
				},
			],
		}),
	],
});
