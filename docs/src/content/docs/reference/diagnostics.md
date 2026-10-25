---
title: Diagnostics and project analysis
description: How Pactole analyzes files, resolves includes and reports diagnostics.
sidebar:
  order: 5
---

Pactole distinguishes five kinds of diagnostics. The distinction matters for
tools such as the CLI and the language server, because not every diagnostic
requires the same context.

## Diagnostic sources

- **Syntax** diagnostics come from the tolerant Tree-sitter parser. They report
  `ERROR` and `MISSING` nodes.
- **Lowering** diagnostics occur when a syntactically recognizable directive
  cannot be converted into a domain entry.
- **Include** diagnostics describe a problem while resolving the file graph.
- **Business** diagnostics come from journal validation: account lifecycles,
  declarations, balancing, balance assertions and unused declarations.
- **Analysis** diagnostics report that part of the project could not be
  analyzed, and that some of the checks depending on it were skipped.

Every project diagnostic has a source file, a half-open source span, a severity,
an origin, a stable code and a human-readable message. Syntax and
lowering diagnostics are available even when the source is incomplete.

## Standalone analysis and project analysis

`pactole_storage_fs::analyze_file` analyzes one source buffer. It never reads
included files and deliberately does not run business validation, because a
single file does not provide enough project context to validate declarations,
account lifecycles or balances reliably.

`pactole_storage_fs::analyze_file_with_loader` analyzes a root path and follows
its `include` directives through a `SourceLoader`. It preserves the path and
source span of every diagnostic, including diagnostics from included files.
Consumers should use this API when they need complete-journal validation.

The LSP uses standalone analysis when no usable `journal_file` is configured.
When a root journal is configured, it analyzes the complete include graph and
publishes diagnostics separately for each file. Open buffers take precedence
over files on disk.

## Partial analysis

File analysis is directive-by-directive. A syntax or lowering error does not
prevent later directives from being inspected, so a `ParsedFile` can contain
both successfully lowered entries and diagnostics.

Project-level business validation survives a broken file. When part of the
project cannot be turned into a domain entry — a directive that fails to
lower, a chunk of source the parser cannot make sense of, an include whose
target cannot be loaded — the analysis becomes **partial**: the checks that do
not depend on the missing content are still reported, and the ones that do are
skipped.

Two axes drive this. The **declaration** axis covers the names whose
declaration may be missing (an account that may have been opened, a commodity
or a payee that may have been declared). The **usage** axis covers the names
the missing content may have used (the accounts, commodities and payees of a
lost transaction or balance assertion).

| Checks | Skipped when |
|---|---|
| `account-not-open`, `commodity-not-declared`, `payee-not-declared`, `commodity-not-allowed` | the name they are about may have been declared in the missing content |
| `balance-assertion-failed`, `account-unused`, `commodity-unused`, `payee-unused` | the name they are about may have been used by the missing content |

The checks that only depend on the entry they are about — the balancing of a
transaction, a duplicated declaration — are always reported.

An `AnalyzedProject` exposes the result with `is_partial()` and `partial()`.
The latter returns a `PartialAnalysis`, holding the aggregate uncertainty (a
`pactole_core::MissingData`) and the per-file breakdown (a list of
`FileLoss`). Every partial file also gets an `analysis-incomplete` warning,
pointing at the first piece of source that could not be analyzed.

An include cycle is a warning: all reachable content remains available, and
business validation continues.

## `AnalyzedProject` journal methods

An `AnalyzedProject` exposes two journal views:

### `journal()`

Returns the structurally valid journal in **source order**. Includes are
flattened at the position of the `include` directive, recursively. A diamond
include is visited once, so the same file is not duplicated when reached by two
paths.

This method does not run business validation and returns diagnostics when a
syntax error, lowering error or content-dropping include issue prevents a
structurally complete journal.

### `validated_journal()`

Returns the chronologically sorted and business-validated journal. Transactions
with one inferable amount are auto-balanced before the journal is returned.
Business diagnostics are included when validation fails.

The source-order journal and the validated journal intentionally serve different
purposes: source order is useful for tooling and provenance, while the
validated journal is the input expected by reports such as `register`.

## Include severities and `--strict`

The following include codes are stable in CLI and LSP output:

| Code | Severity | Meaning |
|---|---|---|
| `include-not-found` | error | The target file does not exist. |
| `include-load-failed` | error | The target exists but could not be loaded. |
| `include-no-base-directory` | error | A relative include has no usable base directory. |
| `include-depth-exceeded` | error | Include recursion exceeded the safety limit. |
| `include-cycle` | warning | Resolving the include would revisit a file already in progress. |

`pactole check` fails on errors and accepts warnings by default. Use
`pactole check --strict` when warnings must also fail, for example in CI.

## Diagnostic codes

Every diagnostic carries a stable code, printed by the CLI and by the
language server.

The catalogue is the public enum `pactole_core::DiagnosticCode`
(re-exported as `pactole_storage_fs::DiagnosticCode`):

- `DiagnosticCode::as_str()` is the single definition of the text written
  in the tables below;
- `DiagnosticCode::ALL` is the exhaustive list, checked against this page by
  the documentation-sync test;
- `DiagnosticCode::parse()` reads a code back, which is how the language
  server recognises the codes an editor sends it (an unknown code is simply
  ignored).

The text form is a stable contract across releases: the Rust variant names
and the messages may change, the strings in brackets may not.

### Syntax and lowering

| Code | Meaning |
|---|---|
| `syntax-error` | The tolerant Tree-sitter parser found an `ERROR` or a `MISSING` node. |
| `lowering-error` | A syntactically recognizable directive could not be converted into a domain entry. |

### Analysis completeness

| Code | Severity | Meaning |
|---|---|---|
| `analysis-incomplete` | warning | Part of the project could not be analyzed, so the checks depending on the missing content were skipped. |

### Business

Business codes are errors, except the two configurable checks — the
undeclared-payee check and the unused-declaration checks — whose severity is
set with the `[validation]` section of `pactole.toml` (and with the LSP
`validation` initialization option). Each level accepts `off` (never
reported), `warn` (reported as a warning) or `error` (blocking).

| Code | Level key | Default | Meaning |
|---|---|---|---|
| `account-already-open` | — | error | An account is opened while it is already open. |
| `account-not-open` | — | error | An account is used, or closed, while it is not open. |
| `commodity-already-declared` | — | error | A commodity is declared more than once. |
| `commodity-not-declared` | — | error | A commodity is used before being declared. |
| `price-not-positive` | — | error | A market-price rate is zero or negative. |
| `price-conflict` | — | error | Different rates are declared for the same directed commodity pair on the same date. |
| `payee-already-declared` | — | error | A payee is declared more than once. |
| `payee-not-declared` | `payee` | error | A transaction uses a payee with no `payee` declaration. |
| `too-many-postings-without-amount` | — | error | More than one posting of a transaction omits its amount; at most one may omit it, so that it can be inferred. |
| `ambiguous-auto-balance-commodity` | — | error | The commodity of the inferred amount cannot be determined from the other postings. |
| `transaction-not-balanced` | — | error | The postings of a transaction do not sum to zero for a commodity. |
| `commodity-not-allowed` | — | error | A posting uses a commodity excluded by the account's `open` restriction. |
| `balance-assertion-failed` | — | error | A `balance` assertion differs from the computed balance beyond its tolerance. |
| `account-unused` | `unused` | warn | An account is opened but neither a transaction nor a balance assertion uses it. |
| `commodity-unused` | `unused` | warn | A commodity is declared but no amount, price directive or `open` restriction uses it. |
| `payee-unused` | `unused` | warn | A payee is declared but no transaction uses it. |

These diagnostics point at the declared name itself (the account or
commodity token, the payee string). Merely `close`ing an account does not
count as a use: an account opened and closed without ever recording a
posting or a balance assertion is still reported.

At the default `warn` level they never prevent `validated_journal()` from
returning a journal: only error-severity diagnostics do.

## Core validation indices

The core validator reports a `ValidationIssue::input_index` for each business
error. This is the index of the entry in the journal supplied to the validator,
before chronological sorting. It is not a line number and does not refer to the
sorted journal. Storage and tooling layers use this index to recover source
provenance without making the core depend on parsers or file paths.

See also [Split a journal with includes](/how-to/split-a-journal-with-includes/),
[the CLI reference](/reference/cli/) and [the LSP guide](/how-to/use-the-lsp/).
