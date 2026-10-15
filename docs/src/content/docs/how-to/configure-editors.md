---
title: Configure editors
description: Configure Pactole syntax highlighting, diagnostics and formatting in your editor.
sidebar:
  order: 3
---

Pactole provides two editor integrations:

- a Tree-sitter grammar for syntax highlighting and structural editor features;
- `pactole-lsp` for diagnostics, completions and date-related code actions.

The `pactole` command-line tool provides formatting through `pactole fmt -`.
The formatter reads the complete document from standard input and writes the
formatted document to standard output, which makes it suitable for editor
integrations.

## Zed

### Install the Pactole extension

Install the Pactole extension from the Zed extensions panel. The extension
provides the Pactole grammar, highlighting queries and language-server
registration.

For local development from a Pactole checkout, use Zed's command palette:

```text
zed: install dev extension
```

Select the `editors/zed` directory. After changing the grammar or extension,
run:

```text
zed: reload window
```

### Configure diagnostics

The extension registers `pactole-lsp`. To analyze the complete journal and its
included files, configure the root journal through Zed's language-server
initialization options in `settings.json`:

```json
{
  "lsp": {
    "pactole-lsp": {
      "initialization_options": {
        "journal_file": "main.pactole"
      }
    }
  }
}
```

The path is resolved relative to the workspace root. Without `journal_file`,
the language server analyzes only the currently open document.

### Configure formatting

Zed does not configure external formatters in the language extension's
`config.toml`. Configure the formatter in `settings.json` instead:

```json
{
  "languages": {
    "Pactole": {
      "formatter": {
        "external": {
          "command": "pactole",
          "arguments": ["fmt", "-"]
        }
      },
      "format_on_save": "on"
    }
  }
}
```

Use `zed: open settings file` to open the user settings file. If the project
should carry this configuration for all contributors, put the same language
section in `.zed/settings.json` at the root of the project.

The `pactole` executable must be available in Zed's `PATH`:

```sh
which pactole
```

Install it with Cargo if necessary:

```sh
cargo install pactole-cli
```

You can format the current document manually with `editor: format`. If Zed
cannot start the formatter, inspect the error with `zed: open log`.

## Helix

The repository contains a complete Helix integration guide with instructions
for installing the grammar, linking the query files and configuring the
language server:

- [Helix integration guide](https://github.com/etienne-monier/pactole/blob/main/HELIX.md)

The essential language configuration is:

```toml
[[language]]
name = "pactole"
scope = "source.pactole"
file-types = ["pactole"]
language-servers = ["pactole-lsp"]
formatter = { command = "pactole", args = ["fmt", "-"] }
auto-format = true
```

The exact `language-server` block depends on the Helix version. The formatter
command is the same: it receives the full buffer on stdin and returns the
formatted buffer on stdout.

After installing or changing the grammar and queries, verify the integration
with:

```sh
hx --health pactole
```

## Other LSP editors

For an editor that supports the Language Server Protocol, configure:

```text
command: pactole-lsp
transport: stdio
```

If the editor supports LSP initialization options, pass the root journal:

```json
{
  "journal_file": "main.pactole"
}
```

Configure formatting as an external command with:

```text
command: pactole
arguments: fmt -
input: document on stdin
output: formatted document on stdout
```

The exact configuration syntax depends on the editor. The formatter does not
read or write the file directly; the editor is responsible for supplying the
buffer on stdin and applying stdout as the new document contents.

## Current limitations

- `pactole-lsp` does not currently provide LSP formatting; use the external
  `pactole fmt -` command.
- The language server uses full-document synchronization rather than
  incremental synchronization.
- The language server does not read configuration from `~/.config/pactole`.

See [Use the Pactole language server](/how-to/use-the-lsp/) for the LSP
features and [Format a ledger](/how-to/format-a-ledger/) for the formatter's
canonical layout and configuration options.
