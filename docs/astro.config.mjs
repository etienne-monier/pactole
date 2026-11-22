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
					items: [
						'tutorials/overview',
						'tutorials/getting-started',
						'tutorials/example-journal',
					],
				},
				{
					label: 'How-to guides',
					items: [
						{
							label: 'Write and organize a journal',
							items: [
								'how-to/install-pactole',
								'how-to/record-a-transaction',
								'how-to/reconcile-with-balance-assertions',
								'how-to/split-a-journal-with-includes',
								'how-to/format-a-ledger',
							],
						},
						{
							label: 'Import data',
							items: [
								'how-to/import-ofx-bank-statement',
								'how-to/migrate-from-actual',
							],
						},
						{
							label: 'Explore and report',
							items: [
								'how-to/query-with-register',
								'how-to/produce-financial-reports',
								'how-to/value-reports-in-another-currency',
							],
						},
						{
							label: 'Editors and interfaces',
							items: [
								'how-to/configure-editors',
								'how-to/use-the-lsp',
								'how-to/use-the-web-interface',
							],
						},
					],
				},
				{
					label: 'Reference',
					items: [{ autogenerate: { directory: 'reference' } }],
				},
				{
					label: 'Explanation',
					items: [
						'explanation/why-pactole',
						'explanation/double-entry-accounting',
						'explanation/concepts-and-glossary',
						'explanation/differences-from-beancount',
						'explanation/plain-text-accounting-landscape',
					],
				},
			],
		}),
	],
});
