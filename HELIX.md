# Using Pactole with Helix

This document explains how to get syntax highlighting for `.pactole` files
in the [Helix](https://helix-editor.com/) editor, using the tree-sitter
grammar in
[`crates/tree-sitter-pactole`](crates/tree-sitter-pactole).

Helix is not aware of the `pactole` language by default: you need to
register the grammar and its highlight queries in your own configuration.
None of this requires building Helix from source or publishing anything —
it works entirely from a local checkout of this repository.

## 1. Register the language and grammar

Add the following to your `languages.toml`, usually located at
`~/.config/helix/languages.toml` (see `hx --health` for the exact path).
Replace the `source.path` value with the absolute path to
`crates/tree-sitter-pactole` on your machine:

```toml
[[language]]
name = "pactole"
scope = "source.pactole"
injection-regex = "pactole"
file-types = ["pactole"]
comment-tokens = ";"
indent = { tab-width = 2, unit = "  " }

[[grammar]]
name = "pactole"
source = { path = "/absolute/path/to/pactole/crates/tree-sitter-pactole" }
```

`source.path` is meant for local development. If you ever publish the
grammar to its own git repository, switch to `source.git`/`source.rev`
instead (this is required before contributing the language upstream to
Helix).

## 2. Build the grammar

Helix compiles tree-sitter grammars into shared libraries under its
runtime directory. Run:

```sh
hx --grammar build
```

This compiles `crates/tree-sitter-pactole` (using the already-generated
`src/parser.c`) into `pactole.so`, placed in
`<runtime>/grammars/pactole.so` (the runtime directory is listed by
`hx --health`, typically `~/.config/helix/runtime`).

If you ever edit `grammar.js`, regenerate the parser first (from
`crates/tree-sitter-pactole`):

```sh
npx tree-sitter generate
```

then re-run `hx --grammar build`.

## 3. Add the highlight queries

Helix looks for query files under `<runtime>/queries/<language>/`. Create
the directory and link (or copy) the grammar's queries into it, so that
future edits to `crates/tree-sitter-pactole/queries/highlights.scm` are
picked up automatically:

```sh
mkdir -p ~/.config/helix/runtime/queries/pactole
ln -sf /absolute/path/to/pactole/crates/tree-sitter-pactole/queries/highlights.scm \
  ~/.config/helix/runtime/queries/pactole/highlights.scm
```

Only `highlights.scm` exists today. Other optional query files
(`indents.scm`, `textobjects.scm`, `injections.scm`, `locals.scm`,
`tags.scm`) can be added later the same way, following the
[tree-sitter query docs](https://tree-sitter.github.io/tree-sitter/3-syntax-highlighting.html#highlights).

## 4. Verify the setup

```sh
hx --health pactole
```

Should report:

```
Tree-sitter parser: ✓
Highlight queries:  ✓
```

Then open any `.pactole` file (e.g. one of the files under
`crates/tree-sitter-pactole/test/comptes/`) to confirm dates, accounts,
amounts, commodities, tags/links, and comments are colored.

## 5. Configure the formatter (optional)

A `pactole fmt` subcommand ships with the CLI (see the top-level
`README.md`). It reads a `.pactole` file (or stdin with `-`) and writes
its canonical form to stdout, so it plugs directly into Helix's
formatter hook:

```sh
cargo install --path crates/pactole-cli
```

```toml
[[language]]
name = "pactole"
# ... (see step 1)
formatter = { command = "pactole", args = ["fmt", "-"] }
auto-format = true
```

`hx --health pactole` should then show a `✓` next to the resolved
`pactole` binary path under "Configured formatter".

## Notes

- If Helix segfaults or behaves oddly after regenerating the grammar,
  delete the stale `<runtime>/grammars/pactole.so` and rebuild.
- If your queries aren't picked up, make sure `HELIX_RUNTIME` isn't
  pointing elsewhere, or check the runtime paths reported by
  `hx --health`.
- The formatter refuses to reformat a file that doesn't parse cleanly
  (to avoid silently corrupting it): if `:w` reports a formatter error,
  fix the syntax error first, e.g. with `pactole parse`.
