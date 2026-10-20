---
title: Differences from Beancount
description: How Pactole's syntax deviates from Beancount, and which features are intentionally missing.
sidebar:
  order: 3
---

Pactole is inspired by [Beancount](https://beancount.github.io/docs/) (with a
couple of nods to Ledger-cli), but it is a smaller language with a handful of
deliberate deviations. If you already know Beancount, this page is the fastest
way to unlearn the right things.

## A smaller set of directives

Only seven directives exist:

`open`, `close`, `commodity`, `payee`, `balance`, a transaction, and `include`.

Beancount directives such as `pad`, `price`, `note`, `event`, `document`,
`query`, `custom`, `option`, `plugin` and `pushtag`/`poptag` are not supported.

## Payee versus narration

In Beancount the first string of a transaction is the narration (and the payee
is optional). Pactole reverses this:

- the **payee is mandatory**;
- the **narration is optional**.

Pactole also adds a `payee "name"` *declaration* directive, borrowed from
Ledger-cli, which has no equivalent in Beancount. Every transaction's payee must
be declared somewhere in the journal, so typos like `"Whole Foods"` vs.
`"Wholefoods"` are rejected. For a soft migration, `[validation] payee =
"warn"` in `pactole.toml` reports
undeclared payees as warnings instead of errors.

## Status flags

Only `*` (cleared), `!` (pending) and `?` (uncleared) are accepted. The status
is optional and defaults to `?` (uncleared). There is no custom flag letter and
no `txn` keyword.

## No cost or price annotations

Postings have no `{cost}`, `@ price` or `@@ total` annotations. These Beancount
features exist mainly for tracking investments, which is out of scope for now.

Account and commodity amounts are therefore plain, single-commodity postings.

## No enforced account roots

Beancount expects account names to start with one of a fixed set of roots
(`Assets`, `Liabilities`, `Equity`, `Income`, `Expenses`). Pactole enforces no
such categories: any capitalized first segment is accepted. The conventional
roots remain a helpful convention but are not required.

## Borrowed from Ledger-cli

Two features come from Ledger-cli rather than Beancount:

- an **effective date**, written `DATE=EFFECTIVE_DATE` on the transaction
  header;
- a parenthesized **reference** at the end of the header, e.g. `(CB-00123)`.

## Stricter metadata and strings

- Metadata **keys** are strictly lowercase: letters, `_` and `-` only (no
  digits, no uppercase). Beancount is more permissive.
- Metadata **values** are limited to a string, a date or a number — a subset of
  Beancount's richer value types.
- Strings have **no escaping mechanism**: a quoted string cannot contain a
  literal `"`.

## Where to look next

- [Grammar reference](/reference/grammar/) — the authoritative syntax.
- [Directives reference](/reference/directives/) — every directive and rule.
- [Why Pactole](/explanation/why-pactole/) — the motivation behind the smaller
  language.
