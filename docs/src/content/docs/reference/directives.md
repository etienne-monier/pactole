---
title: Directives
description: Every Pactole directive, its syntax and the rules it must satisfy.
sidebar:
  order: 2
---

A `.pactole` file is a sequence of directives. This page lists them all with
their syntax and the validation rules enforced by `pactole check`.

## Overview

| Directive     | Purpose                                             | Date? |
|---------------|-----------------------------------------------------|:-----:|
| `open`        | Open an account (optionally restricting commodities) | yes  |
| `close`       | Close an account                                    | yes   |
| `commodity`   | Declare a commodity                                 | yes   |
| `payee`       | Declare a known payee                               | no    |
| `balance`     | Assert an account's balance on a date               | yes   |
| transaction   | Record a movement of funds                          | yes   |
| `include`     | Inline another `.pactole` file                      | no    |

## `open`

```text
DATE open ACCOUNT [COMMODITY_NAME[,COMMODITY_NAME...]]
```

Opens an account. The account must be opened before it is used in a transaction,
a balance assertion, or closed.

```pactole
2026-09-03 open Assets:Checking
2026-09-03 open Assets:Broker EUR,USD
```

The optional comma-separated commodity list (no spaces) restricts the account:
postings using a commodity outside the list are rejected. An empty list leaves
the account unrestricted.

## `close`

```text
DATE close ACCOUNT
```

Closes an account. A closed account can no longer be used after its close date.

```pactole
2026-12-31 close Assets:Checking
```

## `commodity`

```text
DATE commodity COMMODITY_NAME
```

Declares a commodity. A commodity must be declared before it is used in a
transaction or a balance assertion.

```pactole
2026-01-01 commodity EUR
2026-01-01 commodity USD
```

## `payee`

```text
payee "name"
```

Declares a payee as known. Every transaction's payee must match a declaration
somewhere in the journal. The directive carries no date and may appear after its
first use.

```pactole
payee "Whole Foods"
```

## `balance`

```text
DATE balance ACCOUNT NUMBER [~ TOLERANCE] COMMODITY_NAME
```

Asserts the expected balance of an account on a date. `pactole check` recomputes
the running balance of that account and commodity and fails if the difference
exceeds the tolerance (which defaults to zero when `~` is omitted).

```pactole
2026-09-03 balance Assets:Checking 1234.56 ~ 0.01 EUR
```

The account must be open, and the commodity must be declared and allowed by the
account's `open` restriction.

## Transaction

```text
DATE[=EFFECTIVE_DATE] STATUS "payee" ["narration"] TAGS/LINKS [(reference)]
  ACCOUNT AMOUNT?
  ...
```

Records a movement of funds. All postings must sum to zero per commodity; at
most one posting may omit its amount (it is inferred).

```pactole
2026-09-03=2026-09-05 * "Whole Foods" "Weekly groceries" #food (CB-00123)
  Expenses:Groceries 45.30 EUR
  Assets:Checking -45.30 EUR
```

See [Record a transaction](/how-to/record-a-transaction/) for a guided
explanation of each field.

## `include`

```text
include "path"
```

Inlines the entries of another `.pactole` file at the directive's position,
recursively. A relative path is resolved against the directory of the including
file.

```pactole
include "ledger/2026.pactole"
```

See [Split a journal with includes](/how-to/split-a-journal-with-includes/).

## Validation invariants

`pactole check` enforces the following on the flattened journal:

1. **Chronological order** — entries are sorted chronologically.
2. **Account lifecycles** — an account must be opened before it is used (in a
   transaction, a balance assertion) or closed, and cannot be used after it is
   closed.
3. **Declared commodities** — every commodity used must be declared, and allowed
   by any restriction set on the account's `open`.
4. **Declared payees** — every transaction's payee must be declared.
5. **Balanced transactions** — the postings of each transaction sum to zero per
   commodity.
6. **Balance assertions** — each `balance` assertion must match the account's
   running balance up to its date, within the given tolerance.
