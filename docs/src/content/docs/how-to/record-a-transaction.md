---
title: Record a transaction
description: Add a balanced transaction to your journal and understand every field of its header.
sidebar:
  order: 2
---

This guide shows how to write a transaction and which rules it must satisfy.

## The minimal transaction

A transaction is a header line followed by indented postings:

```pactole
2026-09-03 * "Whole Foods" "Weekly groceries"
  Expenses:Groceries 45.30 EUR
  Assets:Checking -45.30 EUR
```

The header has this shape:

```text
 DATE[=EFFECTIVE_DATE] STATUS? "payee" ["narration"] [TAGS/LINKS] [(reference)]
```

- **DATE** is the transaction date in ISO `YYYY-MM-DD` format.
- **EFFECTIVE_DATE** (optional) is a second date after `=`, borrowed from
  Ledger-cli.
- **STATUS** is optional. When present, it is one of `*` (cleared), `!`
  (pending) or `?` (uncleared). When omitted, the transaction is uncleared.
- **payee** is a quoted string and is **mandatory**.
- **narration** is an optional second quoted string.
- **TAGS/LINKS** are zero or more `#tag` and `^link` tokens, freely interleaved.
- **reference** is optional free text between parentheses, e.g. `(CB-00123)`.

## Balance the postings

Every posting is an indented line with an account and — usually — an amount.
All postings in a transaction must sum to zero, per commodity:

```pactole
2026-09-04 * "Bank" "Transfer between accounts"
  Assets:Checking   -1,000.00 EUR
  Assets:Savings     1,000.00 EUR
```

At most one posting may omit its amount; Pactole infers it so that the
transaction balances:

```pactole
2026-09-03 * "Whole Foods" "Weekly groceries"
  Expenses:Groceries 45.30 EUR
  Assets:Checking
```

## Satisfy the journal's rules

Before a transaction is accepted by `pactole check`, the following must hold:

- Every account it uses must be opened beforehand (see `open`) and not already
  closed.
- Every commodity it uses must be declared (see `commodity`), and be allowed by
  the account's `open` restriction if one was given.
- Its payee must be declared somewhere in the journal with a `payee` directive.

The payee rule is configurable: set `[validation] payee = "warn"` in
`pactole.toml` (it defaults to `"error"`) to report undeclared payees as
warnings instead. `--strict` still fails on them.

```pactole
2026-01-01 commodity EUR
2026-01-01 open Assets:Checking
2026-01-01 open Expenses:Groceries
payee "Whole Foods"
```

## Add metadata

You can attach metadata either to the transaction (right after its header,
before any posting) or to a specific posting (right after it):

```pactole
2026-09-03 * "Whole Foods" "Weekly groceries"
  note: "scanned receipt"
  Expenses:Groceries 45.30 EUR
    total_paid: 45.30
  Assets:Checking -45.30 EUR
```

Metadata keys are lowercase (`a-z`, `_`, `-` only); values may be a string, a
date or a number.

## Related

- [Grammar reference](/reference/grammar/) — the full syntax.
- [Directives reference](/reference/directives/) — every directive at a glance.
- [Reconcile with balance assertions](/how-to/reconcile-with-balance-assertions/)
  — prove your postings match reality.
