---
title: Produce financial reports
description: Build a balance sheet, income statement and expense breakdown from a Pactole journal.
sidebar:
  order: 8
---

Once `pactole check` accepts your journal, the same source can produce several
financial views. Reports are read-only and keep amounts per commodity unless
you explicitly request valuation.

## Balance sheet

Use `balance` for account balances at a point in time:

```sh
pactole balance ledger.pactole
pactole balance ledger.pactole --date 2026-03-31
```

The report contains Assets, Liabilities and Equity. Income and expense
accounts are represented by an `Unclosed result` line in Equity. Account roots
are classified through `[reports]`; unknown roots appear in an `Unclassified`
section rather than being dropped.

Use `--depth` to fold account hierarchies:

```sh
pactole balance ledger.pactole --depth 1
```

## Income statement

Use `income` for activity over an inclusive period:

```sh
pactole income ledger.pactole
pactole income ledger.pactole --from 2026-01-01 --to 2026-03-31
```

Income and expenses are displayed as positive amounts and the result is
`income - expenses` per commodity. Choose a grouping with `--by`:

```sh
pactole income ledger.pactole --by account
pactole income ledger.pactole --by payee
pactole income ledger.pactole --by tag
```

When grouping by tag, a transaction with several tags can appear under each
tag. The report's exact subtotal is still computed from the underlying amounts.

## Expense breakdown

Use `expenses` to focus on expense accounts:

```sh
pactole expenses ledger.pactole
pactole expenses ledger.pactole --from 2026-01-01 --to 2026-03-31
pactole expenses ledger.pactole --by payee
pactole expenses ledger.pactole --by tag
pactole expenses ledger.pactole --by month
```

Expense amounts keep their accounting sign, so a refund can appear as a
negative amount. You can also show accounts used to hold uncategorized imports
or expense accounts that had no activity:

```sh
pactole expenses ledger.pactole --uncategorized --inactive
```

Configure the accounts used by `--uncategorized` with
`[reports].uncategorized`. If it is absent, Pactole falls back to
`[import].default_counter_account` when configured.

## Compare expense periods

Compare a period with the preceding period:

```sh
pactole expenses ledger.pactole \
  --from 2026-02-01 --to 2026-02-28 \
  --compare previous --diff percent
```

By default, the difference is `current - previous`, so a positive value means
spending increased. Use `--reverse-diff` when a positive value should mean that
spending decreased. Explicit comparison bounds are also supported:

```sh
pactole expenses ledger.pactole \
  --from 2026-03-01 --to 2026-03-31 \
  --compare-from 2026-02-01 --compare-to 2026-02-28
```

Comparisons require periods of the same length and cannot be combined with
`--by month`, `--uncategorized` or `--inactive`.

## Choose an output format

Reports can be rendered as text, JSON or CSV, depending on the command:

```sh
pactole balance ledger.pactole --format json
pactole income ledger.pactole --format csv
pactole expenses ledger.pactole --format json
```

JSON keeps decimal values as strings so applications do not lose precision.
Reports remain multi-commodity unless you explicitly use
[valuation](/how-to/value-reports-in-another-currency/).

## Configure account classification

If your account roots are not the English defaults, configure them in
`pactole.toml`:

```toml
[reports]
assets = ["Actifs"]
liabilities = ["Passif"]
equity = ["Capitaux"]
income = ["Revenus"]
expenses = ["Depenses", "Impots"]
uncategorized = ["Depenses:A-Classer"]
```

See the [configuration reference](/reference/configuration/) for all report
settings and the [CLI reference](/reference/cli/) for every option.
