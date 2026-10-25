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

Only eight directives exist:

`open`, `close`, `commodity`, `price`, `payee`, `balance`, a transaction,
and `include`.

Beancount directives such as `pad`, `note`, `event`, `document`,
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

Only `*` (cleared), `!` (pending) and `?` (uncleared) are accepted. There is no
custom flag letter and no `txn` keyword.

The **default status differs from Beancount's**. Beancount defaults a
transaction to `*` (cleared); Pactole defaults it to `?` (uncleared). A
Beancount journal whose transactions carry no flag therefore lands in Pactole
as a journal full of uncleared transactions — which is the safe direction, but
it changes what the register report shows as reconciled.

## Commodities have no label

Beancount's `commodity` directive accepts an optional human-readable label:

```beancount
1970-01-01 commodity USD "US Dollar"
```

Pactole's `commodity` directive takes the commodity code and nothing else:

```pactole
1970-01-01 commodity USD
```

The quoted label is a syntax error in Pactole, so it has to be removed when
migrating. A label can be kept as a comment (`; US Dollar` — Pactole comments
start with a semicolon) or as free-form metadata on the directive:

```pactole
1970-01-01 commodity USD ; US Dollar
1970-01-01 commodity GBP
  label: "Pound Sterling"
```

## Accounts have no special metadata

Beancount attaches booking metadata to an `open` directive:

```beancount
2015-04-01 open Assets:Checking USD "Chase Manhattan Bank" {
  booking: Some,
  booking_date: "2015-04-01",
  institution: "Chase",
}
```

Pactole's `open` directive takes the account and, optionally, a
comma-separated list of commodities:

```pactole
2015-04-01 open Assets:Checking EUR,USD
```

The `booking`, `booking_date` and `institution` keys are not special in
Pactole: there is no booking algorithm to configure, so they are only kept as
ordinary metadata if you write them yourself.

## No transaction-cost annotations or lot tracking

Pactole supports the dated `price` directive for market quotes, but postings do
not yet accept `@`/`@@` transaction-cost annotations or `{}` lot-cost
annotations. Transaction costs are planned separately from market prices;
tracking acquisition lots and cost basis is out of scope.

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

## Migrating a Beancount journal

Two differences are easy to miss: the syntax stays valid, so nothing fails
until the business checks run.

### `payee:` and `narration:` become ordinary metadata

In Beancount, `payee` and `narration` are special metadata keys: they carry
the payee and the narration of a transaction whose header holds a single
string. Pactole has no such special keys — `payee` and `narration` are just
free-form metadata. A migrated transaction like this:

```beancount
2026-09-03 * "Weekly groceries"
  payee: "Whole Foods"
  narration: "Weekly groceries"
```

parses in Pactole, but the two lines are just metadata: the transaction's
payee is the header string, not `Whole Foods`, and the narration is empty.
The right translation is:

```pactole
2026-09-03 * "Whole Foods" "Weekly groceries"
```

A migrated journal often trips the `payee-not-declared` check on every
transaction for the same reason. Declaring the payees, or setting
`[validation] payee = "warn"` while migrating, makes the remaining work
visible without blocking it.

### Keys that only differ in case

Beancount accepts metadata keys such as `Invoice-Id` or `PDF-URL`; Pactole
accepts only lowercase letters, `_` and `-`. `invoice_id` and `invoice-id`
are fine, `invoiceId` and `invoice2` are syntax errors.

## Where to look next

- [Grammar reference](/reference/grammar/) — the authoritative syntax.
- [Directives reference](/reference/directives/) — every directive and rule.
- [Why Pactole](/explanation/why-pactole/) — the motivation behind the smaller
  language.
