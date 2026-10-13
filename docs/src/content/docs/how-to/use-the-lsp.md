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

## Editor-specific configuration

For Helix, see the [Helix integration guide](https://github.com/etienne-monier/pactole/blob/main/HELIX.md), which also
explains how to install Pactole syntax highlighting.

For other editors, configure a stdio language server whose command is
`pactole-lsp`. The exact configuration format depends on the editor.

## What the language server does

- reports syntax errors and missing syntax nodes;
- reports lowering errors for invalid directives;
- analyzes included files when `journal_file` is configured;
- uses an open document's current, unsaved contents when resolving includes;
- clears diagnostics for files that are no longer part of the analyzed journal;
- offers the current date as a completion at the beginning of a directive;
  selecting it inserts the date in `YYYY-MM-DD` format and replaces a partial
  date such as `2026-`;
- offers a quick fix to insert the date of the day on an undated `open`,
  `close`, `commodity`, `balance` or transaction directive.

Document synchronization is full-document rather than incremental. This keeps
the server simple and robust for the typically small, hand-edited journals
Pactole is designed for.

## Current limitations

The language server does not currently provide:

- navigation or references;
- automatic insertion without selecting a completion or code action;
- formatting through LSP (use [`pactole fmt`](/reference/cli/) instead);
- incremental synchronization;
- configuration from `~/.config/pactole`.

To enable server logs, set `RUST_LOG` before starting your editor, for example:

```sh
RUST_LOG=pactole_lsp=debug pactole-lsp
```

Logs are written to stderr so that they do not interfere with the LSP protocol.
