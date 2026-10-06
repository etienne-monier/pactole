# tree-sitter-pactole

[tree-sitter](https://tree-sitter.github.io/tree-sitter/) grammar for
the `.pactole` language, with its generated C parser and Rust bindings.

## Role

This crate defines the **syntax** of the `.pactole` language (see
[`../../GRAMMAR.md`](../../GRAMMAR.md) for the full specification) and
provides a parser usable from Rust. It knows nothing about the Pactole
business domain (accounts, transactions, validation, ...): it is a purely
syntactic building block, reusable by any tree-sitter-based tool (text
editors, other host languages, etc.).

## Contents

- `grammar.js`: grammar definition (source of truth).
- `src/parser.c`, `src/grammar.json`, `src/node-types.json`: C parser and
  metadata generated from `grammar.js` by `tree-sitter generate`.
- `bindings/rust/lib.rs`: Rust binding exposing the compiled grammar
  (`language()` returning a `tree_sitter_language::LanguageFn`, used by
  `pactole-syntax`).
- `bindings/rust/build.rs`: compiles `src/parser.c` as C via the `cc`
  crate at build time.
- `queries/highlights.scm`: syntax highlighting queries for editors (see
  [`../../HELIX.md`](../../HELIX.md) for integration into Helix).
- `test/`: tree-sitter test corpus (`tree-sitter test`).
- `tree-sitter.json`: language metadata for the tree-sitter ecosystem
  (`scope: source.pactole`, extension `.pactole`).

## Dependencies / boundaries

- Build dependency: `cc` (compiling the C parser).
- Runtime dependency: `tree-sitter-language`.
- Dev dependency: `tree-sitter` (for the corpus tests).
- Depends on no other workspace crate. This is the most upstream crate
  of the architecture: `pactole-syntax` depends on it, but never the
  other way around.

## Modifying the grammar

If `grammar.js` is modified, regenerate the parser and propagate the
changes to consumers:

```sh
cd crates/tree-sitter-pactole
npx tree-sitter generate
npx tree-sitter test
```

Then update, as needed:

- `queries/highlights.scm` (new tokens to highlight),
- `crates/pactole-storage-fs/src/parser.rs` and `printer.rs` (new AST
  nodes to convert/format),
- [`../../GRAMMAR.md`](../../GRAMMAR.md) (specification).

## Tests / validation

```sh
# tree-sitter test corpus (pure syntax)
cd crates/tree-sitter-pactole
npx tree-sitter test

# Rust binding compilation and tests
cargo test -p tree-sitter-pactole
```

## See also

- [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) — overall repository
  architecture and boundaries between crates.
- [`../../GRAMMAR.md`](../../GRAMMAR.md) — the `.pactole` language
  specification.
- [`../../HELIX.md`](../../HELIX.md) — syntax highlighting integration
  into Helix.
