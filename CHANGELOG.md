# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- OFX import supports several bank accounts: map each statement account
  identifier (OFX `ACCTID`) to its Pactole account in a `[accounts]` table of
  `import-ofx.toml`, or override everything for one run with
  `pactole import-ofx --account <ACCOUNT>`. A statement account absent from the
  mapping falls back to `bank_account` with a warning.
- OFX import decodes source bytes according to the encoding declared by the
  document (OFX header `ENCODING`/`CHARSET` fields or XML declaration), falling
  back to UTF-8 and then windows-1252, so statements in legacy code pages such
  as windows-1252 import their accented text correctly.
- OFX import reads the banking transactions of investment statements
  (`INVSTMTMSGSRSV1`).

### Fixed

- OFX statements whose `BANKACCTFROM` carries an `ACCTKEY` no longer import an
  empty transaction list.
- OFX SGML statements keep non-ASCII text (payee, narration) intact instead of
  corrupting multi-byte UTF-8 characters.
- OFX import fails with an explicit error when a statement clearly contains
  transactions but none can be read, instead of importing nothing silently.
- `pactole import-ofx --into` detects already-imported FITIDs through the
  `include` directives of the target journal, so re-importing into a split
  journal no longer duplicates transactions.

## [0.2.0] - 2026-10-16

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

This is the first public Pactole release. Version `0.1.0` was published
manually to reserve and connect the crates on crates.io; the first functional
release is `0.2.0`.

[unreleased]: https://github.com/etienne-monier/pactole/commits/develop
[0.2.0]: https://github.com/etienne-monier/pactole/releases/tag/v0.2.0
