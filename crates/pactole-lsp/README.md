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
`FsSourceLoader`, `SourceLoader`, `SourceLoadError`, `ParsedFile`,
`AnalyzedProject`, `IncludeIssue`, `SyntaxDiagnostic`, `Span`/`Point`):
it neither adds nor modifies anything in these crates.

## Current features

Diagnostics only (`textDocument/publishDiagnostics`), via:

- `documents.rs`: `Documents` (in-memory table of open URI → current
  text) and `DocumentsSourceLoader`, a
  `pactole_storage_fs::SourceLoader` that serves the buffer of an open
  (potentially unsaved) document when one exists for a given path, with
  a fallback to `FsSourceLoader` otherwise.
- `conversion.rs`: `span_to_range`/`point_to_position`, converting
  `pactole-syntax`'s byte-based `Span`/`Point` into LSP's UTF-16
  `Range`/`Position`, by re-scanning the relevant source line.
- `diagnostics.rs`: `diagnostics_for_file` (syntax + lowering
  diagnostics for a `ParsedFile`) and `project_diagnostics` (per-file
  diagnostics, plus `IncludeIssue`s attached to the file containing the
  offending `include`, for an `AnalyzedProject`).
- `config.rs`: `Config`, resolved once from the `initializationOptions`
  received at `initialize`. Only one optional key is supported:
  `journal_file` (path of the root `.pactole` file, resolved against the
  workspace root, or against the server's current working directory if
  no root is known). **Does not read `~/.config/pactole`**: an explicit,
  documented limitation of this first version.
- `server.rs`: the stdio `Connection`, the main loop, and the
  notification handlers. When `journal_file` is configured and loads,
  diagnostics are computed for every file reachable via `include` (via
  `analyze_file_with_loader`) and published per file; otherwise the
  current document alone is analyzed standalone via `analyze_file` (its
  `include`s are then not followed). URIs that leave the analyzed set
  (e.g. a removed `include`) receive an empty diagnostics list so they
  never go stale.

## Explicitly out of scope (first version)

Completion, hover, navigation, formatting via LSP (use `pactole fmt`),
incremental sync, reading `~/.config/pactole`. Any unhandled request
receives a `MethodNotFound` error rather than being silently ignored.
See the roadmap in [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) §10
for the planned status of these items.

## Client-side configuration

Pass `journal_file` in the `initializationOptions` of the LSP
`initialize` request, for example:

```json
{ "journal_file": "main.pactole" }
```

## Dependencies / boundaries

- `lsp-server`, `lsp-types` (LSP protocol), `serde`/`serde_json` (JSON
  deserialization), `pactole-storage-fs`, `pactole-syntax`.
- `log` + `env_logger` for debug traces (see below).
- No `thiserror` dependency: no error types of its own, only error
  propagation via `Box<dyn Error + Sync + Send>` in `server.rs`.

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
  the current document (`journal_file` configured but unreadable), and
  clearing of all published diagnostics when no analysis is available.
- `trace`: origin of each file loaded during `include` resolution (open
  in-memory buffer or filesystem), via `DocumentsSourceLoader`.

Document content is never logged, only metadata (URI, lengths, paths).

## Tests / validation

```sh
cargo test -p pactole-lsp
cargo clippy -p pactole-lsp --all-targets
```

Unit tests cover notably `config.rs` (`journal_file` resolution) and
`server.rs` (computing the URIs to clear between two diagnostics
publications).

## See also

- [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) §5 — full
  description of the current LSP, its limits, and the roadmap (global
  config, positioned business diagnostics, completion/hover/navigation,
  LSP formatting, incremental sync).
