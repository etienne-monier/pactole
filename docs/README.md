# Pactole documentation

This directory contains the Pactole documentation website, built with
[Astro](https://astro.build/) and [Starlight](https://starlight.astro.build/).

## Structure

The content lives in `src/content/docs/` and follows the
[Diátaxis](https://diataxis.fr/) / [Divio](https://docs.divio.com/documentation-system/)
documentation system, split into four sections, each answering a different kind
of need:

| Section       | Directory       | Answers                             |
|---------------|-----------------|-------------------------------------|
| Tutorials     | `tutorials/`    | "I want to learn Pactole."          |
| How-to guides | `how-to/`       | "I want to accomplish a task."      |
| Reference     | `reference/`    | "I want to look something up."      |
| Explanation   | `explanation/`  | "I want to understand the ideas."   |

Each section has its own sidebar group; pages are ordered with the `sidebar.order`
frontmatter key.

## Commands

All commands are run from this directory:

| Command             | Action                                        |
|---------------------|-----------------------------------------------|
| `npm install`       | Install dependencies                          |
| `npm run dev`       | Start the dev server at `localhost:4321`      |
| `npm run build`     | Build the production site to `./dist/`        |
| `npm run preview`   | Preview the build locally before deploying    |
| `npm run astro ...` | Run CLI commands like `astro check`           |

## Where the content comes from

Much of the reference material is distilled from the repository's top-level
`README.md` and `GRAMMAR.md`, which remain the primary sources for the
command-line usage and the grammar respectively.
