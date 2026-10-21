---
title: Use the Pactole language server
description: Install and configure the Pactole Language Server Protocol server in your editor.
sidebar:
  order: 2
---

The Pactole language server provides diagnostics while you edit `.pactole`
files. It communicates with editors over the standard input/output (stdio)
transport.

## Install the language server

You need a working Rust toolchain. Install the published crate with Cargo:

```sh
cargo install pactole-lsp
```

This installs the `pactole-lsp` executable in Cargo's bin directory
(`~/.cargo/bin` by default). Make sure that directory is on your `PATH`.

The command used by your editor is:

```text
pactole-lsp
```

The server takes no required argument. `pactole-lsp --version` prints its
name and version, and the server also announces them in its `initialize`
response, so your editor can display them in its language server log.

## Configure your editor

Register `pactole-lsp` as the language server for files with the `.pactole`
extension, using the stdio transport. If your editor supports LSP
`initializationOptions`, pass the root journal file as follows:

```json
{
  "journal_file": "main.pactole"
}
```

The path is resolved relative to the workspace root. With this option, the
server analyzes the root journal and every file reachable through an `include`.
Diagnostics are published for each file separately.

Without `journal_file`, the server analyzes only the document currently open in
the editor. This is also the fallback used when the configured journal cannot
be loaded.

Validation settings come from the same layered `pactole.toml` as the CLI: the
user-level `pactole.toml` (`~/.config/pactole/pactole.toml`), then the
project `pactole.toml` (next to `journal_file`, or at the workspace root when
no `journal_file` is set). They can be overridden from
`initializationOptions`, either globally or per check:

```json
{
  "journal_file": "main.pactole",
  "validation": "warn"
}
```

```json
{
  "journal_file": "main.pactole",
  "validation": { "payee": "warn" }
}
```

With `warn`, an undeclared payee is reported as a warning instead of an error,
so a journal being migrated from another tool stays green in the editor. A
broken or unreadable configuration file is logged and ignored rather than
failing initialization.

The `[declaration]` section of `pactole.toml` can redirect new declarations to
separate files:

```toml
[declaration]
payees = "payees.pactole"
accounts = "accounts.pactole"
commodities = "commodities.pactole"
```

Each path is resolved against the journal's directory, like an `include`. The
quick fix that declares a missing payee, account or commodity then appends to
the matching file, provided that file is reachable from the root journal
through an `include`; otherwise it falls back to the current document. The
server never creates the file nor adds the `include`.

See [Diagnostics and project analysis](/reference/diagnostics/) for the
standalone/project distinction, partial analysis behavior and include
diagnostic codes.

## Editor-specific configuration

For Helix, see the [Helix integration guide](https://github.com/etienne-monier/pactole/blob/main/HELIX.md), which also
explains how to install Pactole syntax highlighting.

For other editors, configure a stdio language server whose command is
`pactole-lsp`. The exact configuration format depends on the editor.

See [Configure editors](/how-to/configure-editors/) for complete Zed, Helix
and generic editor configuration examples, including the external formatter.

## What the language server does

- reports syntax errors and missing syntax nodes;
- reports lowering errors for invalid directives;
- reports business validation errors when a complete `journal_file` project is
  available;
- analyzes included files when `journal_file` is configured;
- uses an open document's current, unsaved contents when resolving includes;
- clears diagnostics for files that are no longer part of the analyzed journal;
- offers the current date as a completion at the beginning of a directive;
  selecting it inserts the date in `YYYY-MM-DD` format and replaces a partial
  date such as `2026-`;
- offers a quick fix to insert the date of the day on an undated `open`,
  `close`, `commodity`, `balance` or transaction directive;
- offers a quick fix to declare a missing payee, account or commodity, either
  above the offending transaction or in a `[declaration]` file;
- jumps from an account, payee or commodity to its declaration
  (`textDocument/definition`), and from an `include` path to the included
  file;
- lists every occurrence of an account, payee or commodity across the
  include graph (`textDocument/references`);
- renames an account or a payee across the whole include graph
  (`textDocument/rename`, with `textDocument/prepareRename`); a new name
  that is invalid, or that would create a duplicate `open` or `payee`
  declaration, is refused with an error.

Document synchronization is full-document rather than incremental. This keeps
the server simple and robust for the typically small, hand-edited journals
Pactole is designed for.

## Current limitations

The language server does not currently provide:

- document symbols (`textDocument/documentSymbol`);
- automatic insertion without selecting a completion or code action;
- formatting through LSP (use [`pactole fmt`](/reference/cli/) instead);
- incremental synchronization.

To enable server logs, set `RUST_LOG` before starting your editor, for example:

```sh
RUST_LOG=pactole_lsp=debug pactole-lsp
```

Logs are written to stderr so that they do not interfere with the LSP protocol.
