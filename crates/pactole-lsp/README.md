# pactole-lsp

Minimal stdio [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
server for `.pactole` files.

## Role

Binary `pactole-lsp`, built on `lsp-server` + `lsp-types`. Handles the
LSP lifecycle (`initialize`/`shutdown`/`exit`) and document sync for
`textDocument/didOpen`/`didChange`/`didClose` using **full** sync
(`TextDocumentSyncKind::FULL`): each `didChange` carries the entire
document text, so no LSP position → byte offset conversion is needed to
apply an edit. A deliberate simplicity/robustness choice over incremental
sync, acceptable for typically small, hand-edited `.pactole` files.

It only consumes `pactole-storage-fs`/`pactole-syntax` through their
existing public API (`analyze_file`, `analyze_file_with_loader`,
`analyze_file_with_loader_with_options`, `FsSourceLoader`, `SourceLoader`,
`SourceLoadError`, `ParsedFile`, `AnalyzedProject`, `IncludeIssue`,
`SyntaxDiagnostic`, `Span`/`Point`) and uses `pactole-config` for layered
configuration: it neither adds nor modifies anything in these crates. It
also uses `tree-sitter` directly (same version as `pactole-syntax`) to
locate the CST token under the cursor for navigation.

## Command line

The binary takes no required argument:

```text
pactole-lsp            # serve over stdio (the only transport)
pactole-lsp --version  # print the server name and version
pactole-lsp --help     # print the usage
```

`--stdio` is accepted and ignored: it is the only supported transport, but
editors are often configured to launch a language server with it
(`args = ["--stdio"]`), and such a configuration must keep working. Any other
argument is rejected.

The server also announces its name and version in the `initialize` response
(`serverInfo`), so that clients can display them:

```json
{ "serverInfo": { "name": "pactole-lsp", "version": "0.3.0" } }
```

## Current features

Diagnostics (`textDocument/publishDiagnostics`), completion
(`textDocument/completion`), hover information, navigation
(`textDocument/definition`, `textDocument/references`) and rename
(`textDocument/prepareRename`, `textDocument/rename`), via:

- `documents.rs`: `Documents` (in-memory table of open URI → current
  text), `DocumentsSourceLoader`, a
  `pactole_storage_fs::SourceLoader` that serves the buffer of an open
  (potentially unsaved) document when one exists for a given path, with
  a fallback to `FsSourceLoader` otherwise, plus the request-scope helpers
  shared by hover, completion and navigation: `uri_to_path` (the `file:`
  path behind a document URI) and `analyze_scope` (the configured journal's
  include graph, or the current document as its own entry point when none
  is configured).
- `conversion.rs`: `span_to_range`/`point_to_position`, converting
  `pactole-syntax`'s byte-based `Span`/`Point` into LSP's UTF-16
  `Range`/`Position`, by re-scanning the relevant source line.
- `diagnostics.rs`: `standalone_diagnostics` (syntax + lowering
  diagnostics for a `ParsedFile`) and `project_diagnostics` (per-file
  diagnostics, plus `IncludeIssue`s attached to the file containing the
  offending `include`, for an `AnalyzedProject`).
- `config.rs`: `Config`, resolved once at `initialize` from the
  `initializationOptions` and the layered `pactole.toml` files. Supports
  `journal_file` (path of the root `.pactole` file, resolved against the
  workspace root, or against the server's current working directory if no
  root is known), `validation` (a `"warn"`/`"error"` string applied to every
  configurable check, or a `{ "payee": ..., "unused": ... }` table),
  `format` (`posting_width`) and the `[declaration]` section of
  `pactole.toml` (see below). Validation settings follow the CLI layering:
  user-level `pactole.toml`, then project `pactole.toml` (next to
  `journal_file`, or at the workspace root when none is configured), then
  `initializationOptions`. Broken configuration files, invalid levels and
  invalid formatter overrides are logged and ignored rather than failing
  `initialize`.
- `server.rs`: the stdio `Connection`, the main loop, and the
  notification handlers. The `initialize` result carries the server
  capabilities and the server's name and version (`serverInfo`). When
  `journal_file` is configured and loads,
  diagnostics are computed for every file reachable via `include` (via
  `analyze_file_with_loader`) and published per file; otherwise the
  current document alone is analyzed standalone via `analyze_file` (its
  `include`s are then not followed). URIs that leave the analyzed set
  (e.g. a removed `include`) receive an empty diagnostics list so they
  never go stale.
- `completion.rs`: completion for known accounts, payees, commodities, tags
  and links, plus a date-of-the-day item at the beginning of a directive. The
  date item replaces a partially typed `YYYY-` prefix.
- `code_action.rs`: a quick fix that inserts today's date at the beginning of
  an undated `open`, `close`, `commodity`, `balance` or transaction directive,
  plus declaration quick fixes for the `payee-not-declared`,
  `account-not-open` and `commodity-not-declared` business diagnostics: the
  missing declaration is inserted above the offending transaction, either in
  the current document or in the file configured for that category (see
  `[declaration]` below). `open` and `commodity` declarations are dated with
  the offending transaction's date; a `payee` has no date.
- `navigation.rs`: goto definition and find references for accounts, payees
  and commodities, plus goto definition on an `include` path (jumps to the
  included file). Both requests share one occurrence index built directly
  from each file's CST, searched across one shared scope: the whole include
  graph of `journal_file` (unsaved open buffers taking precedence over what
  is on disk), plus the current document when it is not part of that
  project — and, when no `journal_file` is configured, the current document
  as its own entry point, so its own `include`s are followed too (the same
  scope hover and completion use via `documents::analyze_scope`). A
  position with no symbol, or a symbol with no declaration, returns an
  empty result rather than an error.
- `rename.rs`: rename for accounts and payees (`textDocument/prepareRename`
  and `textDocument/rename`), reusing `navigation.rs`'s occurrence index,
  scope and symbol classification. An account token is replaced as written;
  a payee edit replaces only the content of its `string` token, so the
  surrounding quotes are preserved (`prepareRename` offers that inner range
  too, so the editor prefills the input without quotes). The new name is
  validated (accounts reuse `pactole_core::AccountName`; payees must be
  non-empty and contain no double quote or newline), and a rename that would
  create a duplicate `open` or `payee` declaration in the scope is refused
  with an error before anything is applied. So is a payee occurrence whose
  `string` token was left unterminated by error recovery (its inner range is
  undefined; `prepareRename` reports it as not renameable), and so is a
  rename when a configured `journal_file` cannot be analyzed, since the edit
  would silently cover the current file only. A no-op rename, a non-renamable
  symbol (a commodity, an `include` path, a date, a narration, a tag) or a
  position without a symbol returns an empty result rather than an error.

## Explicitly out of scope

Document symbols (`textDocument/documentSymbol`), range/on-type formatting and
incremental sync. Full-document formatting is available through
`textDocument/formatting`. Any unhandled request receives a `MethodNotFound`
error rather than being silently ignored.
See the roadmap in [`../../ROADMAP.md`](../../ROADMAP.md)
for the planned status of these items.

## Client-side configuration

Pass `journal_file` in the `initializationOptions` of the LSP
`initialize` request, for example:

```json
{ "journal_file": "main.pactole" }
```

Formatter settings can be overridden by the LSP client. They take precedence
over both global and project `pactole.toml` files:

```json
{
  "journal_file": "main.pactole",
  "format": { "posting_width": 60 }
}
```

Validation levels can be overridden the same way, globally or per check:

```json
{ "journal_file": "main.pactole", "validation": "warn" }
```

```json
{ "journal_file": "main.pactole", "validation": { "payee": "warn" } }
```

```json
{ "journal_file": "main.pactole", "validation": { "unused": "off" } }
```

Without an override, the server reads the same layered `pactole.toml` as the
CLI (user-level file, then project file).

Checks at the `warn` level are published as LSP warnings: this is how
declarations nothing ever uses (`payee-unused`, `account-unused`,
`commodity-unused`, `[validation] unused`, a warning by default) show up in
the editor without being reported as errors.

### Declaration files

The `[declaration]` section of `pactole.toml` tells the server where a new
payee, account or commodity declaration should go. Each path is resolved
against the journal's directory, exactly like an `include` written in the root
journal:

```toml
[declaration]
payees = "payees.pactole"
accounts = "accounts.pactole"
commodities = "commodities.pactole"
```

With this configured, a `payee-not-declared`, `account-not-open` or
`commodity-not-declared` quick fix appends the declaration to the matching
file instead of the document being edited.

The target file is only used when it is actually reachable from the root
journal through `include`; when it is not (or when no `journal_file` is
configured), the quick fix falls back to the current document. The server
never creates the declaration file nor adds the `include` for you.

## Dependencies / boundaries

- `chrono` (the local date), `clap` (the command line), `lsp-server`,
  `lsp-types` (LSP protocol), `serde`/`serde_json` (JSON
  deserialization), `pactole-storage-fs`, `pactole-syntax`, `pactole-config`.
- `log` + `env_logger` for debug traces (see below).
- No `thiserror` dependency: the only error type is the hand-written
  `RenameError` struct (mapped to an `InvalidParams` response); errors are
  otherwise propagated via `Box<dyn Error + Sync + Send>` in `server.rs`.

## Debug traces

The server logs via [`log`](https://docs.rs/log) +
[`env_logger`](https://docs.rs/env_logger), **always on `stderr`** (never
on `stdout`, which serves the LSP protocol itself): a stray write on
`stdout` would corrupt the message stream. Without `RUST_LOG`, no trace
is emitted.

Enable traces by setting `RUST_LOG` before launching `pactole-lsp`, for
example:

```sh
RUST_LOG=pactole_lsp=debug pactole-lsp
```

Levels used:

- `info`: server lifecycle (initialization with the workspace root and
  the resolved `journal_file`, shutdown).
- `debug`: `didOpen`/`didChange`/`didClose` notifications (URI and text
  size, never its content), the result of each analysis (number of files
  and diagnostics published in multi-file mode, number of diagnostics in
  standalone mode).
- `warn`: fallback from multi-file analysis to standalone analysis of
  the current document (`journal_file` configured but unreadable), an
  unreadable or invalid `pactole.toml` (or an invalid `validation`
  level) whose settings are then ignored, and clearing of all published
  diagnostics when no analysis is available.
- `trace`: origin of each file loaded during `include` resolution (open
  in-memory buffer or filesystem), via `DocumentsSourceLoader`.

Document content is never logged, only metadata (URI, lengths, paths).

## Tests / validation

```sh
cargo test -p pactole-lsp
cargo clippy -p pactole-lsp --all-targets
```

Unit tests cover notably `navigation.rs` and `rename.rs` (symbol detection,
scope resolution, rename edits and validation), `config.rs` (`journal_file`
resolution and layered validation settings) and `server.rs` (the `initialize`
result, and computing the URIs to clear between two diagnostics
publications).

`tests/cli.rs` drives the built binary: `--version`, the acceptance of
`--stdio`, and a real `initialize` round-trip over the stdio transport
checking the announced `serverInfo`.

## See also

- [`../../AGENTS.md`](../../AGENTS.md) — full
  description of the current LSP and its limits.
- [`../../ROADMAP.md`](../../ROADMAP.md) — the planned status of the
  document symbols and incremental sync.
- [`../../README.md`](../../README.md#configuration) — the shared
  `pactole.toml` format and layering rules.
