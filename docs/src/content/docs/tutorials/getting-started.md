---
title: Getting started
description: Install Pactole, write your first ledger and run it through check, fmt and register.
sidebar:
  order: 1
---

This tutorial walks you through creating your very first Pactole ledger and
using the command-line tool on it. By the end you will have a small, valid
`.pactole` file and know how to check, format and query it.

No prior accounting knowledge is assumed.

## 1. Install Pactole

Pactole is distributed as a Rust crate. If you have a Rust toolchain, install
the `pactole` binary with `cargo`:

```sh
cargo install --path crates/pactole-cli
```

This builds a release binary and copies it to Cargo's bin directory
(`~/.cargo/bin` by default). Make sure that directory is on your `PATH`, then
check that the command is available:

```sh
pactole --version
```

:::tip
If you do not have Rust installed, follow the official instructions at
[rustup.rs](https://rustup.rs/) first.
:::

## 2. Initialize your ledger

Create a starter ledger and the default OFX import configuration:

```sh
pactole init ledger.pactole
```

The command creates `ledger.pactole` and, if it is missing,
`~/.config/pactole/import-ofx.toml`. It never overwrites an existing file. The
starter ledger contains declarations for `Assets:Checking`,
`Expenses:ToReview` and `EUR`, but no fictitious transaction.

## 3. Write your first transaction

A Pactole ledger is a plain text file. Add an expense account, a known payee and
a transaction to the generated `ledger.pactole` so that it contains:

```pactole
2026-01-01 commodity EUR

2026-01-01 open Assets:Checking
2026-01-01 open Expenses:ToReview
2026-01-01 open Expenses:Groceries

payee "Whole Foods"

2026-09-03 * "Whole Foods" "Weekly groceries"
  Expenses:Groceries 45.30 EUR
  Assets:Checking -45.30 EUR
```

Let's read this file top to bottom:

- `commodity EUR` declares the currency you are going to use. Pactole refuses to
  use a commodity that was never declared.
- `open Assets:Checking` and `open Expenses:Groceries` open the two accounts the
  transaction will touch. An account must be opened before it can be used.
- `payee "Whole Foods"` declares the merchant as a known payee. Every
  transaction's payee must be declared somewhere in the journal.
- The last block is a **transaction**. Its header starts with the date, then a
  status flag (`*` means "cleared"), then the payee and an optional narration.
  Its two **postings** (indented lines) move 45.30 EUR out of `Assets:Checking`
  and into `Expenses:Groceries`. Postings always sum to zero.

## 4. Check the ledger

Ask Pactole to validate the file, both grammatically and from a business point
of view:

```sh
pactole check ledger.pactole
```

If everything is fine, the command prints:

```text
OK
```

If something is wrong — a typo in an account name, an undeclared commodity, an
unbalanced transaction — it prints an `error:` message on stderr instead and
exits with a non-zero status. Try it: change `45.30` to `46.30` in one posting
only and run `check` again.

## 5. Format the ledger

Pactole can rewrite a file into a single canonical layout, which is handy to
keep diffs clean when several people edit the same journal. Print the
canonical form to stdout:

```sh
pactole fmt ledger.pactole
```

Or update the file in place:

```sh
pactole fmt --write ledger.pactole
```

## 6. Query your transactions

Finally, list your transactions with a running balance using the `register`
report:

```sh
pactole register ledger.pactole
```

Each matching posting is printed on its own line, with the running balance kept
separately per commodity. You can narrow the report with filters such as
`--account`, `--from`/`--to`, `--payee`, `--tag` and `--status`:

```sh
pactole register ledger.pactole --account Expenses:Groceries
```

## Where to go next

- Import a bank statement with the
  [OFX import guide](/how-to/import-ofx-bank-statement/).
- Add another transaction and explore the filters in the
  [command-line reference](/reference/cli/).
- Learn the meaning of every keyword in the [grammar reference](/reference/grammar/).
- Understand *why* Pactole is built this way in the
  [explanation](/explanation/why-pactole/) section.
