# Pactole Roadmap

This roadmap describes the product directions and concrete implementation
tasks to prioritize in order to make Pactole a reliable, pleasant-to-use,
text-based accounting tool.

Priorities are classified as follows:

- **P0**: foundation required for product reliability;
- **P1**: high user value, to be delivered next;
- **P2**: important improvements that depend on a more stable foundation;
- **P3**: exploratory or advanced features.

Tasks are intentionally small and should ideally result in one focused PR with
targeted tests. Checked items describe the known state when this document was
written; update this list as features evolve.

## General directions

### 1. Accounting correctness and trust

Pactole should detect not only whether a file is syntactically valid, but also
whether the journal respects accounting invariants and account lifecycles.
Errors should be localized, understandable, and reproducible in both the CLI
and the LSP.

### 2. Everyday editing experience

The target workflow is: edit, receive immediate diagnostics, navigate symbols,
format, then check or inspect a report. The LSP and CLI should expose the same
concepts without duplicating business logic.

### 3. Useful accounting reports

The register is the first detailed view. Pactole should also provide summary
views: balance sheet, income statement, cash flow, spending by category, and
period-over-period evolution, with output formats suitable for both the
terminal and external tools.

### 4. Imports and interoperability

The `foreign format -> ImportRecord -> Pactole model` pipeline should make it
possible to add CSV, QIF, and other formats without duplicating the
categorization engine. Imports should support previewing, idempotence, and
provenance tracking.

### 5. Project analysis and performance

Multi-file journals should preserve the provenance of every entry and remain
responsive. A cache and targeted invalidation are preferable to prematurely
complicating LSP synchronization.

### 6. Controlled evolution of the accounting model

Before adding prices, conversions, securities, or lots, Pactole should clearly
distinguish amount, commodity, price, cost, value, and tolerance. Model choices
should allow future extensions without requiring a complex accounting system
immediately.

### 7. Ecosystem and reusable APIs

The domain, analysis, reporting, and import libraries should remain reusable by
the CLI, the LSP, and future tools. Editor integrations should build on these
APIs instead of maintaining independent heuristics.

## Atomic implementations by priority

## P0 — Domain reliability and diagnostics

The public diagnostic and project-analysis contracts are documented in the
[diagnostics reference](/reference/diagnostics/), including standalone versus
project analysis, include severity, source-order journals and validated
journals.

### P0.1 — Stabilize the business diagnostic contract

- [x] Define a business diagnostic type with `kind`, `message`, severity, file,
  and `Span`.
- [x] Document the distinction between syntax, lowering, include, and business
  validation diagnostics.
- [x] Define how an error is attached to an entry, posting, account, or
  directive.
- [ ] Add tests converting business diagnostics to UTF-16 LSP diagnostics,
  including non-ASCII text.

### P0.2 — Add the simplest positioned business validations

- [x] Report a used but undeclared payee on the transaction payee.
- [x] Report a used but undeclared commodity on its occurrence.
- [x] Report an account used before its `open` directive on the invalid use.
- [x] Report an account used after its `close` directive on the invalid use.
- [x] Report inconsistent account openings or closings.
- [ ] Validate directive chronological order and locate the offending entry.
- [ ] Publish these diagnostics in the CLI `check` command and in the LSP with
  stable diagnostic codes.

### P0.3 — Diagnose transaction balancing

- [x] Expose the balancing difference per commodity in a structured error.
- [ ] Attach the error to the transaction and, when possible, to the posting
  that should be completed.
- [x] Distinguish a missing inferable amount, multiple implicit amounts, and a
  genuinely unbalanced transaction.
- [x] Test single-commodity, multi-commodity, implicit-amount, and tolerance
  cases.
- [x] Ensure analysis continues with subsequent transactions after an invalid
  transaction.

### P0.4 — Validate balance assertions

- [x] Calculate the balance at an assertion date without losing posting
  provenance.
- [x] Report the difference between the calculated and expected balance.
- [x] Position the error on the `balance` directive.
- [x] Cover assertions by commodity, unknown accounts, and boundary dates.

### P0.5 — Strengthen regression tests

- [ ] Add realistic multi-file journal fixtures.
- [x] Add CLI integration tests for each validation family.
- [x] Add LSP tests for publishing and clearing diagnostics.
- [ ] Add property tests for balancing and idempotent formatting.
- [x] Run `cargo fmt`, `cargo clippy --workspace --all-targets`, and
  `cargo test --workspace` consistently in CI.

## P1 — Editing and checking experience

### P1.1 — Expose formatting through the LSP

- [ ] Advertise `textDocument/formatting` in server capabilities.
- [ ] Call the existing canonical formatter without duplicating its logic.
- [ ] Return a document-wide `TextEdit` when formatting succeeds.
- [ ] Return a useful error when the document is too invalid to format.
- [ ] Add request-level tests and tests preserving already canonical text.

### P1.2 — Build a positioned symbol index

- [ ] Index accounts, payees, commodities, tags, and links with their
  definitions and occurrences.
- [ ] Store the file path and `Span` for every symbol.
- [ ] Merge symbols from all files reachable through `include`.
- [ ] Define behavior for duplicate or conflicting declarations.
- [ ] Replace completion's `BTreeSet<String>` values with this index where
  appropriate.

### P1.3 — Add LSP navigation

- [ ] Implement `textDocument/documentSymbol`.
- [ ] Implement `textDocument/definition` for accounts, payees, and
  commodities.
- [ ] Implement `textDocument/references` for the same symbols.
- [ ] Add navigation tests across included files.
- [ ] Return an empty result, rather than an error, when no definition exists.

### P1.4 — Improve the existing completion and hover features

- [ ] Use the positioned index for definitions and provenance.
- [ ] Only suggest accounts usable on the current date.
- [ ] Do not suggest a closed account for a transaction after its closing date.
- [ ] Complete `include` paths in subdirectories consistently.
- [ ] Show account opening and closing dates in hover information.
- [ ] Show the declaration file and line for a symbol.
- [ ] Document and test behavior for syntactically incomplete files.

### P1.5 — Add useful quick fixes

- [ ] Turn business diagnostics into structured LSP quick fixes.
- [ ] Offer to create a missing payee declaration.
- [ ] Offer to create a missing commodity declaration.
- [ ] Offer to insert an inferred amount when there is only one possible
  implicit posting.
- [ ] Keep the existing date-insertion quick fix and associate it with the
  relevant diagnostic when possible.

### P1.6 — Improve the CLI workflow

- [x] Display `check` errors as `file:line:column`.
- [ ] Add a structured JSON output option for diagnostics.
- [ ] Add an option or command to explicitly check the complete include graph.
- [x] Keep text output stable for scripts and CI.
- [ ] Extend `pactole init` with a journal skeleton and minimal explanations.

## P1 — Essential accounting reports

### P1.7 — Introduce a shared reporting API

- [ ] Define a period and reference-date abstraction.
- [ ] Define a result structure independent from text rendering.
- [ ] Preserve the commodity in every report line.
- [ ] Support text, JSON, and CSV rendering from the same result.
- [ ] Add tests for inclusive dates, parent accounts, and multiple commodities.

### P1.8 — Implement a balance sheet

- [ ] Produce account balances at a given date.
- [ ] Group accounts by hierarchical prefix.
- [ ] Separate assets, liabilities, and equity using a documented convention or
  configuration.
- [ ] Add the corresponding CLI command.
- [ ] Add fixtures and compare results against known journals.

### P1.9 — Implement an income statement

- [ ] Calculate income and expenses over a period.
- [ ] Support grouping by account, payee, and tag.
- [ ] Add period filters to the CLI.
- [ ] Produce readable text and JSON/CSV output.
- [ ] Document multi-commodity income statement behavior.

## P2 — Imports, exports, and projects

### P2.1 — Add configurable CSV import

- [ ] Define configuration for columns, separator, encoding, and date format.
- [ ] Convert each row to an `ImportRecord` without depending on the CLI.
- [ ] Reuse the `pactole-import` rules engine.
- [ ] Preserve a source line reference and provenance metadata.
- [ ] Add a `--preview` mode that does not modify a file.
- [ ] Add fixtures for several bank statement variants.

### P2.2 — Improve import idempotence and provenance

- [ ] Define a key strategy when the source format has no FITID.
- [ ] Report imported, skipped, and ambiguous transactions separately.
- [ ] Allow an import to be replayed without creating duplicates.
- [ ] Preserve the source filename and external identifier in metadata.
- [ ] Add a command to diagnose potential duplicates.

### P2.3 — Introduce project configuration

- [ ] Define a project-local configuration file.
- [ ] Configure the root journal file and shared LSP options in it.
- [ ] Define explicit resolution rules for relative paths.
- [ ] Allow the CLI and LSP to reuse this configuration.
- [ ] Document precedence between CLI options, project settings, and defaults.

### P2.4 — Add project analysis commands

- [ ] Display the `include` graph.
- [ ] Report cycles and files included more than once.
- [ ] Report `.pactole` files unreachable from the root.
- [ ] Expose project dependencies as JSON for external tools.
- [ ] Add tests for diamond includes and cycles.

### P2.5 — Add an analysis cache

- [ ] Cache file text, analysis results, and file dependencies.
- [ ] Invalidate a file when an open buffer changes.
- [ ] Invalidate dependents when an `include` resolution changes.
- [ ] Measure gains on multi-file journals before complicating the synchronization
  protocol.
- [ ] Keep the cache outside `pactole-storage-fs` if that better preserves its
  current responsibility.

## P2 — Accounting model and advanced ergonomics

### P2.6 — Formalize commodities, prices, and conversions

- [ ] Document the current commodity semantics precisely.
- [ ] Decide whether prices and conversions are in scope for the model.
- [ ] Add separate models for amount, price, cost, and value if needed.
- [ ] Define rounding and tolerance rules.
- [ ] Add multi-currency validations and reports before advanced user
  interfaces.

### P2.7 — Enrich interactive reports

- [ ] Show an account's current balance in LSP hover information.
- [ ] Add spending reports by payee and tag.
- [ ] Add reports for inactive accounts and uncategorized transactions.
- [ ] Add period-over-period comparisons.
- [ ] Evaluate Markdown or HTML output for shareable reports.

### P2.8 — Extend editor integrations

- [ ] Maintain a complete and tested Helix configuration.
- [ ] Document Neovim integration and supported LSP capabilities.
- [ ] Document Zed or VS Code support based on actual user demand.
- [ ] Add snippets for common directives and transactions.
- [ ] Test LSP positions with several clients, especially UTF-16 clients.

## P3 — Explorations to start only after stabilization

### P3.1 — Incremental LSP synchronization

- [ ] Measure the size and analysis time of real journals.
- [ ] Introduce `TextDocumentSyncKind::INCREMENTAL` only if full synchronization
  becomes a measured problem.
- [ ] Add tests for successive edits and UTF-16 positions.

### P3.2 — Alternative storage backends

- [ ] Stabilize storage traits and models before implementing another persistent
  backend.
- [ ] Define entry provenance and identity outside the filesystem.
- [ ] Evaluate a database storage crate only after reports and imports have
  stabilized.
- [ ] Do not create an empty storage crate or stub in advance.

### P3.3 — Graphical or web interface

- [ ] Reuse the existing domain, analysis, reporting, and import APIs.
- [ ] First define the use cases not covered by the CLI and LSP.
- [ ] Avoid turning Pactole into a graphical application before the format and
  accounting model are stable.

## Recommended delivery order

For the next iterations, the recommended order is:

1. positioned business diagnostics;
2. transaction balancing and balance assertions;
3. structured CLI output and validation tests;
4. LSP formatting;
5. positioned symbol index;
6. definitions, references, and document symbols;
7. business quick fixes;
8. balance sheet and income statement;
9. CSV import with preview and idempotence;
10. project configuration and project analysis;
11. analysis cache;
12. advanced multi-currency features, alternative storage, and a graphical
    interface.

This sequence maximizes user value while reusing the foundations already in
place: tolerant analysis, spans, include resolution, the domain model, the
canonical formatter, and the import pipeline.
