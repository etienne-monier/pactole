---
title: Format a ledger
description: Rewrite a .pactole file into its canonical layout, in place or through a pipe.
sidebar:
  order: 5
---

`pactole fmt` rewrites a `.pactole` file into a single canonical layout:
consistent indentation and canonical single-space separators. Running it keeps
diffs small when several people — or automated tools — edit the same journal.

## Print the formatted result

```sh
pactole fmt ledger.pactole
```

The formatted content is written to stdout, leaving the file untouched.

## Write the file in place

```sh
pactole fmt --write ledger.pactole
```

## Format from a pipe

Pass `-` as the file to read from stdin. The result is always printed to stdout
(the `--write` flag is ignored in this mode):

```sh
cat ledger.pactole | pactole fmt -
```

This is useful in editor integrations. With Helix, for instance, pipe the
buffer through `pactole fmt -` to format on save.

## What gets normalized

Formatting only rewrites **whitespace**; it never changes the meaning of your
journal. Among other things, it:

- gives directive headers, `key: value` metadata and `account amount` posting
  lines canonical single-space separators (`=` and `:` stay glued to their
  neighbours, as in `date=effective_date` or `key: value`);
- indents the directive body with two spaces per level (four for a posting's own
  metadata);
- collapses a run of blank lines between two directives to a single one;
- keeps a trailing comment glued to the line it follows.

The textual content of leaves (dates, numbers, strings, accounts, comments,
tags, links, references) is never rewritten: the exact number of decimals or
the thousands separators you typed are preserved as-is.

Because the canonical form is stable, formatting an already-formatted file is a
no-op. `fmt` fails on a file that does not parse; fix the syntax error first
(see [Getting started](/tutorials/getting-started/) for how to spot one).

## Related

- [Command-line reference](/reference/cli/) — the full `fmt` options.
- [Grammar reference](/reference/grammar/) — the syntax `fmt` understands.
