---
title: Diagnostics and project analysis
description: How Pactole analyzes files, resolves includes and reports diagnostics.
sidebar:
  order: 4
---

Pactole distinguishes four kinds of diagnostics. The distinction matters for
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

Every project diagnostic has a source file, a half-open source span, a severity,
an origin, an optional stable code and a human-readable message. Syntax and
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

Project-level business validation is more conservative. If an include cannot
be loaded, the project is incomplete and business validation is suspended to
avoid reporting misleading follow-up errors caused by missing declarations or
transactions. The include error itself is still reported. An include cycle is
a warning: all reachable content remains available and business validation can
continue.

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

## Business diagnostic severities

Every business code is an error except the two checks below, whose severity
is configurable with the `[validation]` section of `pactole.toml` (and with
the LSP `validation` initialization option). Each level accepts `off` (never
reported), `warn` (reported as a warning) or `error` (blocking).

| Code | Level key | Default | Meaning |
|---|---|---|---|
| `payee-not-declared` | `payee` | error | A transaction uses a payee with no `payee` declaration. |
| `account-unused` | `unused` | warn | An account is opened but neither a transaction nor a balance assertion uses it. |
| `commodity-unused` | `unused` | warn | A commodity is declared but no amount uses it, and no `open` restricts an account to it. |
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
