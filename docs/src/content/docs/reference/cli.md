---
title: Command-line interface
description: The pactole binary, its subcommands, options and exit codes.
sidebar:
  order: 4
---

The `pactole` binary exposes twelve subcommands. Run `pactole --help` or
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
(`[validation]`, `[declaration]`, `[format]`, `[reports]`, `[import]`) lives in
that single file; unknown keys are rejected so typos fail loudly.

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
pactole check <FILE> [--strict] [--format text|json]
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

A broken file does not hide the problems of the other files: the business
checks that do not depend on the missing content are still reported, and the
ones that do are skipped. Each partial file gets an `analysis-incomplete`
warning explaining what was left out.

See [Diagnostics and project analysis](/reference/diagnostics/) for the
distinction between standalone and project analysis, diagnostic severities and
the include error codes.

```sh
pactole check ledger.pactole
pactole check --strict ledger.pactole
pactole check ledger.pactole --format json
```

`--format json` writes an object to stdout with an `ok` boolean and a
`diagnostics` array. Each diagnostic includes its file, one-based source range,
severity, stable code and message. If the root file cannot be loaded, the JSON
includes an `error` object. The command's exit status is unchanged by the
format.

## `pactole graph`

```text
pactole graph <FILE> [--scan <DIR>] [--format text|json]
```

Displays the include graph rooted at `<FILE>`, including unresolved include
targets and cycles. Files targeted by more than one include directive are
marked as included more than once; this also identifies diamond-shaped include
graphs. The command recursively scans for `.pactole` files in the root journal's
directory and lists those not reachable from the root. Use `--scan` to choose a
different scan directory.

| Option | Description |
|---|---|
| `<FILE>` | Root `.pactole` file |
| `--scan <DIR>` | Directory recursively scanned for unreachable `.pactole` files; defaults to the root journal's directory |
| `--format <FORMAT>` | Output format: `text` (default) or `json` |

```sh
pactole graph ledger.pactole
pactole graph ledger.pactole --format json
pactole graph ledger.pactole --scan ./journals
```

JSON output provides sorted `files` and `edges`, an `unreachable` list,
`cycles`, and all include diagnostics. Each file is marked `reachable`,
`unreachable` or `unresolved`. Include cycles are warnings; unresolved includes
remain errors and make the command exit unsuccessfully.

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
pactole register <FILE> [FILTERS] [--format text|json]
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
| `--format <FORMAT>`     | Output format: `text` (default) or `json`                                  |

```sh
pactole register ledger.pactole
pactole register ledger.pactole --account Expenses:Groceries
pactole register ledger.pactole --from 2026-01-01 --to 2026-12-31
pactole register ledger.pactole --payee Whole --status cleared
pactole register ledger.pactole --tag food
pactole register ledger.pactole --format json
```

The JSON form is an array with one object per matched posting. Decimal amounts
and running balances are strings to preserve exact precision.

`--account` also matches sub-accounts: `--account Expenses` matches
`Expenses:Groceries`.

## `pactole balance`

```text
pactole balance <FILE> [--date <DATE>] [--format <FORMAT>] [--depth <N>]
```

Builds the balance sheet of a file as of a reference date. It has an *Assets*
section, a *Liabilities* section and an *Equity* section, each with its own
subtotal per commodity. Liabilities and equity are shown with a positive sign,
so a fully classified journal without cross-commodity transaction-cost
annotations satisfies `Assets = Liabilities + Equity` for every commodity.
Converted transactions balance on their `@`/`@@` weights, but the balance sheet
uses recorded account amounts in their original commodities; its unconverted
totals can therefore be non-zero for such a journal.

| Option | Description |
|---|---|
| `<FILE>` | Path to the file to read |
| `--date <DATE>` | Reference date (`YYYY-MM-DD`); defaults to today |
| `--format <FORMAT>` | Output format: `text` (default), `json` or `csv` |
| `--depth <N>` | Fold the accounts deeper than `N` levels |

```sh
pactole balance ledger.pactole
pactole balance ledger.pactole --date 2026-03-31
pactole balance ledger.pactole --date 2026-03-31 --format json
pactole balance ledger.pactole --depth 1
```

Income and expense accounts are not sections of their own: their net result is
added to the *Equity* section as an `Unclosed result` line, unless it is zero.
Accounts whose root matches no configured category are reported in an
`Unclassified` section instead of being dropped.

The categories are matched by account root through the `[reports]` section of
`pactole.toml` (see the [configuration reference](/reference/configuration/)).
Without it, the default English roots `Assets`, `Liabilities`, `Equity`,
`Income` and `Expenses` are used, so a journal written in another language
configures its own roots:

```toml
[reports]
assets = ["Actifs"]
liabilities = ["Passif"]
equity = ["Capitaux"]
income = ["Revenus"]
expenses = ["Depenses", "Impots"]
```

The report is built from the validated journal, so an invalid ledger fails the
command like `pactole check` does. The JSON rendering exposes the same sections
and subtotals with amounts as strings; the CSV rendering adds a `section`
column and a `subtotal` row kind.

## `pactole income`

```text
pactole income <FILE> [--from <DATE>] [--to <DATE>] [--by account|payee|tag]
                      [--format <FORMAT>] [--depth <N>]
```

Builds the income statement of a file over a period. Income accounts are
shown as positive amounts, expense accounts are shown as positive amounts,
and the total is the net result (`income − expenses`) per commodity.

| Option | Description |
|---|---|
| `<FILE>` | Path to the file to read |
| `--from <DATE>` | Start of the period (`YYYY-MM-DD`). Defaults to January 1 of the year containing `--to` |
| `--to <DATE>` | End of the period (`YYYY-MM-DD`). Defaults to today |
| `--by <GROUP>` | Grouping: `account` (default), `payee` or `tag` |
| `--format <FORMAT>` | Output format: `text` (default), `json` or `csv` |
| `--depth <N>` | Fold the accounts deeper than `N` levels (only when `--by account`) |

```sh
pactole income ledger.pactole
pactole income ledger.pactole --from 2026-01-01 --to 2026-03-31
pactole income ledger.pactole --by payee
pactole income ledger.pactole --by tag --format json
pactole income ledger.pactole --by account --depth 1
```

Income and expenses are classified by account root through the `[reports]`
section of `pactole.toml` (see the [configuration reference](/reference/configuration/)).
The report contains an *Income* section and an *Expenses* section. When
`--by payee`, lines are grouped by the transaction payee; transactions with
no payee appear as `(no payee)`. When `--by tag`, a transaction with several
tags is counted under each tag and untagged transactions appear as
`(untagged)`; in tag mode the sum of the lines can exceed the section
subtotal, but the subtotal and the net result are computed from the exact
amounts.

**Multi-commodity behaviour:** report amounts are not converted implicitly.
Each commodity keeps its own totals, so income and expenses in different
currencies appear side by side and the net result is reported per commodity.
Posting `@`/`@@` annotations affect transaction balancing only; report
valuation remains a separate feature.

The report is built from the validated journal, so an invalid ledger fails the
command like `pactole check` does. The JSON rendering exposes the same
sections and subtotals with amounts as strings; the CSV rendering adds a
`section` column and a `subtotal` row kind.

## `pactole expenses`

```text
pactole expenses <FILE> [--from <DATE>] [--to <DATE>] [--by account|payee|tag|month]
                        [--format <FORMAT>] [--depth <N>] [--uncategorized] [--inactive]
                        [--compare previous | --compare-from <DATE> --compare-to <DATE>]
                        [--diff absolute|percent] [--reverse-diff]
```

Builds the expenses breakdown of a file over a period. Only expense
accounts are included. Amounts keep their accounting sign (a refund appears
as a negative amount), and the total is the period's spending per commodity.

| Option | Description |
|---|---|
| `<FILE>` | Path to the file to read |
| `--from <DATE>` | Start of the period (`YYYY-MM-DD`). Defaults to January 1 of the year containing `--to` |
| `--to <DATE>` | End of the period (`YYYY-MM-DD`). Defaults to today |
| `--by <GROUP>` | Grouping: `account` (default), `payee`, `tag` or `month` |
| `--format <FORMAT>` | Output format: `text` (default), `json` or `csv` |
| `--depth <N>` | Fold the accounts deeper than `N` levels (only when `--by account`) |
| `--uncategorized` | Show uncategorized transactions (from `[reports] uncategorized` or `[import] default_counter_account`) |
| `--inactive` | Show inactive expense accounts (opened during or before the period, not closed before its start, with no postings in the period) |
| `--compare <PERIOD>` | Compare against `previous` or provide `--compare-from`/`--compare-to` |
| `--compare-from <DATE>` | Start of the comparison period |
| `--compare-to <DATE>` | End of the comparison period |
| `--diff <MODE>` | Difference mode: `absolute` (default) or `percent` |
| `--reverse-diff` | Use `previous − current` for the difference (positive means you saved money) |

```sh
pactole expenses ledger.pactole
pactole expenses ledger.pactole --from 2026-01-01 --to 2026-03-31
pactole expenses ledger.pactole --by payee
pactole expenses ledger.pactole --by tag --format json
pactole expenses ledger.pactole --by month
pactole expenses ledger.pactole --by account --depth 1
pactole expenses ledger.pactole --from 2026-02-01 --to 2026-02-28 --compare previous --diff percent
pactole expenses ledger.pactole --from 2026-02-01 --to 2026-02-28 --compare previous --reverse-diff
pactole expenses ledger.pactole --uncategorized --inactive
```

Expenses are classified by account root through the `[reports]` section of
`pactole.toml` (see the [configuration reference](/reference/configuration/)).
When `--by month`, one line is emitted per calendar month from `--from` to
`--to` (months with no spending show a zero amount).
`--uncategorized` and `--inactive` add named sections to the report and are
available with every grouping. Configure one or more accounts for the former
with `[reports] uncategorized`; when omitted, the configured import counter
account is used. The inactive section lists expense accounts open during the
period that had no posting in it.

`--compare previous` compares with the preceding period. Complete calendar
month ranges shift by the same number of months (so February compares with
January even when their day counts differ); other ranges shift by the same
number of days. Explicit comparison bounds must describe a period of the same
length under that rule. `--diff percent` uses the previous amount as the base;
when it is zero the difference is `n/a` in text/CSV and `null` in JSON.
By default the difference is `current − previous`, so a positive value means
spending increased. `--reverse-diff` switches it to `previous − current`, so a
positive value means spending decreased. Comparisons cannot be combined with
`--by month`, `--uncategorized` or `--inactive`.

The report is built from the validated journal, so an invalid ledger fails the
command like `pactole check` does. The JSON and CSV renderings are the same
render-agnostic formats used by the other reports.

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

## `pactole import-actual`

```text
pactole import-actual <FILE> --currency <CURRENCY> [--output <PATH>]
  [--opening-equity <ACCOUNT>] [--opening-date <DATE>]
  [--payee-metadata-key <KEY>]
```

Performs a one-shot migration from an Actual Budget SQLite database. Unlike
`import-ofx`, this command does not append to or deduplicate an existing
journal, and it does not read the `[import]` configuration: all migration
options are passed on the command line.

| Option | Description |
|---|---|
| `<FILE>` | Actual Budget SQLite database |
| `--currency <CURRENCY>` | Commodity used by the generated postings (required) |
| `--output <PATH>` | Output journal; stdout when omitted |
| `--opening-equity <ACCOUNT>` | Counter-account for opening balances; defaults to `Equity:Opening` |
| `--opening-date <DATE>` | Date of generated declarations and opening transaction; defaults to `1970-01-01` |
| `--payee-metadata-key <KEY>` | Also preserve the original Actual payee under this transaction metadata key |

The migration creates account, payee and commodity declarations, an explicit
opening transaction, multi-posting transactions for Actual splits, and one
balanced double-entry for each linked transfer. Actual rules using `contains`,
`matches` (regular expressions) and `oneOf` are applied to imported
descriptions.

## Related

- [Configuration reference](/reference/configuration/) — the `pactole.toml`
  schema the commands read.
- [Getting started](/tutorials/getting-started/) — a guided tour of these
  commands.
- [Query with the register report](/how-to/query-with-register/) — worked
  filtering examples.
- [Import an OFX bank statement](/how-to/import-ofx-bank-statement/) — configure
  and run bank statement imports.
