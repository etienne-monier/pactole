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

Pactole is structured as a Cargo workspace, currently with eight crates. A future
database-backed storage crate is anticipated but not yet created; see "Planned Crates" below.

```text
pactole/
├── crates/
│   ├── tree-sitter-pactole/   # Tree-sitter grammar, C parser, and Rust bindings
│   ├── pactole-config/        # Layered pactole.toml schema and resolution (leaf crate)
│   ├── pactole-core/          # Pure domain models, validation, and reports
│   ├── pactole-syntax/        # Tolerant CST analysis, spans, and syntax diagnostics
│   ├── pactole-storage-fs/    # Tree-sitter-based parser, includes, and canonical formatter
│   ├── pactole-import/        # Import external formats (OFX) into domain models
│   ├── pactole-lsp/           # Minimal stdio Language Server Protocol server
│   └── pactole-cli/           # Command-line binary (`pactole`) and CLI tests
├── docs/                      # Astro/Starlight documentation site
├── editors/                   # Editor integrations (Zed extension, outside the workspace)
├── GRAMMAR.md                 # Specification of the .pactole syntax
├── HELIX.md                   # Integration guide for the Helix editor
├── README.md                  # User-facing installation and usage guide
├── CHANGELOG.md               # Delivered history
├── ROADMAP.md                 # Planned work (single source of truth for it)
└── Taskfile.yml               # Task runner definitions (e.g., documentation)
```

### Crate Responsibilities & Boundaries

1. **`pactole-core`**:
   - **Zero I/O and Zero Tree-sitter**: Contains only domain entities, business validation, and financial calculations. Must never depend on `pactole-storage-fs`, `pactole-syntax`, `tree-sitter`, or filesystem operations.
   - **Key modules**:
     - `models/`: Domain structs (`Journal`, `Entry`, `Transaction`, `Posting`, `Amount`, `Balance`, `Open`, `Close`, `Commodity`, `Payee`, etc.), the `TransactionStatus` enum (`Cleared`/`Pending`/`Uncleared`) and newtypes (`AccountName`, `CommodityName`, `MetadataKey`).
     - `validation.rs`: Invariant checks on `Journal` (chronological ordering, account lifecycles, declared commodities, declared payees, transaction balancing), plus `unused_declarations(&Journal)`, reporting `open`/`commodity`/`payee` declarations that nothing in the journal ever uses (severity decided by front-ends via `[validation] unused`). The entry points are `validate_journal` (returns the first error) and `validate_journal_all` (returns a `ValidationReport` holding the normalized `Journal` and every `ValidationIssue`, i.e. the error and the index of the offending entry in the input journal).
     - `validation.rs` (partial analysis): `MissingData` describes what a journal may be missing when it was assembled from a project that could not be analyzed completely. It has two axes — the *declaration* axis (`accounts`, `commodities`, `payees`: the names whose declaration may be missing) and the *usage* axis (`used_accounts`, `used_commodities`, `used_payees`: the names the missing content may have used) — plus an `everything` flag for an unknown amount of lost content. `validate_journal_all_with(&Journal, &MissingData)` and `unused_declarations_with(&Journal, &MissingData)` validate a possibly incomplete journal: the checks whose outcome depends on the missing content are suppressed, the others are still reported. `validate_journal_all` and `unused_declarations` are the complete-journal entry points (they use the default `MissingData`).
     - `register.rs`: Register report computation (running balance per commodity, filtering by account, date range, payee, narration, status, tag): `RegisterFilter` (the criteria), `RegisterEntry` (one matched posting with its running balance) and the listing function.
     - `report/`: Render-agnostic reporting API. `Period` is an inclusive date range (`Period::new`, `Period::single`, `Period::contains`), `ReferenceDate` is an inclusive reporting cut-off day, and `ReportScope` combines them (`AsOf` for stock reports, `Over` for flow reports). `Amounts` holds at most one value per commodity, so a multi-commodity total is a set of per-commodity amounts and never a cross-commodity sum. `AccountBalances` accumulates signed balances per account and `AccountGrouping` (`Exact`, `Depth`, `Full`) turns them into `ReportLine`s; `Full` produces cumulative prefix lines, so `AccountBalances::totals` is computed from the exact balances rather than from the displayed lines. `AccountBalances::from_journal` and `iter()` build reports directly from a validated [`Journal`](models::Journal). A report may be made of several [`ReportSection`]s (the balance sheet uses *Assets*, *Liabilities* and *Equity*) with per-section subtotals, or a single anonymous section for flat reports; the balance sheet flips liabilities and equity to positive, adds the net income/expense result to equity as `Unclosed result` when non-zero, and places accounts whose root is unclassified in an `Unclassified` section. [`AccountClassification`](balance_sheet::AccountClassification) maps account roots to categories and is overridable per journal (defaults to `Assets`/`Liabilities`/`Equity`/`Income`/`Expenses`). `Report` and `ReportLine` are plain result data (no rendering logic, no serialization derives); `Report::render` produces the text, JSON or CSV rendering selected by `ReportFormat`, with decimal amounts serialized as strings in JSON, and `ReportError::InvalidPeriod` reports an inverted period. [`balance_sheet`](balance_sheet) builds a [`Report`](Report) from a journal; [`income_statement`](income_statement) builds a [`Report`](Report) over a [`Period`] with `ReportScope::Over`, grouping by account, payee or tag (`IncomeGrouping`) and reporting income/expenses as positive amounts with a net result per commodity. [`expense_breakdown`](expenses) builds expense breakdowns grouped by account, payee, tag or month (`ExpenseGrouping`), with optional uncategorized transactions and inactive accounts (`ExpenseOptions`); [`expense_breakdown_with_options`](expenses) provides the configurable variant. [`ComparisonReport`](comparison)/[`ComparisonLine`](comparison) compares two periods with configurable difference direction (`DiffDirection`, `--reverse-diff`) and mode (`DiffMode`); `previous_period` computes a period of the same length, `compare_reports` builds the comparison. The CLI only prints the rendered string. The balance sheet (1.2), income statement (1.3) and spending breakdowns (1.4) build on these types.
     - `traits.rs`: Storage abstractions like `ReadableStorage`.
     - `diagnostics.rs`: `DiagnosticCode`, the stable catalogue of every Pactole diagnostic code. The enum spans the syntax, lowering, include, analysis-completeness and business families, but `pactole-core` only *produces* the business ones (see `ValidationError::code`); `pactole-storage-fs` maps the others onto the remaining variants. `DiagnosticCode::as_str` is the single definition of the wire text printed by the CLI and published by the LSP, `DiagnosticCode::parse` reads a code back (the LSP does this for the codes an editor sends it), and `DiagnosticCode::ALL` is the exhaustive list the documentation is checked against.
     - `errors.rs`: `ModelError`, `ValidationError`, and `ValidationError::code()`, which returns the `DiagnosticCode` of each business variant.
   - **Arithmetic**: Always uses `rust_decimal::Decimal` (never floating-point numbers).

2. **`tree-sitter-pactole`**:
   - Syntax definition in `grammar.js`.
   - Generated parser files (`src/parser.c`, `src/grammar.json`, `src/node-types.json`).
   - Rust binding via `bindings/rust/lib.rs` and `bindings/rust/build.rs`; its public API is the
     `LANGUAGE` constant (the `LanguageFn` to register in a `tree_sitter::Parser`), the
     `NODE_TYPES` constant (the contents of `src/node-types.json`), and the `HIGHLIGHTS_QUERY`,
     `INJECTIONS_QUERY`, `LOCALS_QUERY` and `TAGS_QUERY` constants (the editor queries, behind
     their `with_*_query` cargo features).
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
       (walks a tree-sitter `Tree` and collects `ERROR`/`MISSING` nodes as `SyntaxDiagnostic`s),
       with `collect_error_nodes_with_source` quoting the unexpected text when the source is
       available.
   - **Scope for now**: purely syntactic. It does **not** convert CST nodes into `pactole-core`
     domain models — that conversion remains the responsibility of `pactole-storage-fs`'s
     crate-private `parser` module (`src/parser.rs`).
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
   - `printer.rs`: Formats AST/source code into canonical `.pactole` layout: `format` (default
     options), `format_with_options` (explicit `[format]` options) and
     `format_transactions(&[Transaction]) -> String` /
     `format_transactions_with_options(&[Transaction], &FormatOptions)`, which serialize a list
     of domain transactions into canonical `.pactole` text (the second one is what
     `pactole import-ofx` uses, so that the resolved `[format]` options are honoured; the inverse
     of parsing for that subset of directives).
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
    - **`analysis.rs`** (public API: `analyze_file`, `ParsedFile`, `ParsedEntry`, `EntrySpans`,
      `PostingSpans`, `ParsedInclude`, `LoweringDiagnostic`, `Diagnostic`, `DiagnosticSeverity`,
      `DiagnosticSource`, `PartialAnalysis`, `FileLoss`): a positioned, directive-by-directive analysis for tooling (editors, a
     future `pactole-lsp`) that needs spans and per-directive diagnostics instead of an
     all-or-nothing `Journal`. Every `Diagnostic` carries a typed `code: DiagnosticCode`
     (re-exported by this crate from `pactole-core`); syntax, lowering, include, analysis and business
     issues are all mapped onto that single catalogue, and only the CLI and LSP turn a code
     into text. `analyze_file` never fails, even on syntactically invalid or
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
       - `analyze_file_with_loader_with_options(entry_path, loader, &ValidationOptions)` is the
         configurable variant: it gives the configurable business checks — the undeclared payee
         check (`PayeeNotDeclared`, `[validation] payee`) and the unused-declaration checks
         (`AccountUnused`/`CommodityUnused`/`PayeeUnused`, `[validation] unused`) — the severity
         configured for them, while every other business issue stays an error.
         `validated_journal()` then succeeds as long as no error-severity diagnostic remains;
         warnings alone do not block it. The `analyze_file_with_loader` form uses
         `ValidationOptions::default()` (undeclared payees errors, unused declarations warnings),
         so existing callers behave exactly as before.
       - Configuration types (`ValidationOptions`, `ValidationLevel`, `FormatOptions`) come from
         `pactole-config`; `pactole-storage-fs` re-exports `FormatOptions` to keep its formatting
         API self-contained.
       - Include diagnostics are exposed through `AnalyzedProject::diagnostics()` and
         `structural_diagnostics()` as positioned `Diagnostic` values; the internal include issue
         representation is not part of the public API.
       - **Partial analysis** (public API: `PartialAnalysis`, `FileLoss`, and the
         `AnalyzedProject::is_partial()`/`AnalyzedProject::partial()` accessors): business
         validation survives a broken file. When part of the project cannot be turned into a
         domain entry, the checks that do not depend on the missing content are still reported and
         the ones that do are skipped.
         - `uncertainty.rs` (crate-private) recovers, from the CST of the lost directives, the
           names the missing content may have *declared* (an account that may have been opened, a
           commodity or a payee that may have been declared) or *used* (the accounts, commodities
           and payees of a lost transaction or balance assertion). Tree-sitter's error recovery
           is directive-local, so the tree of a broken directive still holds the `account`,
           `commodity_name` and `string` nodes it recognized.
         - The per-file uncertainties are aggregated into a `pactole_core::MissingData`, which is
           handed to `pactole_core::validate_journal_all_with` and
           `pactole_core::unused_declarations_with`. An include that drops its content sets the
           `everything` flag: every completeness-dependent check becomes uncertain.
         - `PartialAnalysis` holds that aggregate `MissingData` and the per-file breakdown
           (`FileLoss { path, lost_directives, missing }`). A partial file also gets an
           `analysis-incomplete` warning (`DiagnosticCode::AnalysisIncomplete`, a new
           `DiagnosticSource::Analysis` origin).
       - Kept intentionally minimal: no caching, no incremental re-analysis, and no attempt to
         track which loaded files a given analysis depends on for invalidation — a future
         `pactole-lsp` is expected to build that on top of this contract rather than this crate
         growing it prematurely.

6. **`pactole-cli`**:
   - Command-line application binary named `pactole` (using `clap` derive).
   - **Root flags** (before the subcommand):
      - `--config <path>`: use this `pactole.toml` and skip global/project layer discovery.
   - Subcommands:
      - `pactole parse <file>`: Parse file and debug-print journal entries.
      - `pactole fmt <file> [--write]`: Format `.pactole` file canonically (supports `-` for stdin),
        using the resolved `[format]` options.
      - `pactole completions <shell>`: Generate shell completion scripts.
       - `pactole check <file> [--strict] [--format text|json]`: Syntax parsing + business validation,
         using the resolved `[validation]` options (`--strict` turns warnings into a failure).
       - `pactole graph <file> [--scan <dir>] [--format text|json]`: Display the include graph,
         cycles, multiply included files, and unreachable `.pactole` files under the scan directory
         (the root journal's directory by default).
       - `pactole register <file> [filters] [--format text|json]`: Register report with running balances.
       - `pactole balance <file> [--date <date>] [--format <text|json|csv>] [--depth <n>]`:
         Build the balance sheet with the `[reports]` classification (Assets/Liabilities/Equity,
         income/expenses closed into equity as `Unclosed result`, unclassified roots in an
         `Unclassified` section).
       - `pactole income <file> [period and grouping options]`: Build the income statement grouped
         by account, payee or tag.
       - `pactole expenses <file> [period, grouping and comparison options]`: Build the spending
         breakdown grouped by account, payee, tag or month; optionally list uncategorized
         transactions and inactive expense accounts, or compare equal-length periods using
         `periods_have_same_length`. The
         `[reports] uncategorized` account list falls back to `[import] default_counter_account`.
      - `pactole init [ledger] [options]`: Initialize a starter ledger and, next to it (or in the
        user configuration directory with `--global`), a `pactole.toml`. Options: `--account`
        (default `Assets:Checking`), `--currency` (default `EUR`), `--counter-account` (default
        `Expenses:ToReview`), `--date`, `--stdout`, `--global`, `--no-config`, `--no-ledger`.
        `--no-config` and `--no-ledger` are mutually exclusive: at least one file must be created.
     - `pactole import-ofx <file> [--into <path>] [--account <account>] [--write]`:
       Import an OFX/QFX
       statement (SGML 1.x or XML 2.x) into canonical `.pactole` transactions, via
       `pactole_import::decode_ofx_source` (source bytes → UTF-8) +
       `pactole_import::import_ofx` + `pactole_storage_fs::format_transactions_with_options`.
       Configuration
       (bank account, optional `[import.accounts]` ACCTID → Pactole account mapping, default
       counter account, optional fallback currency, `[[import.rules]]`) is read from the
       resolved `pactole.toml`; the command fails with a clear message when no `[import]`
       section is configured anywhere.
       `--account` overrides the configured bank account and mapping for that run. Without
       `--into`, transactions are printed to stdout; with `--into <file>`, FITIDs already present
       in that file (and in the files it `include`s) are skipped (idempotence), and with `--write`
       the generated transactions are appended to it.
   - Configuration is resolved by `pactole-config`: global `pactole.toml`, then the project
     `pactole.toml` next to the journal (or `--into` for `import-ofx`), then the root flags.
   - Integration tests in `crates/pactole-cli/tests/cli.rs`.

5. **`pactole-lsp`**:
   - A minimal stdio Language Server Protocol server for `.pactole` files, built on `lsp-server` +
     `lsp-types`. Binary name: `pactole-lsp`.
   - **Command line**: `clap` derive, like `pactole`. `--version` and `--help` are handled by
     clap; `--stdio` is accepted and ignored (stdio is the only transport) so that editor
     configurations passing `args = ["--stdio"]` keep working; any other argument is rejected.
     The `initialize` result carries the server capabilities plus `serverInfo` (name and version),
     so clients can display them.
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
     - `config.rs`: `Config`, resolved once at `initialize` time from `initializationOptions`
       and from the layered `pactole.toml` files. Supports `journal_file` (path to the root
       `.pactole` file, resolved against the workspace root — from `workspace_folders`, then
       `rootUri`, then `rootPath` — when given as relative; when no workspace root is known at
       all, resolved against the server process's current working directory instead of being
       silently left relative, so it does not later fail to resolve and silently drop every
       diagnostic for the whole include graph) and `validation` (an
       `"off"`/`"warn"`/`"error"` string applied to every configurable check, or a
       `{ "payee": ..., "unused": ... }` table). Validation
       settings follow the same layering as the CLI: the
       global `pactole.toml` (via `pactole_config::config_dir`), then the project `pactole.toml`
       (next to `journal_file`, or at the workspace root when none is configured), then
       `initializationOptions`. Unreadable or invalid configuration files, and invalid
       validation levels, are logged and ignored rather than failing `initialize`.
       Also supports the `[declaration]` section (`payees`/`accounts`/`commodities`), whose
       relative paths are resolved against the journal's directory (like an `include`), and
       exposes them as `DeclarationTargets` for the declaration quick fixes.
     - `server.rs`: the stdio `Connection`, main loop, and notification handlers. Publishes
       `textDocument/publishDiagnostics`:
       - When `journal_file` is configured and loads successfully, diagnostics are computed for
         *every file reachable via `include`* using
         `pactole_storage_fs::analyze_file_with_loader_with_options` with
         `DocumentsSourceLoader` (open buffers first, filesystem fallback), and published
         per-file (keyed by each file's own path/URI).
       - Otherwise (no `journal_file`, or it fails to load), the current document alone is
         analyzed standalone via `pactole_storage_fs::analyze_file` (its `include`s are *not*
         followed in this fallback path).
       - The main loop tracks the set of URIs published with diagnostics by the previous
         `publish_for` call; any URI no longer covered by the latest one (e.g. an `include` was
         removed, dropping a file out of the project) is published an empty diagnostics list, so
         diagnostics never go stale once a file drops out of the analyzed set.
     - `navigation.rs`: goto definition (`textDocument/definition`) and find references
       (`textDocument/references`) for accounts, payees, commodities and `include` paths
       (jumping to the included file). Both resolve against one shared occurrence index built
       directly from each file's CST, obtained through `pactole-syntax`
       (`ParsedFile::document().root_node()`), using the `tree-sitter` crate's `Node` type
       directly (hence the crate's direct `tree-sitter` dependency, but no
       `tree-sitter-pactole` one), tagging each token as a declaration or a use. The scope is the
       `journal_file` include graph analyzed with `DocumentsSourceLoader` (unsaved buffers
       first), plus the current document when it is not part of it; with no `journal_file`, the
       current document is its own entry point so its own `include`s are followed too — the same
       scope `hover.rs` and `completion.rs` use via the shared `documents::analyze_scope` helper.
       A missing declaration is reported as an empty result, never an error.
     - `rename.rs`: rename support (`textDocument/prepareRename` and `textDocument/rename`) for
       the two renamable symbol kinds, accounts and payees, reusing `navigation.rs`'s occurrence
       index, scope and symbol classification (shared as `pub(crate)` items — rename does not
       duplicate the CST scan). An account token is replaced as written; a payee edit replaces
       only the content of its `string` token, preserving the surrounding quotes
       (`prepareRename` offers that inner range too, so the editor prefills the input without
       quotes). The new name is validated (accounts reuse `pactole_core::AccountName`; payees
       must be non-empty and contain no double quote or newline), and a rename that would
       create a duplicate `open` or `payee` declaration in the scope is refused with a
       `RenameError`, which `server.rs` maps to an `InvalidParams` response. So are a payee
       occurrence whose `string` token was left unterminated by error recovery (its inner range
       is undefined; `prepareRename` reports it as not renameable) and a rename when a
       configured `journal_file` cannot be analyzed (the edit would cover the current file
       only); a no-op rename, a
       non-renamable symbol or a position without a symbol yields an empty result.
    - Completion, code actions, hover, navigation and rename are supported. Formatting over LSP (use
      `pactole fmt`), `textDocument/documentSymbol` and incremental sync remain out of scope for
      this first version. Unhandled requests are
      answered with a `MethodNotFound` error rather than silently ignored.
    - Declaration quick fixes (`code_action.rs`): beyond the "insert today's date" fix, the server
      turns the `payee-not-declared`, `account-not-open` and `commodity-not-declared` business
      diagnostics into quick fixes inserting the missing declaration above the offending
      transaction (`open`/`commodity` are dated with the transaction's date; a `payee` has no
      date). The declaration goes into the current document, or is appended to the file configured
      in the `[declaration]` section of `pactole.toml` — but only when that file is actually
      reachable from the root journal through `include`; otherwise the fix falls back to the
      current document. `[declaration]` paths are resolved against the journal's directory (like
      an `include`), and the fixes only exist when a `journal_file` is configured, since business
      diagnostics require a project context.
   - **Depends on `pactole-storage-fs`/`pactole-syntax` only through their existing public API**
     (`analyze_file`, `analyze_file_with_loader_with_options`,
     `normalize_lexically`, `FsSourceLoader`, `SourceLoader`, `SourceLoadError`,
     `ParsedFile`, `AnalyzedProject`, `Span`/`Point`; `analyze_file_with_loader` is used
     in the crate's tests only), plus `pactole-config` for layered configuration and
     `tree-sitter` (directly, for the navigation CST index); it does not add or change
     anything in those crates.
   - Has no dependency on `thiserror`: its only error type is the hand-written `RenameError`
     struct, and errors are otherwise propagated via `Box<dyn Error + Sync + Send>` in
     `server.rs`.

7. **`pactole-import`**:
   - **Edge adapter, zero I/O, depends only on `pactole-core` and `pactole-config`** (plus
     `chrono`, `encoding_rs`, `rust_decimal`, `thiserror` and `ofx-rs`): translates a foreign
     input format into pure domain models. It knows nothing about tree-sitter,
     `pactole-storage-fs`, the `.pactole` text
     format, or where its configuration comes from, so it is reusable by any front-end (the
     `pactole` CLI today, a future `pactole-storage-db`, the LSP tomorrow).
   - **Key modules**:
     - `record.rs`: `ImportRecord`, a format-neutral intermediate representation (date, signed
       amount, currency, payee, memo, transaction type, check number, reference, FITID, source
       account identifier).
     - `ofx.rs`: OFX parser (1.x SGML and 2.x XML, via the `ofx-rs` crate) producing
       `ImportRecord`s from `BANKMSGSRSV1`, `CREDITCARDMSGSRSV1` and `INVSTMTMSGSRSV1`
       (investment statements contribute their `INVBANKTRAN` banking transactions only). A
       document that plainly contains `<STMTTRN>` elements but yields no record fails with
       `ImportError::TransactionsDropped` instead of importing nothing silently.
     - `encoding.rs`: `decode_ofx_source(&[u8]) -> String`, tolerant decoding of raw OFX source
       bytes into UTF-8 (it never fails). It honours the declared encoding — the
       `<?xml … encoding="…"?>` declaration, the `ENCODING:`/`CHARSET:` lines of an OFX 1.x
       header (Windows code pages 1250–1258, 874, the CJK code pages 932/936/949/950, Mac
       Roman), or a byte order mark — and otherwise falls back to strict UTF-8 and then
       windows-1252, so mislabeled bank exports still decode instead of being rejected.
     - `ImportConfig`, `Rule`, `Direction` are **defined in `pactole-config`** (the `[import]`
       section of `pactole.toml`) and re-exported here for convenience.
     - `rules.rs`: shared rule engine lowering an `ImportRecord` into a balanced
       `pactole_core::Transaction` (first matching rule wins, otherwise the default counter
       account; a rule may also override the payee). The bank-side account is resolved per
       record from `ImportConfig::accounts` (keyed by the record's source account identifier),
       falling back to `ImportConfig::bank_account` with an `ImportWarning::UnmappedAccount`
       (emitted once per distinct unmapped identifier, only when a mapping table exists).
     - `error.rs`: `ImportError`.
    - **Public API**: `import_ofx(source, &config, &HashSet<String>) -> Result<ImportOutcome,
      ImportError>`, `migrate_actual_database(path, currency, opening_equity, opening_date,
      payee_metadata_key) -> Result<ActualMigration, ImportError>`, `parse_actual_database`,
      `decode_ofx_source(&[u8]) -> String`, plus `ActualMigration`, `ImportConfig`, `Rule`,
      `Direction`, `ImportRecord`, `ImportOutcome`, `ImportWarning`, `ImportError`. `import_ofx`
     skips any source transaction whose FITID is
     already in `known_fitids` (reported in `ImportOutcome::skipped`), and stores each produced
     transaction's FITID as `fitid` metadata so a later run can rebuild `known_fitids`.
   - **Deliberate two-step pipeline** (format parser → `ImportRecord`, then shared rule engine →
     `Transaction`) so future importers (CSV, QIF, ...) reuse the rule engine instead of
     duplicating it. Only OFX is implemented for now.
   - **Serialization stays out of this crate**: turning the produced `Transaction`s into
     `.pactole` text is `pactole_storage_fs::format_transactions_with_options`, called by the CLI.

8. **`pactole-config`**:
   - **Leaf crate**: depends on no other Pactole crate (only `serde`, `toml`, `thiserror`), and
     owns the whole `pactole.toml` schema plus its layering rules. Every other crate that needs
     configuration depends on it.
   - **Layers** (lowest to highest precedence): global
     `$XDG_CONFIG_HOME/pactole/pactole.toml`, else `%APPDATA%\pactole\` on Windows, else
     `~/.config/pactole/pactole.toml`; then the project `pactole.toml` (next to the journal); then
     in-memory overrides (CLI root flags, LSP `initializationOptions`). An explicit
     `--config <path>` replaces discovery with that single file. No parent-directory walk-up.
   - **Merge semantics**: per key. Tables merge per entry (`[import.accounts]` unions);
     `[[import.rules]]` is replaced in block by the highest layer defining it. Unknown keys are
     rejected per section (`deny_unknown_fields`), so typos fail loudly.
   - **Schema**:
     - `[validation] payee`: `"off"`, `"warn"` or `"error"` (default `"error"`, preserving
       historical behavior). It controls only the diagnostic for an undeclared payee; the payee
       remains mandatory in the grammar.
     - `[validation] unused`: `"off"`, `"warn"` or `"error"` (default `"warn"`). It controls the
       diagnostics for declarations nothing uses: an `open`ed account with no transaction nor
       balance assertion, a `commodity` never used by any amount (nor restricted to an account by
       an `open`), and a `payee` never used by a transaction (`account-unused`,
       `commodity-unused`, `payee-unused`).
     - `[format] posting_width`: amount alignment column (default 50).
     - `[declaration]` (`payees`, `accounts`, `commodities`): the files an
       editor front-end should insert new declarations into, instead of the
       current document. Paths are kept raw (leaf crate); `pactole-lsp`
       resolves them against the journal's directory.
      - `[reports]`: `assets`, `liabilities`, `equity`, `income`, `expenses` — account-root
        lists used by the reports; each list replaces in block (defaults to the English roots), an
        unknown root lands in an `Unclassified` balance-sheet section. `uncategorized` is a list of
        exact accounts (including subaccounts) used by `pactole expenses --uncategorized`.
     - `[import]`: `bank_account`, `currency`, `default_counter_account`, `[import.accounts]`,
       `[[import.rules]]` — the OFX import settings; `ImportConfig`/`Rule`/`Direction` are
       defined here and re-exported by `pactole-import`.
   - **Public API**: `Config` (resolved), `Config::resolve(&[Source], &Overrides)` (pure, no I/O),
     `Config::load(explicit, project, &Overrides)` (I/O), `ConfigError`, `Overrides`, `Source`,
     `ValidationLevel`/`ValidationOptions`, `FormatOptions`, `DeclarationOptions`,
     `ReportOptions`, `ImportConfig`/`Rule`/`Direction`, `config_dir()`, `default_config_path()`.

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

### Fixtures and property tests

Two example projects live at the root of the repository, under `fixtures/`.
They are read by the integration tests of `pactole-storage-fs`
(`tests/journal_fixtures.rs`), `pactole-cli` (`tests/journal.rs`) and
`pactole-lsp` (`tests/diagnostics.rs`). Those tests skip when the repository's
fixtures are not available (a published crate tested from a tarball), so
`cargo test` still passes outside a checkout.

- `fixtures/journal/` is a valid, canonical, multi-file ledger (EUR and USD,
  current account, savings, salary, rent, everyday expenses). It must stay
  completely clean: no diagnostic at any severity, and every file already in
  canonical form. Run `pactole check --strict` and `pactole fmt --write` on
  the files you change.
- `fixtures/journal-partial/` is the deliberately broken sibling used by the
  partial-analysis tests: its `broken.pactole` cannot be analyzed, and the
  tests pin which business diagnostics survive that loss and which are
  suppressed.
- `fixtures/README.md` describes both, and
  `docs/src/content/docs/tutorials/example-journal.md` walks users through the
  valid one. Keep them in sync when the fixtures change.

Property tests use `proptest` (a workspace dependency, enabled as a dev
dependency of `pactole-core`, `pactole-storage-fs` and `pactole-lsp`). They
cover the invariants that example-based tests cannot enumerate:

```sh
# Balancing rules, order-independence, agreement between validation entry points
cargo test -p pactole-core --test properties

# Formatter idempotence, diagnostic-free output, serializer round-trip
cargo test -p pactole-storage-fs --test properties

# Byte-offset <-> UTF-16 conversions on arbitrary text
cargo test -p pactole-lsp --test properties
```

A failure prints the minimal input that reproduces it and writes a
`*.proptest-regressions` file next to the test; commit that file together with
the fix so the case stays covered.

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

### Versioning & Publishing

- **The version lives in one place**: `[workspace.package] version` in the root
  `Cargo.toml`. Every crate inherits it with `version.workspace = true`, and
  every crate depends on the others through `[workspace.dependencies]`, which
  pairs the `path` (local builds always use the workspace sources, crates.io
  is never consulted for them) with the `version` (what the published crates
  require from the registry). Cargo checks that the two match, so a mismatch
  fails the build instead of silently drifting.
- **Bump with the task runner**, never by hand:

  ```sh
  task version VERSION=0.3.0
  ```

  It rewrites the package version and the internal dependency requirements,
  then refreshes `Cargo.lock`. Keep `Cargo.lock` committed and in sync: the CI
  runs with `--locked`.
- **The crates are published in lockstep** (`0.3.0`, `0.4.0`, ...). Because a
  `0.x` caret requirement (`^0.3.0` means `>=0.3.0, <0.4.0`) makes the minor
  version the compatibility boundary, bumping a crate to a new minor requires
  bumping every crate that depends on it.
- **Publication order** is handled by `cargo publish --workspace` (Cargo 1.90
  and later): `tree-sitter-pactole` → `pactole-core` → `pactole-config` →
  `pactole-syntax` → `pactole-storage-fs` → `pactole-import` → `pactole-cli` →
  `pactole-lsp`. The CI runs the same command as a dry run
  (`publish-check` job) to catch non-publishable manifests before the tag.
- **The minimum supported Rust version is `rust-version = "1.90"`** (required
  by `tree-sitter` 0.27), and the CI has an `msrv` job that builds and tests
  the workspace with that toolchain.

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
6. Update the documentation site: the directive tables of
   `docs/src/content/docs/reference/directives.md`, the directive list of
   `docs/src/content/docs/explanation/differences-from-beancount.md` and
   `docs/src/content/docs/reference/grammar.md`. This is enforced by
   `crates/pactole-storage-fs/tests/workspace_docs_sync.rs`, which fails when
   a directive of the grammar is missing from the reference.

### Documentation Synchronization

Two integration tests keep the prose in sync with the code. They are skipped
when the repository's markdown files are not available (a published crate
tested from a tarball), so `cargo test` still passes outside a checkout.

```sh
# The CLI surface (sub-commands and options) vs the reference and the README
cargo test -p pactole-cli --test docs_sync

# The directives, the diagnostic codes and the public API vs the documentation
cargo test -p pactole-storage-fs --test workspace_docs_sync
```

Run them whenever you add a sub-command, an option, a directive, a diagnostic
code or a public item, and update the documentation in the same change: the
tests fail otherwise.

---

## 5. Coding Guidelines & Best Practices

- **Strict Architecture Boundaries**: Never import storage or parser code into `pactole-core`. `pactole-syntax` must never depend on `pactole-core` or perform I/O; it stays purely syntactic (CST, spans, diagnostics). `pactole-storage-fs`'s existing strict API and the canonical formatter remain unchanged and stay in `pactole-storage-fs`. `pactole-import` depends only on `pactole-core` and `pactole-config`, performs no I/O, and knows nothing of tree-sitter, `pactole-storage-fs`, the `.pactole` text format, or the config file format. `pactole-config` is a leaf crate: it depends on no other Pactole crate and owns the whole `pactole.toml` schema and layering rules. `pactole-lsp` consumes `pactole-storage-fs`/`pactole-syntax` through their existing public API and uses `pactole-config` for layered configuration.
- **Financial Precision**: Never use `f32` or `f64` for money amounts. Always use `rust_decimal::Decimal`.
- **Error Handling**:
  - Prefer structured errors via `thiserror` enums (`ModelError`, `ValidationError`, `PactoleFsStorageError`).
  - Do not use `unwrap()` or `expect()` in non-test library code; bubble up errors with `Result`.
- **Testing**:
  - For domain logic / validation, add unit tests in `pactole-core`.
  - For parsing / formatting, add unit tests in `pactole-storage-fs`.
  - For external-format import (OFX, ...), add integration tests in
    `pactole-import` (`crates/pactole-import/tests/`), with fixtures under
    `tests/fixtures/`.
  - For end-to-end command behavior, add integration tests in
    `crates/pactole-cli/tests/cli.rs`, and use the root `fixtures/` journals
    (see §4) for realistic multi-file scenarios.
  - For rules that must hold on every input (balancing, formatting,
    byte/UTF-16 conversion), add a `proptest` property to the crate's
    `tests/properties.rs` instead of enumerating examples.
- **Minimal Diffs**: Keep changes focused and avoid reformatting unrelated code.
