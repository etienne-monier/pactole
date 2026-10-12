---
title: Query with the register report
description: List transactions and filter them by account, date, payee, tag or status.
sidebar:
  order: 6
---

The `register` command lists your transactions, one line per matching posting,
with a running balance kept per commodity — like the `register` report of other
plain-text accounting tools.

## List everything

```sh
pactole register ledger.pactole
```

Without any filter, every posting of every transaction is listed. Each line
shows the date, the transaction status, the payee and narration, the account,
the posting amount, and the running balance.

## Filter the report

Filters can be combined; a posting is shown only when **all** of them match.

```sh
# Only postings on this account or one of its sub-accounts.
pactole register ledger.pactole --account Expenses:Groceries

# Restrict to a date range.
pactole register ledger.pactole --from 2026-01-01 --to 2026-12-31

# Match text in the payee or the narration (case-insensitive).
pactole register ledger.pactole --payee Whole
pactole register ledger.pactole --narration groceries

# Filter by tag or by status.
pactole register ledger.pactole --tag food
pactole register ledger.pactole --status cleared
```

`--account` also matches sub-accounts: `--account Expenses` matches
`Expenses:Groceries` too. `--status` accepts `cleared`, `pending` or
`uncleared`.

## Combine filters

```sh
pactole register ledger.pactole --account Expenses --from 2026-09-01 --tag food
```

This shows only the postings on `Expenses` or its sub-accounts, dated on or
after 2026-09-01, from transactions carrying the `food` tag.

## Related

- [Command-line reference](/reference/cli/) — every `register` option.
- [Getting started](/tutorials/getting-started/) — a guided first run.
