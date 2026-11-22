---
title: Pactole at a glance
description: Understand Pactole's files, tools and recommended workflow before starting.
sidebar:
  order: 1
---

Pactole is a plain-text, double-entry accounting tool. You keep the source of
truth in readable `.pactole` files, then use Pactole to validate, format,
import and report on those files.

This page gives you the map of the project. If you want to start immediately,
continue with [Getting started](/tutorials/getting-started/).

## The main pieces

| Piece | Purpose |
|---|---|
| `.pactole` files | The journal: declarations, transactions and assertions. |
| `pactole.toml` | Project and user configuration for validation, formatting, reports and imports. |
| `pactole check` | Checks syntax, includes and accounting invariants. |
| `pactole fmt` | Writes a stable canonical layout. |
| `pactole register` | Lists postings with running balances. |
| `pactole balance` | Builds a balance sheet at a reference date. |
| `pactole income` | Builds an income statement over a period. |
| `pactole expenses` | Breaks spending down by account, payee, tag or month. |
| `pactole import-ofx` | Turns a bank statement into journal transactions. |
| `pactole serve` | Serves a local, read-only web interface. |
| `pactole-lsp` | Adds diagnostics and editing tools to supported editors. |

## A normal workflow

Most projects follow this cycle:

```text
initialize the project
        ↓
write transactions or import them
        ↓
format the files
        ↓
check the complete project
        ↓
inspect the register and reports
        ↓
commit the text files with Git
```

The commands have deliberately different jobs:

- `init` creates a starting ledger and configuration;
- `fmt` changes layout, but is not a replacement for validation;
- `check` validates the journal before reports are built;
- reports read the validated journal and do not modify it;
- imports produce transactions which may need to be merged into a complete
  journal before they can pass `check`.

## One project, several views

The same journal can be used from several interfaces:

- the CLI is the complete automation and reporting interface;
- the web interface is a convenient local read-only dashboard;
- the language server brings diagnostics, completion, navigation, rename and
  formatting into an editor.

They all use the same journal rules. The web interface and the language server
do not create a second accounting model.

## Choose a path

| If you want to… | Read… |
|---|---|
| Create your first journal | [Getting started](/tutorials/getting-started/) |
| Record a transaction | [Record a transaction](/how-to/record-a-transaction/) |
| Split a project across files | [Split a journal with includes](/how-to/split-a-journal-with-includes/) |
| Import bank data | [Import an OFX bank statement](/how-to/import-ofx-bank-statement/) |
| Migrate from Actual Budget | [Migrate from Actual Budget](/how-to/migrate-from-actual/) |
| Build financial reports | [Produce financial reports](/how-to/produce-financial-reports/) |
| Convert reports to another commodity | [Value reports in another currency](/how-to/value-reports-in-another-currency/) |
| Configure an editor | [Configure editors](/how-to/configure-editors/) |
| Understand a term or concept | [Concepts and glossary](/explanation/concepts-and-glossary/) |
| Find exact syntax or options | [Reference](/reference/grammar/) |

## What Pactole does not do

Pactole does not hide the journal behind a proprietary database. The text files
remain the source of truth, and the web interface is read-only. It also keeps a
deliberately small language: see [Differences from Beancount](/explanation/differences-from-beancount/)
for features that are not currently part of Pactole.
