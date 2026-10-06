# pactole-storage-fs

File-based `.pactole` storage: strict parser, canonical formatter, and
tolerant multi-file analysis for tooling.

## Role

`pactole-storage-fs` is the only junction between the syntactic world
(tree-sitter / `pactole-syntax`) and the domain world (`pactole-core`).
It exposes two distinct flows (see
[`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) §3 for details):

- a **strict flow**, historical and unchanged, all-or-nothing: file read
  → parsing → a single flattened `pactole_core::Journal` (included files
  merged in), used by `pactole-cli`;
- a **tolerant/positioned flow**, additive, which never fails the whole
  analysis and preserves per-file provenance, used by `pactole-lsp`.

## Main API

Re-exported from `src/lib.rs`:

- **Strict flow**:
  - `PactoleFileStorage`: implements `pactole_core::ReadableStorage` for
    a local file. `TryFrom<PathBuf>` reads the file; `parse()` converts
    it into a `Journal` (recursively resolving `include`s); `journal()`
    returns the resulting `Journal`.
  - `format(source: &str) -> Result<String, PactoleFsStorageError>`
    (`printer.rs`): reformats `.pactole` text into its canonical layout.
  - `PactoleFsStorageError` (`errors.rs`): structured errors
    (`thiserror`).
- **Tolerant/positioned flow** (`analysis.rs`):
  - `analyze_file(source: &str) -> ParsedFile`: analyzes a single file
    without resolving its `include`s; each directive is lowered
    independently, producing either a `ParsedEntry { span, entry }` or a
    `LoweringDiagnostic { span, message }` without ever abandoning the
    whole file; `ParsedInclude { span, path }` detects `include`s (raw,
    unresolved path, neither read nor parsed here);
    `ParsedFile::syntax_diagnostics()` exposes the underlying
    `pactole-syntax` `SyntaxDiagnostic`s.
  - `analyze_file_with_loader(entry_path, loader: &dyn SourceLoader) -> Result<AnalyzedProject, ...>`
    — follows `include`s recursively through a `SourceLoader`, producing
    an `AnalyzedProject` (each file keeping its own
    `AnalyzedFile { path, file }`) and a list of `IncludeIssue`s for
    inclusion problems (unresolvable path, cycle, unloadable target) —
    never a global failure, unless `entry_path` itself cannot be loaded.
- **Source loading** (`loader.rs`):
  - `SourceLoader`: minimal trait `fn load(&self, path: &Path) -> Result<String, SourceLoadError>`.
  - `FsSourceLoader`: reads real files from the filesystem.
  - `InMemorySourceLoader`: serves content from an in-memory table
    (useful for tests and a preview of a future loader backed by editor
    buffers).
  - `SourceLoadError`: loading error.

## Dependencies / boundaries

- `pactole-core` (domain models), `pactole-syntax` (tolerant parsing,
  spans, diagnostics), `tree-sitter` (raw CST nodes for AST → model
  conversion and formatting), `chrono`, `rust_decimal`, `thiserror`.
  `toml` appears in `Cargo.toml` as a declared dependency but is
  currently used by no module of this crate; it is reserved for future
  structured metadata value support.
- `parser.rs` and `printer.rs` both go through
  `pactole_syntax::parse_document`/`ParsedDocument::into_result` for
  parser initialization and `ERROR`/`MISSING` detection, so this
  plumbing is not duplicated; they each keep their own tree-sitter node
  traversal logic (domain model conversion and canonical formatting
  respectively).
- The historical strict API (`PactoleFileStorage`, `parser.rs`,
  `printer.rs`) remains unchanged by the introduction of
  `pactole-syntax` and `analysis.rs`/`loader.rs`: the latter are
  strictly additive.
- No `Span`/tree-sitter type leaks into `pactole-core`:
  `ParsedEntry`/`ParsedInclude`/`LoweringDiagnostic` live here and only
  wrap a `pactole_syntax::Span` around existing `pactole-core` types.

## Tests / validation

```sh
cargo test -p pactole-storage-fs
cargo clippy -p pactole-storage-fs --all-targets
```

Add parsing/formatting unit tests in this crate (see `parser.rs`,
`printer.rs`, `analysis.rs`, `loader.rs`).

## See also

- [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) — strict vs tolerant
  flow, multi-file resolution, roadmap (notably the positioned business
  diagnostics, still missing from this crate).
- [`../../GRAMMAR.md`](../../GRAMMAR.md) — the `.pactole` language
  specification that `parser.rs`/`printer.rs`/`analysis.rs` interpret.
