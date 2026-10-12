---
title: Command-line interface
description: The pactole binary, its subcommands, options and exit codes.
sidebar:
  order: 3
---

The `pactole` binary exposes four subcommands. Run `pactole --help` or
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

## Related

- [Getting started](/tutorials/getting-started/) — a guided tour of these
  commands.
- [Query with the register report](/how-to/query-with-register/) — worked
  filtering examples.
