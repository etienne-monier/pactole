# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `pactole-lsp` now reports its name and version: `pactole-lsp --version`
  prints `pactole-lsp <version>`, and the server announces the same in its
  `initialize` response (`serverInfo`), so editors can display it in their
  language server log. The `--stdio` flag is accepted and ignored, so
  editor configurations passing `args = ["--stdio"]` keep working.

### Documentation

- The documentation site gained a **configuration reference**
  (`/reference/configuration/`) consolidating the whole `pactole.toml` schema:
  the layers and their discovery order, the merge semantics, and the
  `[validation]`, `[format]`, `[declaration]` and `[import]` sections.
- The **diagnostics reference** now lists every diagnostic code
  (`syntax-error`, `lowering-error`, the business codes and the include codes)
  with its default severity and its meaning.
- The **differences from Beancount** page now contrasts the default status
  (`?` in Pactole, `*` in Beancount), the unsupported `commodity` label, the
  unsupported `open` booking metadata, and the `payee:`/`narration:` metadata
  that silently becomes free-form metadata when migrating a Beancount journal.
- `AGENTS.md` is now the single architecture reference: it documents the
  public API of every crate, and the orphaned `ARCHITECTURE.md` (and the links
  to it) has been removed.

### Changed

- Two integration tests now guard the documentation against drift:
  `pactole-cli`'s `docs_sync` compares the CLI surface with the reference and
  the README, and `pactole-storage-fs`'s `workspace_docs_sync` compares the
  directives, the diagnostic codes and the public API with the documentation.
  They are skipped when the repository's markdown files are not available.

## [0.3.0] - 2026-10-21

### Added

- Unused declarations are now reported: an `open`ed account that no
  transaction nor balance assertion uses (merely `close`ing it does not
  count as a use), a `commodity` that no amount uses (and no `open`
  restricts an account to), and a `payee` that no transaction uses each
  produce an `account-unused`, `commodity-unused` or `payee-unused`
  diagnostic pointing at the declared name itself. The detection lives in
  the new `pactole_core::unused_declarations`, which only reports and never
  fails a journal.
- The severity of those unused-declaration diagnostics is configurable with
  the new `[validation] unused` key of `pactole.toml` (`off`, `warn` or
  `error`, default `warn`): by default they show up as warnings in
  `pactole check` and in the language server, and only block under
  `--strict` (or when raised to `error`). The LSP accepts the same key
  through its `validation` `initializationOptions`, either as a single
  level applied to every configurable check or as a
  `{ "payee": ..., "unused": ... }` table.
- Unified `pactole.toml` configuration, resolved by the new
  `pactole-config` crate: a global layer
  (`~/.config/pactole/pactole.toml`, or `$XDG_CONFIG_HOME/pactole/`), a
   project layer (next to the journal), and in-memory overrides (the CLI's
   root `--config` flag, and the LSP's `initializationOptions`).
  It carries `[validation]`, `[format]` and `[import]` sections; unknown keys
  are rejected so typos fail loudly.
- Configurable validation severity: `[validation] payee = "warn"` reports
  transactions with an undeclared payee as warnings instead of errors (soft migration from
  Beancount), while every other business check stays an error. `pactole check
  --strict` still fails on warnings. The `off` level suppresses undeclared
  payee diagnostics.
- `pactole init` now creates a project `pactole.toml` next to the ledger
  (`--global` writes it into the user configuration directory instead).
- The language server reads the same layered configuration files as the CLI
  and accepts a `validation` `initializationOptions` key (a
  `"warn"`/`"error"` string applied to every configurable check, or a
  `{ "payee": ..., "unused": ... }` table); broken
  configuration files are logged and ignored instead of failing `initialize`.
- OFX import supports several bank accounts: map each statement account
  identifier (OFX `ACCTID`) to its Pactole account in an `[import.accounts]`
  table of `pactole.toml`, or override everything for one run with
  `pactole import-ofx --account <ACCOUNT>`. A statement account absent from
  the mapping falls back to `bank_account` with a warning.
- OFX import decodes source bytes according to the encoding declared by the
  document (OFX header `ENCODING`/`CHARSET` fields or XML declaration), falling
  back to UTF-8 and then windows-1252, so statements in legacy code pages such
  as windows-1252 import their accented text correctly.
- OFX import reads the banking transactions of investment statements
  (`INVSTMTMSGSRSV1`).
- The language server turns its business diagnostics into quick fixes: an
  undeclared payee, a not-yet-open account or an undeclared commodity can be
  declared straight from the diagnostic, inserted above the offending
  transaction — or into the declaration files configured by the new
  `[declaration]` section of `pactole.toml` (`payees`, `accounts`,
  `commodities`) when those files are reachable from the journal.
- The language server supports navigation: `textDocument/definition` jumps
  from an account, payee or commodity use to its declaration (and from an
  `include` path to the included file), and `textDocument/references` lists
  every occurrence of the symbol across the include graph of `journal_file`
  (or of the current document when none is configured).
- The language server supports renaming accounts and payees:
  `textDocument/prepareRename` offers the renameable range, and
  `textDocument/rename` edits every occurrence across the same include graph
  (payee edits keep the surrounding quotes). An invalid new name, or one that
  would create a duplicate `open` or `payee` declaration, is rejected with an
  error before anything is applied, as are a payee occurrence in an
  unterminated string and a rename when a configured `journal_file` cannot be
  analyzed.

### Changed

- OFX import rules are now written as `[[import.rules]]` inside `pactole.toml`
  instead of `[[rules]]` in a dedicated file.
- The minimum supported Rust version is now 1.90, which `tree-sitter` 0.27
  requires. The CI builds and tests the workspace with that toolchain to keep
  the advertised minimum honest.
- The version of every crate of the workspace now comes from a single place,
  the workspace manifest, and the crates depend on each other through the
  workspace dependencies. Local builds always use the crates of the workspace,
  while the published crates require the matching versions from crates.io.
- The CI now checks that the whole workspace can be published
  (`cargo publish --workspace --dry-run`), and that the `Cargo.lock` stays in
  sync with the manifests.

### Removed

- The standalone `format.toml` and `import-ofx.toml` configuration files:
  all configuration moved to the single `pactole.toml`, with no fallback.

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
- `pactole init` command to create a starter ledger.
- `pactole completions <shell>` command to generate shell completion scripts.
- Right-aligned amounts in `pactole fmt`, with a configurable alignment
  column.
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

Version `0.1.0` was published manually to reserve and connect the crates on
crates.io; `0.2.0` was the first functional release, and `0.3.0` adds the
layered `pactole.toml` configuration, the navigation and rename features of the
language server, and the OFX import fixes.

[unreleased]: https://github.com/etienne-monier/pactole/commits/develop
[0.3.0]: https://github.com/etienne-monier/pactole/releases/tag/v0.3.0
[0.2.0]: https://github.com/etienne-monier/pactole/releases/tag/v0.2.0
