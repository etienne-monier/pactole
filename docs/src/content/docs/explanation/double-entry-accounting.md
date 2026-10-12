---
title: Double-entry accounting
description: The one rule that makes a ledger self-checking, explained for newcomers.
sidebar:
  order: 2
---

Pactole uses **double-entry accounting**. If you have only ever tracked
expenses in a spreadsheet, this page explains the idea — and why it is what
makes a plain-text ledger trustworthy.

## Every movement has two sides

Instead of writing down "I spent 45.30 EUR", you record where the money came
from *and* where it went:

```pactole
2026-09-03 * "Whole Foods" "Weekly groceries"
  Expenses:Groceries 45.30 EUR
  Assets:Checking -45.30 EUR
```

Read this as: `Assets:Checking` decreased by 45.30 EUR, and
`Expenses:Groceries` increased by the same amount. The two postings are the two
sides of one event.

## The rule: postings sum to zero

Each transaction's postings must sum to zero, per commodity:

```text
+45.30 EUR  (Expenses:Groceries)
-45.30 EUR  (Assets:Checking)
= 0
```

This is why Pactole can sometimes infer an amount: if a transaction has one
posting without an amount, its value is whatever balances the rest. It is also
why a typo in one amount is caught immediately — the transaction no longer adds
up.

## Accounts and their trees

Accounts are `:`-separated names such as `Assets:Checking` or
`Expenses:Groceries`. The first segment groups related accounts, so all
`Expenses:*` accounts can be summed together, and `--account Expenses` in the
[register report](/how-to/query-with-register/) matches every sub-account.

Unlike Beancount, Pactole does not force a fixed set of root categories
(`Assets`, `Liabilities`, `Equity`, `Income`, `Expenses`): any capitalized name
works. The conventional roots are still a useful habit.

## Signs, not judgement

Pactole does not label accounts as "debit" or "credit" or turn them into assets
and liabilities for you. It just keeps a running sum per account and commodity.
Whether a negative `Assets:Checking` means "overdrawn" is a question you answer
when you read the report, not something the format imposes.

## Assertions: proving it against reality

Double-entry keeps the ledger internally consistent — the numbers add up — but
it says nothing about whether they match your bank. A
[`balance` assertion](/how-to/reconcile-with-balance-assertions/) closes that
gap: it states the expected balance of an account on a date, and
`pactole check` recomputes it from your transactions. When the assertion fails,
either a transaction is missing or one is wrong.

## Related

- [Reconcile with balance assertions](/how-to/reconcile-with-balance-assertions/)
- [Why Pactole](/explanation/why-pactole/)
- [Directives reference](/reference/directives/)
