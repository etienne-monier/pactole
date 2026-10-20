---
title: The plain-text accounting landscape
description: Common pain points in Beancount and Ledger-cli, and where Pactole can stand apart.
sidebar:
  order: 4
---

Pactole is inspired by Beancount and Ledger-cli, but it does not need to
reproduce every feature of either project. Understanding the problems their
communities regularly discuss helps define a useful direction for Pactole.

The observations on this page are a synthesis of project documentation, issue
trackers and community discussions. Individual experiences vary, so these points
should be treated as recurring themes rather than universal measurements.

## Problems shared by plain-text accounting tools

### Entering data is harder than reading it

A text file is easy to inspect, search, diff and version with Git. It is not
necessarily easy to fill in correctly. A typical bank workflow involves:

1. downloading a statement;
2. converting it to the ledger format;
3. normalizing payees and descriptions;
4. choosing accounts;
5. removing duplicates;
6. handling transfers;
7. balancing and reconciling the result.

Most of this work happens outside the core ledger tool, in personal scripts or
several loosely connected importers.

### Errors are often discovered late

Many workflows only validate a journal when the user runs a command such as
`bean-check`, `ledger`, or `hledger`. Editor integrations can provide faster
feedback, but they are separate from the core tools and vary by editor.

This creates a gap between writing an entry and understanding whether it is
valid from both a syntax and an accounting perspective.

### Similar formats are not interchangeable

Ledger-cli, hledger and Beancount share double-entry ideas and broadly similar
textual notation, but their semantics differ. Simple transactions often convert
well. Costs, lots, prices, virtual postings, assertions, metadata and automated
transactions frequently need manual review during migration.

Plain text makes data ownership excellent, but it does not automatically make
data portable.

### Imports do not model the whole accounting workflow

An imported bank row is not always one accounting transaction. Transfers often
appear once on each account's statement, possibly with different identifiers,
dates or descriptions. A simple one-row-to-one-transaction importer can create
duplicates or leave the relationship between the two sides implicit.

## Ledger-cli: flexibility and complexity

Ledger-cli is powerful and expressive, but its flexibility can introduce a
steep learning curve:

- a large number of options and expressions;
- historical syntax and behaviours;
- subtle interactions between command-line options;
- inferences that can hide an omitted amount or account;
- complex cost, lot and gain semantics;
- reports that may require custom scripts for standard accounting views;
- a minimal native data-entry experience.

This is not an argument that Ledger's flexibility is wrong. It is a reminder
that a new tool can choose a smaller and more predictable semantic surface.

Pactole should avoid becoming a Ledger clone with a different implementation.
It can instead make a deliberate trade-off: fewer concepts, clearer validation,
and explicit behaviour.

## Beancount: rigor and ecosystem complexity

Beancount's strict validation is one of its major strengths. It catches many
errors that a more permissive tool can allow to remain hidden. The same
strictness can also create a significant onboarding barrier:

- accounts must be declared;
- commodities must be declared;
- payees may need declarations;
- transactions must balance strictly;
- booking, costs and lots require accounting concepts;
- importers and plugins commonly involve Python;
- the workflow spans Beancount, Beangulp, Fava, bean-query and third-party
  tools.

Python makes Beancount highly extensible, but users may also encounter
environment management, dependency compatibility, startup time or memory costs
when their journals and importers grow.

Pactole can retain strict validation while reducing the operational surface:
one native binary, one documented grammar, one CLI and an editor integration
based on the same parser and domain model.

## Where Pactole can stand apart

### 1. Make imports a first-class workflow

Pactole's `pactole-import` crate and `import-ofx` command are a strong starting
point. The long-term workflow should be:

```text
statement
  -> normalization
  -> duplicate detection
  -> categorization
  -> human review
  -> journal integration
  -> validation
  -> reconciliation
```

Useful extensions include:

- preview and import reports;
- source provenance;
- source fingerprints and namespaced identifiers;
- deduplication beyond a single FITID;
- CSV, QIF and CAMT support;
- explicit warnings for inferred or discarded information;
- matching the two sides of transfers;
- reproducible output from the same source and configuration.

The key promise should be **safe and reproducible imports**, not merely support
for another input format.

### 2. Make the editor part of the product

Pactole already has a tolerant syntax layer, positioned diagnostics, multi-file
analysis and an LSP. These can become a central user-facing advantage:

- syntax and business diagnostics while editing;
- completion for accounts, commodities and payees;
- navigation to declarations and references;
- hover information for symbols;
- code actions to create missing declarations or fix common errors;
- formatting through the LSP;
- diagnostics that follow included files and unsaved buffers.

This directly addresses the recurring complaint that errors are only found
after saving and running a separate command.

### 3. Keep semantics small and predictable

Pactole can differentiate through deliberate limits:

- a small number of directives;
- exact decimal arithmetic;
- canonical formatting;
- deterministic include resolution;
- explicit validation rules;
- limited inference, with every inference visible to the user;
- no hidden command-line mode that silently changes the meaning of the journal.

The language should remain capable without becoming a second programming
language embedded in a ledger.

### 4. Offer honest interoperability

Migration and conversion tools should report their limitations instead of
pretending every construct maps perfectly. A useful conversion report could
state:

```text
Converted: 1,240 transactions
Warnings: 12
Unsupported constructs: 3
Potentially lossy fields: 8
Entries requiring review: 5
```

This is more trustworthy than silently rewriting complex accounting semantics.

### 5. Provide useful reports without forcing scripts

Ledger-style generic reports are powerful, but many users eventually need
standard views such as:

- balance sheet;
- income statement;
- cash flow;
- account reconciliation;
- CSV and JSON exports.

Pactole can grow toward these reports while keeping the underlying journal
plain-text and scriptable.

## What not to prioritize first

Pactole does not need to immediately reproduce:

- every Ledger-cli expression and option;
- Beancount's full investment-booking model;
- a Python plugin ecosystem;
- a complete web application;
- every bank's proprietary export format.

The LSP, imports, deduplication, transfer reconciliation and structured reports
are likely to solve more everyday pain than immediate support for every
investment edge case.

## Suggested positioning

Pactole should not primarily be presented as "Ledger written in Rust" or
"Beancount but faster". A more durable positioning is:

> Pactole combines plain-text accounting with safe imports, editor-first
> validation and reproducible reconciliation.

In practical terms:

| Community pain point | Pactole opportunity |
| --- | --- |
| Manual data entry | Configurable importers and a review workflow |
| Late error discovery | LSP diagnostics with syntax and business context |
| Duplicate bank transactions | FITID, fingerprints and explicit deduplication |
| Ambiguous transfers | Transfer matching and reconciliation |
| Complex syntax | A deliberately smaller language |
| Python environment management | A native, self-contained binary |
| Lossy conversions | Conversion reports and explicit warnings |
| Reports requiring scripts | Native CLI, CSV and JSON outputs |
| Includes that are hard to debug | Multi-file analysis with provenance |

## References

- [A comparison of Beancount and Ledger/hledger](https://beancount.github.io/docs/a_comparison_of_beancount_and_ledger_hledger/)
- [hledger and Ledger](https://hledger.org/ledger.html)
- [hledger and Beancount](https://hledger.org/beancount.html)
- [Review of Plain Text Accounting](https://forum.plaintextaccounting.org/t/review-of-plain-text-accounting-an-opinionated-view-2022/403)
- [Ledger performance issue](https://github.com/ledger/ledger/issues/3202)
- [Fava and Beangulp compatibility discussion](https://github.com/beancount/fava/issues/1831)
- [Ledger-like tools discussion](https://news.ycombinator.com/item?id=28421284)
