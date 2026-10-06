# pactole-syntax

**Tolerant** syntax analysis of `.pactole` source text: positions,
diagnostics, never an unrecoverable failure.

## Role

`pactole-syntax` sits between the raw tree-sitter grammar
(`tree-sitter-pactole`) and higher-level consumers such as
`pactole-storage-fs` or `pactole-lsp`. It performs no I/O and never
depends on `pactole-core`: it only manipulates the concrete syntax tree
(CST) and its diagnostics, never domain models. The parsing it provides
**always succeeds**, even on invalid text: errors are reported as
positioned diagnostics rather than blocking failures.

## Main API

Re-exported from `src/lib.rs`:

- **`span`**: `Point` (byte offset + zero-indexed row/column) and `Span`
  (a pair of `Point` forming a half-open interval), with
  `Span::byte_range()` to slice the source text.
- **`diagnostics`**: `SyntaxDiagnostic` (a `DiagnosticKind`, a `Span`,
  and a message) and `DiagnosticKind` (`Error` for tree-sitter `ERROR`
  nodes, `Missing` for "missing" nodes).
- **`document`**:
  - `parse_document(source: &str) -> ParsedDocument` — entry point of
    the tolerant parsing; `analyze` is an alias of this function;
  - `ParsedDocument` — source + tree-sitter `Tree` + collected
    diagnostics; `ParsedDocument::into_result()` provides a strict
    conversion to `Result<ParsedDocument, Vec<SyntaxDiagnostic>>` for
    consumers (such as `pactole-storage-fs::parser`/`printer`) that only
    want to proceed on a syntactically clean document;
  - `collect_error_nodes(tree: &Tree) -> Vec<SyntaxDiagnostic>` — walks
    a tree-sitter `Tree` and collects `ERROR`/`MISSING` nodes.

## Dependencies / boundaries

- `tree-sitter`, `tree-sitter-pactole` (compiled grammar).
- Never depends on `pactole-core`: this crate is purely syntactic by
  construction, a shared foundation for any future tooling that needs
  positioned diagnostics on possibly-invalid text, without duplicating
  the tree-sitter plumbing for each consumer.

## Tests / validation

```sh
cargo test -p pactole-syntax
cargo clippy -p pactole-syntax --all-targets
```

Unit tests (in `src/lib.rs`) cover valid/invalid parsing, the strict
conversion via `into_result`, and the consistency between
`collect_error_nodes` and a `ParsedDocument`'s diagnostics.

## See also

- [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) — the role of
  `pactole-syntax` in the tolerant/positioned flow and its boundaries
  with `pactole-storage-fs` and `pactole-lsp`.
