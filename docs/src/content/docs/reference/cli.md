---
title: Command-line interface
description: The pactole binary, its subcommands, options and exit codes.
sidebar:
  order: 3
---

The `pactole` binary exposes six subcommands. Run `pactole --help` or
`pactole <command> --help` for the built-in help, and `pactole --version` to
print the version.

On success a command exits with status `0`. On failure it prints a line starting
with `error:` on stderr and exits with a non-zero status.

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

## `pactole check`

```text
pactole check <FILE>
```

Parses the file like `parse`, then validates the resulting journal (see the
[validation invariants](/reference/directives/#validation-invariants)). Prints
`OK` on success.

```sh
pactole check ledger.pactole
```

## `pactole init`

```text
pactole init [LEDGER] [OPTIONS]
```

Initializes a starter ledger and, when it does not already exist, the OFX import
configuration. The default ledger path is `ledger.pactole`. Existing files are
never overwritten.

| Option | Description |
|---|---|
| `[LEDGER]` | Ledger path; defaults to `ledger.pactole` |
| `--config <PATH>` | OFX configuration path; defaults to the user configuration directory |
| `--account <ACCOUNT>` | Bank account; defaults to `Assets:Checking` |
| `--currency <COMMODITY>` | Initial commodity; defaults to `EUR` |
| `--counter-account <ACCOUNT>` | Default import counter-account; defaults to `Expenses:ToReview` |
| `--date <DATE>` | Date for declarations; defaults to today |
| `--stdout` | Print the generated ledger instead of creating its file |
| `--no-config` | Do not initialize the OFX configuration |
| `--no-ledger` | Do not initialize the ledger |

For example:

```sh
pactole init
pactole init finances.pactole --account Assets:Bank --currency USD
pactole init --config ./import-ofx.toml --counter-account Expenses:Review
```

The generated ledger contains the initial commodity and account declarations,
but no fictitious transaction. It is immediately suitable for `pactole check`.
The generated OFX configuration contains commented examples for categorization
rules.

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
pactole import-ofx <FILE> [--config <PATH>] [--into <PATH>] [--write]
```

Imports an OFX or QFX bank statement into canonical `.pactole` transaction
directives. Both OFX 1.x SGML and OFX 2.x XML are supported.

| Option | Description |
|---|---|
| `<FILE>` | OFX or QFX statement to import |
| `--config <PATH>` | TOML configuration containing the bank account and categorization rules |
| `--into <PATH>` | Existing `.pactole` file whose `fitid` metadata is used for deduplication |
| `--write` | Append new transactions to `--into` instead of printing them; requires `--into` |

Without `--config`, Pactole reads:

```text
~/.config/pactole/import-ofx.toml
```

When `XDG_CONFIG_HOME` is set, it reads
`$XDG_CONFIG_HOME/pactole/import-ofx.toml` instead.

Without `--into`, generated transactions are printed to stdout. With `--into`,
source transactions whose OFX `FITID` is already stored as `fitid` metadata in
the target file are skipped. The command reports the imported and skipped counts
on stderr.

The generated output is a journal fragment: it contains transactions, but does
not declare accounts, commodities or payees. Add it to a complete journal before
running `pactole check`.

See [Import an OFX bank statement](/how-to/import-ofx-bank-statement/) for the
configuration format and categorization examples.

## Related

- [Getting started](/tutorials/getting-started/) — a guided tour of these
  commands.
- [Query with the register report](/how-to/query-with-register/) — worked
  filtering examples.
- [Import an OFX bank statement](/how-to/import-ofx-bank-statement/) — configure
  and run bank statement imports.
