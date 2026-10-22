---
title: Grammar
description: The syntax of .pactole files, as defined by the tree-sitter grammar.
sidebar:
  order: 1
---

This page describes the syntax of `.pactole` files, as defined by the
tree-sitter grammar in `crates/tree-sitter-pactole/grammar.js`. The format is
inspired by [Beancount](https://beancount.github.io/docs/) (and, for a couple of
details, by Ledger-cli), with a few simplifications and additions specific to
Pactole.

A `.pactole` file is a sequence of **directives**, each starting at column 0
with a header line, optionally followed by indented metadata lines and postings.

```pactole
2026-09-03 open Assets:Checking
  description: "Our joint checking account"
  opened_on: 2026-09-03

2026-01-01 commodity EUR

2026-09-03=2026-09-05 * "Whole Foods" "Weekly groceries" #food ^reimbursement (CB-00123)
  note: "scanned receipt"
  Expenses:Groceries 45.30 EUR
    total_paid: 45.30
  Assets:Checking -45.30 EUR
```

## Overall structure

```text
source_file := (directive | newline)*
directive   := header newline property* posting*
```

Each directive starts with a header (`open`, `close`, `commodity`, `balance`,
`transaction`, `payee` or `include`), followed by a newline, then an optional
block of metadata (`property`) and — for transactions only — a sequence of
postings. Postings may only appear under a `transaction` directive.

:::caution
A directive's own metadata must come **before** its postings. Once a posting has
been seen, any further indented line is parsed as either a new posting or
metadata *belonging to the previous posting*, never as metadata of the directive
itself.
:::

## Directive headers

| Directive     | Syntax                                                           |
|---------------|------------------------------------------------------------------|
| `open`        | `DATE open ACCOUNT [COMMODITY_NAME[,COMMODITY_NAME...]]`          |
| `close`       | `DATE close ACCOUNT`                                              |
| `commodity`   | `DATE commodity COMMODITY_NAME`                                   |
| `payee`       | `payee "name"`                                                    |
| `balance`     | `DATE balance ACCOUNT NUMBER [~ TOLERANCE] COMMODITY_NAME`        |
| `transaction` | `DATE[=EFFECTIVE_DATE] STATUS? "payee" ["narration"] TAGS/LINKS [(reference)]` |
| `include`     | `include "path"`                                                  |

`payee` is the only directive with no leading date: a payee has no temporal life
cycle (it is never "opened" or superseded), so declaring it is a simple,
order-independent fact about the journal.

```pactole
2026-09-03 open Assets:Checking
2026-09-03 open Assets:Broker EUR,USD
2026-12-31 close Assets:Checking
2026-01-01 commodity EUR
payee "Whole Foods"
2026-09-03 balance Assets:Checking 1234.56 ~ 0.01 EUR
include "ledger/2026.pactole"
```

`open` accepts an optional, comma-separated list of commodities (no spaces
around the commas), e.g. `EUR,USD`. When the list is non-empty, it restricts the
account: any posting on it using a commodity outside the list is rejected during
business validation. An empty list (the default) leaves the account
unrestricted.

### Transactions

```text
transaction := date ("=" effective_date)? status? payee narration?
               (tag | link)*
               reference?
```

- `date`: the transaction's date (see [Dates](#dates)).
- `=effective_date` (optional): a second, "effective" date, borrowed from
  Ledger-cli. Not part of standard Beancount.
- `status`: optionally one of `*` (cleared), `!` (pending) or `?` (uncleared).
  When omitted, the transaction is considered uncleared. No custom flag and no
  `txn` keyword.
- `payee`: a quoted string, **mandatory**.
- `narration` (optional): a second quoted string.
- `tag`/`link`: zero or more, **freely interleaved**.
- `reference` (optional): free text between parentheses, e.g. `(CB-00123)`.

```pactole
2026-09-03=2026-09-05 * "Whole Foods" "Weekly groceries" #food ^reimbursement (CB-00123)
2026-09-10 ! "Amazon"
2026-09-11 * "Employer" "September salary" #income
2026-09-12 * "Landlord" "Rent" #housing ^lease-2026 ^auto-pay #recurring (REF-042)
```

### Postings

```text
posting := account amount? property*
```

The amount is optional: at most one posting per transaction may omit it
(interpolation). There is no cost or price annotation (`{cost}`, `@ price`,
`@@ total`); these Beancount features, dedicated to investment tracking, are not
supported.

```pactole
  Expenses:Groceries 45.30 EUR
  Assets:Checking
```

## Metadata (`property`)

```text
property := key ":" value
```

- `key`: lowercase letters, `_` or `-` only (`[a-z_\-]+`).
- `value`: a string, a date, or a number.

Metadata can be attached either to a directive (right after the header, before
any posting) or to a specific posting (right after its line):

```pactole
2026-09-03 open Assets:Checking
  description: "Our joint checking account"    ; directive-level metadata

2026-09-03 * "Whole Foods"
  Expenses:Groceries 45.30 EUR
    total_paid: 45.30               ; posting-level metadata
```

## Accounts

```text
account := [A-Z][A-Za-z0-9_-]* (":" [A-Za-z0-9_-]+)*
```

An account is a sequence of `:`-separated segments. The first segment must start
with an ASCII uppercase letter. Unlike Beancount, **no fixed root category**
(`Assets`, `Liabilities`, `Equity`, `Income`, `Expenses`) is enforced: any
capitalized name works, e.g. `Assets:Checking` or `Expenses:Groceries:Restaurants`.

## Commodities / currencies

```text
commodity_name := [A-Z][A-Z0-9'._-]{0,22}[A-Z0-9]
```

A 2 to 24 character code, starting with an uppercase letter, ending with an
uppercase letter or digit, and made of uppercase letters, digits, `'`, `.`, `_`
or `-` in between — exactly the rule Beancount uses for its currency codes (e.g.
`EUR`, `BTC.SAT-2`, `BRK'A`).

```pactole
2026-01-01 commodity EUR
2026-01-01 commodity USD
2026-01-01 commodity BRK'A
2026-01-01 commodity BTC.SAT-2
```

## Payees

```text
payee := "payee" string
```

Declares a payee as "known". A transaction's payee must match one of these
declarations somewhere in the journal — this catches typos and inconsistent
naming (e.g. `"Whole Foods"` vs. `"Wholefoods"`), the same way `open` catches a
reference to an unknown account.

Unlike every other directive, `payee` carries **no date**: only its presence in
the file matters, not its position relative to the transactions using it — it
may even be declared *after* its first use.

```pactole
payee "Whole Foods"
payee "Landlord"
```

## Numbers

```text
number := "-"? [0-9]+ ("," [0-9]{3})* ("." [0-9]+)?
```

Numbers accept an optional minus sign, an optional decimal part, and — like
Beancount — thousands separators `,` every 3 digits (e.g. `1,234,567.89`). They
are normalized (commas stripped) before being converted to a decimal.

```pactole
2026-09-03 balance Assets:Checking 1,234,567.89 EUR
```

A `balance` assertion's `tolerance` (after the `~`) follows a simpler format,
with no sign and no thousands separator: `[0-9]+("."[0-9]+)?`.

## Dates

```text
date := [0-9]{4} "-" [0-9]{2} "-" [0-9]{2}
```

ISO format `YYYY-MM-DD` only.

## Strings

```text
string := '"' [^"\r\n]* '"'
```

A double-quoted string with no escaping mechanism: it cannot contain a literal
quote character.

## Tags, links and reference

```text
tag       := "#" [A-Za-z0-9_-]+
link      := "^" [A-Za-z0-9_-]+
reference := "(" [^)\r\n]* ")"
```

## Comments

```text
comment := ";" [^\r\n]*
```

A comment starts with `;` and extends to the end of the line. Comments are only
recognized at specific positions:

- a standalone line, anywhere between two directives or between two of a
  directive's `property`/`posting` lines;
- right after a directive header, before its newline;
- right after a `property` line's value, before its newline;
- right after a posting's `account [amount]`, before its newline.

:::note
A standalone comment line right after a posting is syntactically
indistinguishable from that posting's own trailing metadata, so it is parsed —
and reformatted by `pactole fmt` — as trailing content of the *preceding*
posting.
:::

## Whitespace and indentation

All separators accept **one or more spaces or tabs** (`[ \t]+`), which allows
amounts and indentation to be freely aligned:

```pactole
2026-09-03  *  "Whole Foods"   "Weekly groceries"
  Expenses:Groceries    45.30 EUR
  Assets:Checking      -45.30 EUR
```

## Main differences from Beancount

- Only 7 directives are supported; `pad`, `price`, `note`, `event`, `document`,
  `query`, `custom`, `option`, `plugin`, `pushtag`/`poptag` do not exist.
- `payee` is mandatory in a transaction and `narration` is optional (the reverse
  of the Beancount convention). A `payee` *declaration* does not exist in
  Beancount at all.
- Status flags are limited to `*`, `!`, `?` (no custom flags, no `txn` keyword);
  the status may be omitted and defaults to `?` (uncleared).
- No cost/price annotations on postings (`{cost}`, `@`, `@@`).
- No enforced account root categories.
- Added an effective date (`date=effective_date`) and a parenthesized reference
  (`(ref)`), borrowed from Ledger-cli.
- Metadata values limited to string/date/number; keys are strictly lowercase.
- No escaping inside strings.

## Implementation

- The tree-sitter grammar: `crates/tree-sitter-pactole/grammar.js`.
- Grammar tests (expected trees):
  `crates/tree-sitter-pactole/test/corpus/`.
- Building Rust objects from the syntax tree:
  `crates/pactole-storage-fs/src/parser.rs`.
