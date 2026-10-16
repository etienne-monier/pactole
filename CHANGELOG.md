# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Plain-text, double-entry accounting journal format defined by
  `GRAMMAR.md`.
- Tree-sitter grammar, generated parser, Rust bindings, and editor highlight
  queries for `.pactole` files.
- Core accounting domain models for journals, entries, transactions, postings,
  amounts, balances, accounts, commodities, payees, metadata, and tags.
- Domain validation for transaction balancing, account lifecycles, declared
  commodities, declared payees, and chronological ordering.
- Register reports with account, date, payee, narration, status, and tag
  filters.
- Tolerant syntax analysis with positioned spans and syntax diagnostics.
- Filesystem storage with recursive `include` resolution and canonical
  formatting.
- Positioned, directive-by-directive analysis that continues after invalid
  directives.
- Multi-file analysis through an abstract source loader, including support for
  in-memory source buffers and include-cycle reporting.
- `pactole` CLI commands for parsing, checking, formatting, and register
  reports.
- OFX/QFX import support for SGML 1.x and XML 2.x statements.
- Import rules for mapping external records to balanced Pactole transactions,
  including FITID-based idempotence.
- Minimal stdio LSP server with full-document synchronization and diagnostics
  for open files and included files.
- LSP completion for directives, accounts, payees, commodities, tags, links,
  include paths, and dates.
- LSP hover information for accounts, payees, commodities, tags, and links.
- LSP quick fix for adding a missing date to an undated directive.
- Helix and Zed editor integration resources, including indentation,
  injection, local-variable, tag, and text-object queries where supported.

### Changed

- Business diagnostics now target the most precise available source span for
  payees, posting accounts, commodities, and balance assertions, while
  transaction-level diagnostics retain the transaction span when no posting
  index is available.
- The LSP prefers open, unsaved document buffers when resolving included files,
  and falls back to the filesystem for unopened files.
- LSP diagnostics are published per source file and stale diagnostics are
  cleared when a file leaves the analyzed include graph.
- The canonical formatter remains in `pactole-storage-fs` and is shared by
  CLI-facing formatting and transaction serialization.
- The import pipeline is separated into format parsing, format-neutral records,
  domain conversion, and output serialization so future importers can reuse the
  same rule engine.

### Fixed

- Preserve UTF-16 character positions when converting byte-based syntax spans
  to LSP ranges, including sources containing non-ASCII characters.
- Keep diagnostics available for valid directives when another directive in the
  same file cannot be lowered.
- Prevent diagnostics from remaining stale after included files are removed or
  the configured journal becomes unavailable.

## Release notes

No versioned release has been published yet. Move the relevant entries from
`[Unreleased]` into a dated version section when the first release is created.

[unreleased]: https://github.com/etienne-monier/pactole/commits/develop
