---
title: Reconcile with balance assertions
description: Use balance directives to prove that your recorded postings match reality.
sidebar:
  order: 4
---

A `balance` directive asserts the expected balance of an account on a given
date. `pactole check` recomputes the account's running balance from the
transactions that precede it and fails if the two do not match. This is how you
"reconcile" a journal with a bank statement.

## Syntax

```text
DATE balance ACCOUNT NUMBER [~ TOLERANCE] COMMODITY
```

For example, to assert that `Assets:Checking` holds exactly 100.00 EUR on
2026-01-31:

```pactole
2026-01-31 balance Assets:Checking 100.00 EUR
```

Pactole sums every posting on `Assets:Checking` in EUR up to that date and
compares the result to `100.00`. Any mismatch is reported as a
`balance assertion failed` error.

## Allow a tolerance

Real statements round amounts, so an exact match is often impractical. Add a
tolerance after `~` to accept a difference up to that value:

```pactole
2026-01-31 balance Assets:Checking 100.00 ~ 0.01 EUR
```

Without `~`, the tolerance is zero and the match must be exact.

## A worked example

```pactole
2026-01-01 open Assets:Checking
2026-01-01 open Expenses:Groceries
2026-01-01 commodity EUR
payee "Whole Foods"

2026-09-03 * "Whole Foods" "Weekly groceries"
  Expenses:Groceries 45.30 EUR
  Assets:Checking -45.30 EUR

2026-09-30 balance Assets:Checking -45.30 EUR
```

Run the check to confirm the assertion holds:

```sh
pactole check ledger.pactole
```

Now change the assertion to `-45.00 EUR` and run `check` again: Pactole reports
a `balance assertion failed` error naming the expected and actual balances.

## Rules to keep in mind

- The asserted account must be open on the assertion's date.
- The asserted commodity must be declared, and allowed by the account's `open`
  restriction if one was given.
- Balances are tracked **per commodity**: asserting EUR says nothing about an
  account's USD balance.

## Related

- [Directives reference](/reference/directives/) — `balance` among the others.
- [Explanation: double-entry accounting](/explanation/double-entry-accounting/)
  — why assertions matter.
