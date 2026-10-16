# AGENTS.md

Instructions and project reference for AI coding agents working on **Pactole**.

---

## 1. Project Overview

Pactole is a plain-text double-entry accounting ledger tool written in Rust, inspired by [Beancount](https://beancount.github.io/docs/) and Ledger-cli.

Users record financial transactions in human-readable, git-friendly `.pactole` files. The project provides:
- A tree-sitter grammar and highlight queries for `.pactole` files.
- A core domain library with validation and reporting logic.
- A filesystem storage parser and canonical formatter.
- A command-line interface (`pactole`).

---

## 2. Workspace & Architecture

Pactole is structured as a Cargo workspace, currently with seven crates. A future
database-backed storage crate is anticipated but not yet created; see "Planned Crates" below.

```text
pactole/
├── crates/
│   ├── tree-sitter-pactole/   # Tree-sitter grammar, C parser, and Rust bindings
│   ├── pactole-core/          # Pure domain models, validation, and reports
│   ├── pactole-syntax/        # Tolerant CST analysis, spans, and syntax diagnostics
│   ├── pactole-storage-fs/    # Tree-sitter-based parser, includes, and canonical formatter
│   ├── pactole-import/        # Import external formats (OFX) into domain models
│   ├── pactole-lsp/           # Minimal stdio Language Server Protocol server
│   └── pactole-cli/           # Command-line binary (`pactole`) and CLI tests
├── GRAMMAR.md                 # Specification of the .pactole syntax
├── HELIX.md                   # Integration guide for the Helix editor
└── Taskfile.yml               # Task runner definitions (e.g., documentation)
```

### Crate Responsibilities & Boundaries

1. **`pactole-core`**:
   - **Zero I/O and Zero Tree-sitter**: Contains only domain entities, business validation, and financial calculations. Must never depend on `pactole-storage-fs`, `pactole-syntax`, `tree-sitter`, or filesystem operations.
   - **Key modules**:
     - `models/`: Domain structs (`Journal`, `Entry`, `Transaction`, `Posting`, `Amount`, `Balance`, `Open`, `Close`, `Commodity`, `Payee`, etc.) and newtypes (`AccountName`, `CommodityName`, `MetadataKey`).
     - `validation.rs`: Invariant checks on `Journal` (chronological ordering, account lifecycles, declared commodities, declared payees, transaction balancing).
     - `register.rs`: Register report computation (running balance per commodity, filtering by account, date range, payee, narration, status, tag).
     - `traits.rs`: Storage abstractions like `ReadableStorage`.
     - `errors.rs`: `ModelError`, `ValidationError`.
   - **Arithmetic**: Always uses `rust_decimal::Decimal` (never floating-point numbers).

2. **`tree-sitter-pactole`**:
   - Syntax definition in `grammar.js`.
   - Generated parser files (`src/parser.c`, `src/grammar.json`, `src/node-types.json`).
   - Rust binding via `bindings/rust/lib.rs` and `bindings/rust/build.rs`.
   - Editor highlight queries in `queries/highlights.scm`.
   - Corpus and test files under `test/`.

3. **`pactole-syntax`**:
   - **Zero I/O and zero dependency on `pactole-core`**: wraps `tree-sitter-pactole` to provide
     *tolerant* syntax analysis of `.pactole` source text — parsing always succeeds and never
     returns a hard failure, even for invalid input.
   - **Key modules**:
     - `span.rs`: `Point` (byte offset + zero-indexed row/column) and `Span` (start/end `Point`
       pair, half-open, with a `byte_range()` helper for slicing source text).
     - `diagnostics.rs`: `SyntaxDiagnostic` (a `DiagnosticKind`, a `Span`, and a message) and
       `DiagnosticKind` (`Error` for tree-sitter `ERROR` nodes, `Missing` for "missing" nodes).
     - `document.rs`: `ParsedDocument` (source + tree-sitter `Tree` + collected diagnostics),
       `parse_document`/`analyze` (tolerant parsing entry points), and `collect_error_nodes`
       (walks a tree-sitter `Tree` and collects `ERROR`/`MISSING` nodes as `SyntaxDiagnostic`s).
   - **Scope for now**: purely syntactic. It does **not** convert CST nodes into `pactole-core`
     domain models — that conversion remains the responsibility of `pactole-storage-fs::parser`.
     This crate is intended as the shared foundation for future tooling (e.g. `pactole-lsp`) that
     needs positioned diagnostics on possibly-invalid text, without duplicating tree-sitter
     plumbing across consumers.

4. **`pactole-storage-fs`**:
   - Implements `ReadableStorage` for local files (`PactoleFileStorage`).
    - `analysis.rs`: Converts tree-sitter CST nodes into positioned domain
      entries, resolves includes through `SourceLoader`, and exposes
      source-order and validated journals with unified diagnostics.
    - `parser.rs`: Contains the shared CST-to-domain lowering logic used by
      `analysis.rs`; include resolution is no longer implemented here.
   - `printer.rs`: Formats AST/source code into canonical `.pactole` layout. Also exposes
     `format_transactions(&[Transaction]) -> String`, which serializes a list of domain
     transactions into canonical `.pactole` text (used by `pactole import-ofx`; the inverse of
     parsing for that subset of directives).
   - `errors.rs`: `PactoleFsStorageError`.
   - **Reuses `pactole-syntax`**: parser initialization and syntax-error detection go through
     `pactole_syntax::parse_document`/`ParsedDocument::into_result`, so tree-sitter parser setup
      and `ERROR`/`MISSING` detection are not duplicated here. `analysis.rs` and `printer.rs` walk
      the resulting `ParsedDocument` tree for lowering and canonical formatting respectively.
    - **Stability note**: `PactoleFileStorage` and `ReadableStorage` remain
      available. Their parsing path now delegates to the same loader-based
      project analysis as the CLI and LSP. The canonical formatter stays in
      `pactole-storage-fs` for now; it is not moved or duplicated into
      `pactole-syntax`.
    - **`analysis.rs`** (public API: `analyze_file`, `ParsedFile`, `ParsedEntry`, `ParsedInclude`,
      `LoweringDiagnostic`, `Diagnostic`): a positioned, directive-by-directive analysis for tooling (editors, a
     future `pactole-lsp`) that needs spans and per-directive diagnostics instead of an
     all-or-nothing `Journal`. `analyze_file` never fails, even on syntactically invalid or
     partially invalid input:
      - It reuses `pactole-syntax::parse_document` for tolerant parsing and the
        shared `AstBuilder::build_directive_outcome` lowering logic — there is
        no second parser.
     - Each top-level `directive` node is lowered independently: on success it becomes a
       `ParsedEntry { span, entry }` (reusing `pactole_syntax::Span` and `pactole_core::Entry`);
       on failure it becomes a `LoweringDiagnostic { span, message }` and analysis continues with
        the next directive.
      - `include` directives are detected as `ParsedInclude { span, path }` and
        recursively resolved by `analyze_file_with_loader` through a
        `SourceLoader`, while preserving file provenance.
      - `AnalyzedProject::journal()` returns the source-order flattened journal, while
        `validated_journal()` returns the chronologically validated and auto-balanced journal.
      - `ParsedFile::syntax_diagnostics()` exposes the underlying `pactole-syntax`
       `SyntaxDiagnostic`s (`ERROR`/`MISSING` nodes) unchanged, so callers can distinguish
       syntax-level issues from lowering-level ones.
     - No `Span`/tree-sitter type leaks into `pactole-core`: `ParsedEntry`/`ParsedInclude`/
       `LoweringDiagnostic` live in `pactole-storage-fs` and only wrap `pactole_syntax::Span`
       around existing `pactole-core` types.
     - **`loader.rs`** (public API: `SourceLoader`, `SourceLoadError`, `FsSourceLoader`,
       `InMemorySourceLoader`) and **multi-file analysis** (public API:
       `analyze_file_with_loader`, `AnalyzedProject`, `AnalyzedFile`, in
       `analysis.rs`): a minimal abstraction over "where source text comes from", so
       multi-file/include resolution is not tied to reading real files from disk.
       - `SourceLoader` is a small trait (`fn load(&self, path: &Path) -> Result<String,
         SourceLoadError>`) that only loads raw source text for an already-resolved path; it knows
         nothing about `.pactole` syntax. `FsSourceLoader` reads real files for project analysis and
         `PactoleFileStorage`; `InMemorySourceLoader` serves
         content from an in-memory `path -> String` map, standing in for a future `pactole-lsp`
         loader backed by the editor's open (possibly unsaved) buffers.
       - `analyze_file_with_loader(entry_path, loader)` recursively follows `include` directives
         through a `&dyn SourceLoader`, reusing `analyze_file` per file. Each file keeps its own
         `ParsedFile` inside an `AnalyzedFile { path, file }`, while `AnalyzedProject::journal()`
         and `validated_journal()` provide flattened journals when needed.
       - It never fails on the *content* of included files: a relative include resolved without a
         reachable base directory, a target the loader cannot provide, or an include cycle are all
         reported as positioned include diagnostics on `AnalyzedProject` rather than
         aborting the whole analysis. Only a failure to load `entry_path` itself is returned as an
         `Err`, since there is then nothing to analyze at all. A "diamond" include (the same file
         reached through two different paths) is visited once; a self-include or mutual cycle is
         detected via a stack of in-progress paths and reported as an include diagnostic, not lowered.
       - `analyze_file(&str) -> ParsedFile` is kept exactly as-is (single-file, no include
         resolution) for backward compatibility; `analyze_file_with_loader` is strictly additive.
       - Include diagnostics are exposed through `AnalyzedProject::diagnostics()` and
         `structural_diagnostics()` as positioned `Diagnostic` values; the internal include issue
         representation is not part of the public API.
       - Kept intentionally minimal: no caching, no incremental re-analysis, and no attempt to
         track which loaded files a given analysis depends on for invalidation — a future
         `pactole-lsp` is expected to build that on top of this contract rather than this crate
         growing it prematurely.

6. **`pactole-cli`**:
   - Command-line application binary named `pactole` (using `clap` derive).
   - Subcommands:
      - `pactole parse <file>`: Parse file and debug-print journal entries.
      - `pactole fmt <file> [--write]`: Format `.pactole` file canonically (supports `-` for stdin).
      - `pactole completions <shell>`: Generate shell completion scripts.
      - `pactole check <file>`: Syntax parsing + business validation.
      - `pactole register <file> [filters]`: Register report with running balances.
      - `pactole init [ledger]`: Initialize a starter ledger and OFX configuration.
     - `pactole import-ofx <file> [--config <path>] [--into <path>] [--write]`: Import an OFX/QFX
       statement (SGML 1.x or XML 2.x) into canonical `.pactole` transactions, via
       `pactole_import::import_ofx` + `pactole_storage_fs::format_transactions`. Configuration
       (bank account, default counter account, optional fallback currency, `[[rules]]`) is read
       from `~/.config/pactole/import-ofx.toml` by default (overridable with `--config`). Without
       `--into`, transactions are printed to stdout; with `--into <file>`, FITIDs already present
       in that file are skipped (idempotence), and with `--write` the generated transactions are
       appended to it.
   - Integration tests in `crates/pactole-cli/tests/cli.rs`.

5. **`pactole-lsp`**:
   - A minimal stdio Language Server Protocol server for `.pactole` files, built on `lsp-server` +
     `lsp-types`. Binary name: `pactole-lsp`.
   - **Lifecycle & sync**: handles `initialize`/`shutdown`/`exit`, and
     `textDocument/didOpen`/`didChange`/`didClose` using **full-document synchronization**
     (`TextDocumentSyncKind::FULL`) — each `didChange` carries the whole new text, so no
     LSP-position-to-byte-offset translation is needed to apply edits. This is a deliberate
     simplicity/robustness trade-off over incremental sync, acceptable for typically small,
     hand-edited `.pactole` files; revisit if editing very large journals becomes a problem.
   - **Key modules**:
     - `documents.rs`: `Documents`, the in-memory map of open document URI to its current full
       text, plus `DocumentsSourceLoader`, a `pactole_storage_fs::SourceLoader` that serves an
       open (possibly-unsaved) document's buffer when one exists for a path, falling back to
       `FsSourceLoader` otherwise. This makes open, unsaved edits to an *included* file take
       precedence over its last-saved-to-disk content when resolving `include`s.
     - `conversion.rs`: `span_to_range`/`point_to_position`, converting `pactole-syntax`'s
       byte-offset `Span`/`Point` into LSP's UTF-16-based `Range`/`Position` by re-scanning the
       relevant source line (a byte offset alone does not determine a UTF-16 character offset
       once the source has non-ASCII characters).
      - `diagnostics.rs`: `standalone_diagnostics` (syntax + lowering diagnostics for one
       `pactole_storage_fs::ParsedFile`) and `project_diagnostics` (per-file diagnostics, plus
       include issues attached to the file containing the offending `include`, for an
       `AnalyzedProject`).
     - `config.rs`: `Config`, resolved once from `initializationOptions` at `initialize` time.
       Currently supports a single optional key, `journal_file` (path to the root `.pactole`
       file, resolved against the workspace root — from `workspace_folders`, then `rootUri`, then
       `rootPath` — when given as relative; when no workspace root is known at all, resolved
       against the server process's current working directory instead of being silently left
       relative, so it does not later fail to resolve and silently drop every diagnostic for the
       whole include graph). **Does not read `~/.config/pactole`** or any other user-global
       configuration; this is an explicit, documented limitation of this first version, not an
       oversight.
     - `server.rs`: the stdio `Connection`, main loop, and notification handlers. Publishes
       `textDocument/publishDiagnostics`:
       - When `journal_file` is configured and loads successfully, diagnostics are computed for
         *every file reachable via `include`* using `pactole_storage_fs::analyze_file_with_loader`
         with `DocumentsSourceLoader` (open buffers first, filesystem fallback), and published
         per-file (keyed by each file's own path/URI).
       - Otherwise (no `journal_file`, or it fails to load), the current document alone is
         analyzed standalone via `pactole_storage_fs::analyze_file` (its `include`s are *not*
         followed in this fallback path).
       - The main loop tracks the set of URIs published with diagnostics by the previous
         `publish_for` call; any URI no longer covered by the latest one (e.g. an `include` was
         removed, dropping a file out of the project) is published an empty diagnostics list, so
         diagnostics never go stale once a file drops out of the analyzed set.
    - Completion, code actions and hover are supported. Formatting over LSP (use `pactole fmt`),
      navigation, incremental sync, and reading `~/.config/pactole` remain out of scope for this
      first version. Unhandled requests are answered with a `MethodNotFound` error rather than
      silently ignored.
   - **Depends on `pactole-storage-fs`/`pactole-syntax` only through their existing public API**
     (`analyze_file`, `analyze_file_with_loader`, `FsSourceLoader`, `SourceLoader`,
      `SourceLoadError`, `ParsedFile`, `AnalyzedProject`, `SyntaxDiagnostic`,
     `Span`/`Point`); it does not add or change anything in those crates.
   - Has no dependency on `thiserror`: it has no error enums of its own, only propagating errors
     via `Box<dyn Error + Sync + Send>` in `server.rs`.

7. **`pactole-import`**:
   - **Edge adapter, zero I/O, depends only on `pactole-core`** (plus `chrono`, `rust_decimal`,
     `serde`): translates a foreign input format into pure domain models. It knows nothing about
     tree-sitter, `pactole-storage-fs`, the `.pactole` text format, or the configuration file
     format, so it is reusable by any front-end (the `pactole` CLI today, a future
     `pactole-storage-db`, the LSP tomorrow).
   - **Key modules**:
     - `record.rs`: `ImportRecord`, a format-neutral intermediate representation (date, signed
       amount, currency, payee, memo, transaction type, check number, reference, FITID).
     - `ofx.rs`: OFX parser (1.x SGML and 2.x XML, via the `ofx-rs` crate) producing
       `ImportRecord`s.
     - `config.rs`: `ImportConfig`, `Rule`, `Direction` — serde types describing the bank
       account, the default counter account, the optional fallback currency and the
       categorization rules. These types fix no file format; the caller chooses TOML.
     - `rules.rs`: shared rule engine lowering an `ImportRecord` into a balanced
       `pactole_core::Transaction` (first matching rule wins, otherwise the default counter
       account; a rule may also override the payee).
     - `error.rs`: `ImportError`.
   - **Public API**: `import_ofx(source, &config, &HashSet<String>) -> Result<ImportOutcome,
     ImportError>`, plus `ImportConfig`, `Rule`, `Direction`, `ImportRecord`, `ImportOutcome`,
     `ImportWarning`, `ImportError`. `import_ofx` skips any source transaction whose FITID is
     already in `known_fitids` (reported in `ImportOutcome::skipped`), and stores each produced
     transaction's FITID as `fitid` metadata so a later run can rebuild `known_fitids`.
   - **Deliberate two-step pipeline** (format parser → `ImportRecord`, then shared rule engine →
     `Transaction`) so future importers (CSV, QIF, ...) reuse the rule engine instead of
     duplicating it. Only OFX is implemented for now.
   - **Serialization stays out of this crate**: turning the produced `Transaction`s into
     `.pactole` text is `pactole_storage_fs::format_transactions`, called by the CLI.

### Planned Crates (not yet created)

- **A future database-backed storage crate** (e.g. `pactole-storage-db`): expected to implement
  `pactole-core::traits::ReadableStorage` (and possibly a writable counterpart) against a
  database instead of the filesystem, mirroring `pactole-storage-fs`'s role but without
  tree-sitter/file-parsing concerns.

This crate is referenced here for architectural planning only; do not create a stub crate for it
unless explicitly requested.

---

## 3. Pactole Language Specifications & Invariants

Refer to `GRAMMAR.md` for complete grammar details. Key invariants:

- **Directives** start at column 0 on a header line:
  - `DATE open ACCOUNT`
  - `DATE close ACCOUNT`
  - `DATE commodity COMMODITY`
  - `payee "NAME"` (undated, global declaration)
  - `DATE balance ACCOUNT NUMBER [~ TOLERANCE] COMMODITY`
  - `DATE[=EFFECTIVE_DATE] STATUS "payee" ["narration"] [TAGS/LINKS] [(ref)]`
  - `include "path"`
- **Indentation & Directive Structure**:
  - Metadata lines (`key: value`) must immediately follow the directive header **before** any transaction postings.
  - Transaction postings are indented.
  - Metadata belonging to a posting is indented underneath that posting.
- **Payees**:
  - Transactions require a mandatory payee string.
  - In business validation (`check`), every transaction's payee must be declared in the journal via a `payee` directive.
- **Transaction Balancing**:
  - All postings in a transaction must sum to zero per commodity.
  - At most one posting can omit an amount (its amount is inferred).
- **Account Lifecycles**:
  - Accounts must be opened before being used in transactions, balance assertions, or closed.
  - Closed accounts cannot be used after their close date.

---

## 4. Development & Verification Workflows

When working on this repository, run the following commands to validate changes:

### Building and Testing

```sh
# Run all workspace unit and integration tests
cargo test --workspace

# Run only CLI integration tests
cargo test -p pactole-cli --test cli

# Run tests for a specific crate
cargo test -p pactole-core
cargo test -p pactole-syntax
cargo test -p pactole-storage-fs
cargo test -p pactole-import
cargo test -p pactole-lsp
```

### Code Quality & Linting

```sh
# Check lints across the workspace
cargo clippy --workspace --all-targets

# Check formatting
cargo fmt --check
```

### Git Hooks & CI

A `prek` (pre-commit-compatible) configuration lives in
`.pre-commit-config.yaml`. Install the hook once per clone:

```sh
prek install
```

The pre-commit hook runs `cargo fmt --all`, applies the automatic clippy fixes
and then fails on any remaining lint. The same checks, plus `cargo test`,
`cargo doc --workspace --no-deps`, `cargo-deny` (dependency licenses and
RustSec advisories) and the tree-sitter grammar checks, run in GitHub Actions
on every push to `develop` and every pull request
(`.github/workflows/ci.yml`). Publishing the crates to crates.io is triggered
by pushing a `v*` tag (`.github/workflows/release.yml`).

### Documentation

```sh
# Build workspace documentation without external dependencies
cargo doc --workspace --no-deps
# or using task
task doc
```

### Tree-sitter Grammar Workflow

If modifying `crates/tree-sitter-pactole/grammar.js`:
1. Regenerate parser files:
   ```sh
   cd crates/tree-sitter-pactole
   npx tree-sitter generate
   ```
2. Run tree-sitter test corpus:
   ```sh
   npx tree-sitter test
   ```
3. Update `queries/highlights.scm` if syntax tokens were added or modified.
4. Update `crates/pactole-storage-fs/src/parser.rs` and `crates/pactole-storage-fs/src/printer.rs` to support the new AST nodes.
5. Update `GRAMMAR.md`.

---

## 5. Coding Guidelines & Best Practices

- **Strict Architecture Boundaries**: Never import storage or parser code into `pactole-core`. `pactole-syntax` must never depend on `pactole-core` or perform I/O; it stays purely syntactic (CST, spans, diagnostics). `pactole-storage-fs`'s existing strict API and the canonical formatter remain unchanged and stay in `pactole-storage-fs`. `pactole-import` depends only on `pactole-core`, performs no I/O, and knows nothing of tree-sitter, `pactole-storage-fs`, the `.pactole` text format, or the config file format. `pactole-lsp` only consumes `pactole-storage-fs`/`pactole-syntax` through their existing public API and does not read `~/.config/pactole`.
- **Financial Precision**: Never use `f32` or `f64` for money amounts. Always use `rust_decimal::Decimal`.
- **Error Handling**:
  - Prefer structured errors via `thiserror` enums (`ModelError`, `ValidationError`, `PactoleFsStorageError`).
  - Do not use `unwrap()` or `expect()` in non-test library code; bubble up errors with `Result`.
- **Testing**:
  - For domain logic / validation, add unit tests in `pactole-core`.
  - For parsing / formatting, add unit tests in `pactole-storage-fs`.
  - For external-format import (OFX, ...), add integration tests in `pactole-import`
    (`crates/pactole-import/tests/`), with fixtures under `tests/fixtures/`.
  - For end-to-end command behavior, add integration tests in `crates/pactole-cli/tests/cli.rs`.
- **Minimal Diffs**: Keep changes focused and avoid reformatting unrelated code.
