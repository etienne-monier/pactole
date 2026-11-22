---
title: Concepts and glossary
description: The accounting and Pactole vocabulary used throughout the documentation.
sidebar:
  order: 2
---

This page defines the words used in Pactole's documentation. Start here when a
reference page assumes a concept you have not encountered before.

## Journal concepts

### Journal

The journal is the complete set of entries that records an accounting history.
It can be stored in one `.pactole` file or assembled from several files with
[`include`](/reference/directives/#include).

### Entry

An entry is one top-level directive in the journal: for example an account
declaration, a commodity declaration, a balance assertion or a transaction.

### Directive

A directive is a top-level construct in a `.pactole` file. Pactole supports
directives such as `open`, `close`, `commodity`, `price`, `payee`, `balance`,
`include`, `pushtag`, `poptag` and transactions.

### Declaration

A declaration introduces a name that the journal can use. `open` declares an
account's lifecycle, `commodity` declares a commodity, and `payee` declares a
known payee.

### Transaction

A transaction describes a financial event. It has a date, an optional status,
a payee, an optional narration and two or more postings. Tags, links and
metadata can provide additional context.

### Posting

A posting is one line inside a transaction. It associates an account with an
amount and a commodity:

```pactole
  Expenses:Groceries 45.30 EUR
```

The posting weights in a transaction must balance to zero per commodity.

### Account

An account is a named category whose balance changes over time. Colons express
hierarchy, so `Expenses:Food:Groceries` is a child of `Expenses:Food`.

Accounts have a lifecycle: they must be opened before use and cannot be used
after they are closed.

### Commodity

A commodity is the unit attached to an amount, such as `EUR`, `USD` or `BTC`.
The term is broader than *currency* because a commodity can also represent an
asset or another traded unit.

### Amount

An amount is a precise numeric value paired with a commodity:

```text
45.30 EUR
```

Pactole preserves decimal amounts exactly rather than silently rounding them.

### Payee

The payee is the person or organization associated with a transaction. A
transaction's payee is normally declared with a `payee` directive so that
names remain consistent throughout the journal.

### Metadata

Metadata are key-value properties attached to an entry or posting. Importers
use metadata to preserve source identifiers such as an OFX `FITID`.

### Tag and link

A **tag** labels a transaction, for example `#vacation`. A **link** associates
it with an external reference, for example `^invoice-2026-042`. Tags are useful
for grouping reports; links are useful for tracing related entries.

## Accounting concepts

### Double-entry accounting

Every transaction records a balanced movement between accounts. Pactole checks
that the transaction's posting weights sum to zero for each commodity. This
turns many data-entry mistakes into explicit diagnostics.

### Balance

An account balance is the cumulative sum of its postings, kept separately for
each commodity. A balance report shows those balances at a reference date.

### Balance assertion

A balance assertion checks that an account has an expected balance on a date:

```pactole
2026-03-31 balance Assets:Checking 1250.00 EUR
```

Assertions are checks on the journal; they do not create a posting.

### Equity

Equity represents the owner's or project's residual interest. An opening
balance is commonly offset against an account such as `Equity:Opening`.

### Income and expenses

Income and expense accounts record flows during a period. The balance sheet
shows their net result as part of equity, while the income statement presents
income and expenses as period activity.

## Prices, costs and reports

### Transaction cost

An `@` or `@@` annotation supplies a transaction-specific conversion weight
used to balance that transaction. It does not change the recorded amount in an
account.

### Market price

A `price` directive records a dated market quote, for example:

```pactole
2026-01-02 price USD 0.92 EUR
```

Market prices are used for report valuation. They are separate from
transaction costs and are not substituted into transaction balancing.

### Valuation

Valuation converts report amounts into a requested target commodity. Pactole
selects the latest direct price on or before the valuation date. It does not
infer inverse or multi-step conversions, and a missing quote leaves the amount
in its original commodity.

### Report

A report is a calculated view of a validated journal. Pactole provides a
register, balance sheet, income statement and expense breakdown. Reports do
not modify the journal.

### Period and reference date

A **period** is an inclusive range used for flow reports such as income and
expenses. A **reference date** is the cutoff used for stock reports such as a
balance sheet. Valuation date and report cutoff can be different.

## Project and tooling concepts

### Include

An `include` adds another `.pactole` file to the project. Relative paths are
resolved from the including file. Pactole detects missing files, cycles and
files reached more than once, while preserving source locations in diagnostics.

### Validation

Validation has several layers:

1. syntax parsing;
2. lowering the syntax into domain entries;
3. project and include analysis;
4. business validation, such as account lifecycles and balancing.

`pactole check` brings these layers together.

### Partial analysis

If one file or directive cannot be analyzed, Pactole keeps analyzing the rest
of the project. Checks that depend on the missing content are suspended, while
independent checks still produce diagnostics. The `analysis-incomplete`
diagnostic describes this uncertainty.

### Canonical formatting

Canonical formatting gives equivalent source files one stable layout. It keeps
Git diffs focused on meaningful changes. `pactole fmt` formats source; it does
not replace `pactole check`.

### `pactole.toml`

The project configuration controls validation, formatting, report account
classification, declarations for editor actions and import rules. Configuration
can be layered from the user directory, the project and explicit overrides.

### Language server

`pactole-lsp` is a Language Server Protocol implementation. It connects a
supported editor to Pactole's parser and project analysis for diagnostics,
completion, navigation, rename, code actions and formatting.

### Web interface

The web interface is a local, read-only frontend served by `pactole serve`. It
uses the same application reports as the CLI and does not require Node.js at
runtime.

## Related pages

- [Getting started](/tutorials/getting-started/)
- [Grammar](/reference/grammar/)
- [Directives](/reference/directives/)
- [Diagnostics and project analysis](/reference/diagnostics/)
- [Why Pactole](/explanation/why-pactole/)
