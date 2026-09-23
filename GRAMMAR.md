# Pactole Grammar

This document describes the syntax of `.pactole` files, as defined by the
tree-sitter grammar in
[`crates/tree-sitter-pactole/grammar.js`](crates/tree-sitter-pactole/grammar.js).
The format is inspired by [Beancount](https://beancount.github.io/docs/)
(and, for a couple of details, by Ledger-cli), with a few simplifications
and additions specific to Pactole.

A `.pactole` file is a sequence of **directives**, each starting at column
0 with a header line, optionally followed by indented metadata lines and
postings.

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

```
source_file := (directive | newline)*
directive   := header newline property* posting*
```

Each directive starts with a header (`open`, `close`, `commodity`,
`balance`, `transaction`, `payee` or `include`), followed by a newline,
then an optional block of metadata (`property`) and — for transactions
only — a sequence of postings.

**Important: a directive's own metadata must come before its postings.**
Once a posting has been seen, any further indented line is parsed as
either a new posting or metadata *belonging to the previous posting*,
never as metadata of the directive itself.

## Directive headers

| Directive     | Syntax                                                           |
|---------------|-------------------------------------------------------------------|
| `open`        | `DATE open ACCOUNT`                                                |
| `close`       | `DATE close ACCOUNT`                                               |
| `commodity`   | `DATE commodity COMMODITY_NAME`                                    |
| `payee`       | `payee "name"`                                                      |
| `balance`     | `DATE balance ACCOUNT NUMBER [~ TOLERANCE] COMMODITY_NAME`         |
| `transaction` | `DATE[=EFFECTIVE_DATE] STATUS "payee" ["narration"] TAGS/LINKS [(reference)]` |
| `include`     | `include "path"`                                                    |

Note `payee` is the only directive with no leading date: unlike an
account or a commodity, a payee has no temporal life cycle (it is never
"opened" or superseded), so declaring it is a simple, order-independent
fact about the journal — see [Payees](#payees) below.

Examples:

```pactole
2026-09-03 open Assets:Checking
2026-12-31 close Assets:Checking
2026-01-01 commodity EUR
payee "Whole Foods"
2026-09-03 balance Assets:Checking 1234.56 ~ 0.01 EUR
include "ledger/2026.pactole"
```


### Transactions

```
transaction := date ("=" effective_date)? status payee narration?
               (tag | link)*
               reference?
```

- `date`: the transaction's date (see [Dates](#dates)).
- `=effective_date` (optional): a second, "effective" date, borrowed from
  Ledger-cli. Not part of standard Beancount.
- `status`: one of three flags, `*` (cleared), `!` (pending) or `?`
  (uncleared). Unlike Beancount, no custom flag (arbitrary uppercase
  letter) is allowed, and there is no `txn` keyword.
- `payee`: a quoted string, **mandatory** (unlike Beancount, where this
  field is optional and a single string is treated as the narration
  instead).
- `narration` (optional): a second quoted string.
- `tag`/`link`: zero or more, **freely interleaved** (`#tag1 ^link1 #tag2
  ^link2`, or any other order).
- `reference` (optional): free text between parentheses, e.g.
  `(CB-00123)` — borrowed from Ledger-cli's transaction "code", not part
  of Beancount.

```pactole
2026-09-03=2026-09-05 * "Whole Foods" "Weekly groceries" #food ^reimbursement (CB-00123)
```

Additional examples showing the freedom in tags/links ordering and optional
fields:

```pactole
2026-09-10 ! "Amazon"
2026-09-11 * "Employer" "September salary" #income
2026-09-12 * "Landlord" "Rent" #housing ^lease-2026 ^auto-pay #recurring (REF-042)
```

### Postings

Each indented line under a transaction that starts with an account name is
a posting:

```
posting := account amount? property*
```

The amount is optional: at most one posting per transaction may omit it
(interpolation), although this is not yet enforced at the grammar level.

```pactole
  Expenses:Groceries 45.30 EUR
  Assets:Checking
```

There is no cost or price annotation (`{cost}`, `@ price`, `@@ total`):
these Beancount features (dedicated to investment tracking) are not
supported.

## Metadata (`property`)

```
property := key ":" value
```

- `key`: lowercase letters, `_` or `-` only (`[a-z_\-]+`) — no digits and
  no uppercase letters after the first character, unlike Beancount, which
  is more permissive.
- `value`: a string, a date, or a number — a subset of Beancount's richer
  value types (which also accepts accounts, currencies, tags, booleans,
  `None`, etc.).

Metadata can be attached either to a directive (right after the header,
before any posting), or to a specific posting (right after its
account/amount line):

```pactole
2026-09-03 open Assets:Checking
  description: "Our joint checking account"    ; directive-level metadata

2026-09-03 * "Whole Foods"
  Expenses:Groceries 45.30 EUR
    total_paid: 45.30               ; posting-level metadata
```

## Accounts

```
account := [A-Z][A-Za-z0-9_-]* (":" [A-Za-z0-9_-]+)*
```

An account is a sequence of `:`-separated segments. The first segment
must start with an ASCII uppercase letter (so it can never be confused
with a metadata key, which is always lowercase). Unlike Beancount, **no
fixed root category** (`Assets`, `Liabilities`, `Equity`, `Income`,
`Expenses`) is enforced: any capitalized name works, e.g.
`Assets:Checking` or `Expenses:Groceries:Restaurants`.

## Commodities / currencies

```
commodity_name := [A-Z][A-Z0-9'._-]{0,22}[A-Z0-9]
```

A 2 to 24 character code, starting with an uppercase letter, ending with
an uppercase letter or digit, and made of uppercase letters, digits, `'`,
`.`, `_` or `-` in between — this is exactly the rule Beancount uses for
its currency/commodity codes (e.g. `EUR`, `BTC.SAT-2`, `BRK'A`).

```pactole
2026-01-01 commodity EUR
2026-01-01 commodity USD
2026-01-01 commodity BRK'A
2026-01-01 commodity BTC.SAT-2
```

## Payees

```
payee := "payee" string
```

Declares a payee as "known", borrowed from Ledger-cli's `payee`
directive. A transaction's payee must match one of these declarations
*somewhere* in the journal — this catches typos and inconsistent
naming (e.g. `"Whole Foods"` vs. `"Wholefoods"`) the same way `open`
catches a reference to an unknown account.

Unlike every other directive, `payee` carries **no date**: a payee has
no temporal life cycle (it is never "opened", closed or superseded), so
only its presence in the file matters, not its position relative to the
transactions using it — it may even be declared *after* its first use.

```pactole
payee "Whole Foods"
payee "Landlord"

2026-09-03 * "Whole Foods" "Weekly groceries"
  Expenses:Groceries 45.30 EUR
  Assets:Checking -45.30 EUR
```

## Numbers

```
number := "-"? [0-9]+ ("," [0-9]{3})* ("." [0-9]+)?
```

Numbers accept an optional minus sign, an optional decimal part, and —
like Beancount — thousands separators `,` every 3 digits (e.g.
`1,234,567.89`). They are then normalized (commas stripped) by the Rust
builder before being converted to a `Decimal`.

```pactole
2026-09-03 balance Assets:Checking 1,234,567.89 EUR
2026-09-04 * "Bank" "Transfer"
  Assets:Checking   -1,000.00 EUR
  Assets:Savings     1,000.00 EUR
```

A `balance` assertion's `tolerance` (after the `~`) follows a simpler
format, with no sign and no thousands separator:
`[0-9]+("."[0-9]+)?`.

## Dates

```
date := [0-9]{4} "-" [0-9]{2} "-" [0-9]{2}
```

ISO format `YYYY-MM-DD` only.

## Strings

```
string := '"' [^"\r\n]* '"'
```

A double-quoted string, with no escaping mechanism: it cannot contain a
literal quote character (a difference from Beancount, which supports
escaping).

## Tags, links and reference

```
tag       := "#" [A-Za-z0-9_-]+
link      := "^" [A-Za-z0-9_-]+
reference := "(" [^)\r\n]* ")"
```

```pactole
2026-09-03 * "Landlord" "Rent" #housing ^lease-2026 (REF-042)
```

## Comments

```
comment := ";" [^\r\n]*
```

A comment starts with `;` and extends to the end of the line. It may
appear anywhere (declared as `extras` in the grammar).

## Whitespace and indentation

All separators (between the date and `open`, between a key and its value,
before an amount, at the start of a posting or metadata line, etc.)
accept **one or more spaces or tabs** (`[ \t]+`). This allows amounts or
indentation to be freely aligned, as is common practice in Beancount
files:

```pactole
2026-09-03  *  "Whole Foods"   "Weekly groceries"
  Expenses:Groceries    45.30 EUR
  Assets:Checking      -45.30 EUR
```

## Main differences from Beancount

For reference, a summary of the deliberate deviations from Beancount:

- Only 7 directives are supported (`open`, `close`, `commodity`,
  `payee`, `balance`, `transaction`, `include`); `pad`, `price`, `note`,
  `event`, `document`, `query`, `custom`, `option`, `plugin`,
  `pushtag`/`poptag` do not exist.
- `payee` is mandatory in a transaction; `narration` is optional (the
  reverse of the Beancount convention). A `payee` *declaration* (this
  file's `payee "name"` directive, borrowed from Ledger-cli) does not
  exist in Beancount at all.
- Status flags limited to `*`, `!`, `?` (no custom flags, no `txn`
  keyword).
- No cost/price annotations on postings (`{cost}`, `@`, `@@`).
- No enforced account root categories.
- Added an effective date (`date=effective_date`) and a parenthesized
  reference (`(ref)`), borrowed from Ledger-cli.
- Metadata values limited to string/date/number; keys are strictly
  lowercase (no digits).
- No escaping inside strings.

## Implementation

- The tree-sitter grammar: [`crates/tree-sitter-pactole/grammar.js`](crates/tree-sitter-pactole/grammar.js).
- Grammar tests (expected trees): [`crates/tree-sitter-pactole/test/corpus/test.txt`](crates/tree-sitter-pactole/test/corpus/test.txt).
- Building Rust objects (`pactole_core::Entry` and friends) from the
  syntax tree: [`crates/pactole-storage-fs/src/parser.rs`](crates/pactole-storage-fs/src/parser.rs).
- To explore the tree produced for a given file:
  ```sh
  cargo run -- parse my-file.pactole
  ```
