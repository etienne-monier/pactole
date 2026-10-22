---
title: Command-line interface
description: The pactole binary, its subcommands, options and exit codes.
sidebar:
  order: 4
---

The `pactole` binary exposes seven subcommands. Run `pactole --help` or
`pactole <command> --help` for the built-in help, and `pactole --version` to
print the version.

On success a command exits with status `0`. Errors are printed on stderr and
cause a non-zero status. Commands that support warnings may still exit with
status `0` unless `--strict` is passed.

## Global options

One option is accepted **before** the subcommand:

| Option | Description |
|---|---|
| `--config <PATH>` | Use this `pactole.toml` as the only configuration file, skipping global and project discovery |

```sh
pactole --config ./pactole.toml check ledger.pactole
pactole check ledger.pactole
```

Without `--config`, Pactole reads the user-level `pactole.toml`
(`~/.config/pactole/pactole.toml`, or
`$XDG_CONFIG_HOME/pactole/pactole.toml`), then the project `pactole.toml`
next to the journal being processed, merging them key by key. Every setting
(`[validation]`, `[declaration]`, `[format]`, `[import]`) lives in that single file; unknown
keys are rejected so typos fail loudly.

## `pactole parse`

```text
pactole parse <FILE>
```

Parses a `.pactole` file and prints the resulting journal entries, one per
entry, using their Rust `Debug` representation. Meant mainly for debugging the
grammar and parser.

```sh
pactole parse ledger.pactole
```

## `pactole fmt`

```text
pactole fmt <FILE> [--write]
```

Reformats a `.pactole` file into its canonical layout. By default the result is
printed to stdout; pass `--write` to update the file in place.

| Argument   | Description                                          |
|------------|------------------------------------------------------|
| `<FILE>`   | Path to the file to format, or `-` to read from stdin |
| `--write`  | Write the result back to `<FILE>` instead of printing it |

With `FILE` set to `-`, the input is read from stdin and the result is always
printed to stdout (`--write` is ignored).

```sh
pactole fmt ledger.pactole
pactole fmt --write ledger.pactole
cat ledger.pactole | pactole fmt -
```

## `pactole completions`

```text
pactole completions <SHELL>
```

Generates a completion script on stdout for one of the supported shells:
`bash`, `elvish`, `fish`, `powershell` or `zsh`.

For example, to install completions for Zsh:

```sh
mkdir -p ~/.zfunc
pactole completions zsh > ~/.zfunc/_pactole
```

For Bash:

```sh
mkdir -p ~/.local/share/bash-completion/completions
pactole completions bash > ~/.local/share/bash-completion/completions/pactole
```

## `pactole check`

```text
pactole check <FILE> [--strict]
```

Parses the file like `parse`, then validates the resulting journal (see the
[validation invariants](/reference/directives/#validation-invariants). Prints
`OK` on success. Warnings are printed on stderr but do not fail the command by
default; `--strict` treats warnings as errors.

A configurable check such as the mandatory payee declaration can be
downgraded to a warning in `pactole.toml`:

```toml
[validation]
payee = "warn"
```

Every other business check remains an error. See the
[configuration reference](/reference/configuration/) for the whole schema.

Errors include syntax errors, lowering errors, business validation errors and
includes whose content could not be loaded. Include cycles are warnings because
all files involved have still been loaded and validated.

See [Diagnostics and project analysis](/reference/diagnostics/) for the
distinction between standalone and project analysis, diagnostic severities and
the include error codes.

```sh
pactole check ledger.pactole
pactole check --strict ledger.pactole
```

## `pactole init`

```text
pactole init [LEDGER] [OPTIONS]
```

Initializes a starter ledger and, when it does not already exist, the project
`pactole.toml` configuration. The default ledger path is `ledger.pactole`.
Existing files are never overwritten.

| Option | Description |
|---|---|
| `[LEDGER]` | Ledger path; defaults to `ledger.pactole` |
| `--account <ACCOUNT>` | Bank account; defaults to `Assets:Checking` |
| `--currency <COMMODITY>` | Initial commodity; defaults to `EUR` |
| `--counter-account <ACCOUNT>` | Default import counter-account; defaults to `Expenses:ToReview` |
| `--date <DATE>` | Date for declarations; defaults to today |
| `--global` | Write `pactole.toml` into the user configuration directory instead of next to the ledger |
| `--stdout` | Print the generated ledger instead of creating its file |
| `--no-config` | Do not initialize `pactole.toml` |
| `--no-ledger` | Do not initialize the ledger |

`--no-config` and `--no-ledger` cannot be combined: at least one of the two
files must be created.

For example:

```sh
pactole init
pactole init finances.pactole --account Assets:Bank --currency USD
pactole init --global
```

The generated ledger contains the initial commodity and account declarations,
but no fictitious transaction. It is immediately suitable for `pactole check`.
The generated configuration contains commented examples for the OFX import
settings and categorization rules.

## `pactole register`

```text
pactole register <FILE> [FILTERS]
```

Lists the transactions of a file, one line per matching posting, with a running
balance kept per commodity. Without any filter, every posting is listed. Filters
can be combined; a posting is shown only when all of them match.

| Option                  | Description                                                                 |
|-------------------------|-----------------------------------------------------------------------------|
| `<FILE>`                | Path to the file to read                                                    |
| `-a`, `--account <ACCOUNT>` | Only postings on this account or one of its sub-accounts                |
| `--from <DATE>`         | Only transactions on or after this date (`YYYY-MM-DD`)                      |
| `--to <DATE>`           | Only transactions on or before this date (`YYYY-MM-DD`)                     |
| `--payee <TEXT>`        | Only transactions whose payee contains this text (case-insensitive)         |
| `--narration <TEXT>`    | Only transactions whose narration contains this text (case-insensitive)     |
| `--tag <TAG>`           | Only transactions carrying this tag                                         |
| `--status <STATUS>`     | Only transactions with this status: `cleared`, `pending` or `uncleared`     |

```sh
pactole register ledger.pactole
pactole register ledger.pactole --account Expenses:Groceries
pactole register ledger.pactole --from 2026-01-01 --to 2026-12-31
pactole register ledger.pactole --payee Whole --status cleared
pactole register ledger.pactole --tag food
```

`--account` also matches sub-accounts: `--account Expenses` matches
`Expenses:Groceries`.

## `pactole import-ofx`

```text
pactole import-ofx <FILE> [--into <PATH>] [--account <ACCOUNT>] [--write]
```

Imports an OFX or QFX bank statement into canonical `.pactole` transaction
directives. Both OFX 1.x SGML and OFX 2.x XML are supported.

| Option | Description |
|---|---|
| `<FILE>` | OFX or QFX statement to import |
| `--into <PATH>` | Existing `.pactole` file whose `fitid` metadata is used for deduplication |
| `--account <ACCOUNT>` | Post every imported transaction to this account, overriding `import.bank_account` and the `[import.accounts]` mapping for this run |
| `--write` | Append new transactions to `--into` instead of printing them; requires `--into` |

The bank account and categorization rules come from the `[import]` section of
the resolved `pactole.toml` (see [Global options](#global-options)); use the
global `--config` flag to point at a specific file:

```sh
pactole --config ./pactole.toml import-ofx statement.ofx
```

```toml
[import]
bank_account = "Actifs:Banque"
currency = "EUR"
default_counter_account = "Depenses:A-Classer"

[import.accounts]
"00012345678" = "Actifs:Courant"

[[import.rules]]
payee_contains = "carrefour"
counter_account = "Depenses:Alimentation"
payee = "Carrefour"
```

The command fails with a clear message when no `[import]` section is
configured.

Without `--into`, generated transactions are printed to stdout. With `--into`,
source transactions whose OFX `FITID` is already stored as `fitid` metadata in
the target file are skipped. The command reports the imported and skipped counts
on stderr.

Each transaction is posted to the account mapped from its statement's `ACCTID`
in `[import.accounts]`, to `--account` when given, and to `bank_account`
otherwise (with a warning when a mapping table exists but does not cover the
statement account).

The generated output is a journal fragment: it contains transactions, but does
not declare accounts, commodities or payees. Add it to a complete journal before
running `pactole check`.

See [Import an OFX bank statement](/how-to/import-ofx-bank-statement/) for the
configuration format and categorization examples.

## Related

- [Configuration reference](/reference/configuration/) — the `pactole.toml`
  schema the commands read.
- [Getting started](/tutorials/getting-started/) — a guided tour of these
  commands.
- [Query with the register report](/how-to/query-with-register/) — worked
  filtering examples.
- [Import an OFX bank statement](/how-to/import-ofx-bank-statement/) — configure
  and run bank statement imports.
