---
title: Import an OFX bank statement
description: Import OFX or QFX bank statements into categorized Pactole transactions.
sidebar:
  order: 4
---

Pactole can import bank statements exported in the OFX or QFX format. The
importer supports both OFX 1.x SGML and OFX 2.x XML files, turns each source
transaction into a balanced Pactole transaction, and keeps the source `FITID`
so that repeating an import is safe.

## 1. Prepare an import configuration

Create `~/.config/pactole/import-ofx.toml`. When `XDG_CONFIG_HOME` is set, the
equivalent path is `$XDG_CONFIG_HOME/pactole/import-ofx.toml`.

Start with a minimal configuration:

```toml
bank_account = "Assets:Checking"
currency = "EUR"
default_counter_account = "Expenses:ToReview"
```

- `bank_account` is the Pactole account represented by the bank statement.
- `currency` is used when an OFX transaction does not provide its own currency.
- `default_counter_account` receives transactions that match no rule. It is a
  useful holding account for transactions that need to be categorized later.

## 2. Add categorization rules

Rules select the counter account for an imported transaction. They are evaluated
in order and the **first matching rule wins**:

```toml
[[rules]]
payee_contains = "carrefour"
counter_account = "Expenses:Groceries"
payee = "Carrefour"

[[rules]]
memo_contains = "salary"
direction = "credit"
counter_account = "Income:Salary"

[[rules]]
direction = "debit"
counter_account = "Expenses:ToReview"
```

Available match criteria are:

- `payee_contains`: case-insensitive substring of the source payee or name;
- `memo_contains`: case-insensitive substring of the source memo;
- `trntype`: exact, case-insensitive OFX transaction type such as `DEBIT`,
  `CREDIT`, `POS` or `XFER`;
- `direction`: `debit` for money leaving the imported account, or `credit` for
  money entering it.

Criteria in one rule are combined with `AND`. A rule with no criteria matches
every transaction, so it can be used as a final catch-all. The optional `payee`
field replaces the source payee with a canonical name.

## 3. Preview the import

Pass the statement to `import-ofx`. Without `--into`, generated transactions are
printed to stdout:

```sh
pactole import-ofx statement.ofx
```

Use `--config` when the configuration is somewhere else:

```sh
pactole import-ofx statement.qfx --config ./import-ofx.toml
```

The generated output contains cleared transactions with two postings: the
configured counter account and the imported bank account. For example:

```pactole
2026-09-03 * "Carrefour" "Courses"
  fitid: 2026090300001
  Expenses:Groceries 45.30 EUR
  Assets:Checking -45.30 EUR
```

The importer writes the OFX `FITID` as `fitid` metadata. It also prints a summary
on stderr, for example:

```text
imported 2 transaction(s), skipped 0
```

## 4. Append to an existing journal

Use `--into` to compare source identifiers with an existing `.pactole` file:

```sh
pactole import-ofx statement.ofx --into ledger.pactole
```

Transactions whose `FITID` is already present in `ledger.pactole` are skipped.
This makes it safe to import an overlapping statement more than once.

To append the new transactions directly to the target file, add `--write`:

```sh
pactole import-ofx statement.ofx \
  --config ~/.config/pactole/import-ofx.toml \
  --into ledger.pactole \
  --write
```

`--write` requires `--into`. In write mode, the generated transactions are not
printed to stdout; the target file is updated and the import summary remains on
stderr.

## Validate the result

The importer generates transaction directives only. It does not generate
`open`, `commodity` or `payee` declarations:

```text
generated transactions = a journal fragment, not a complete journal
```

Therefore, make sure the target journal already declares the imported account,
counter accounts, currencies and payees before running:

```sh
pactole check ledger.pactole
```

If a source transaction has no payee and no rule supplies one, the importer uses
the placeholder payee `Unknown` and emits a warning. Declare `payee "Unknown"`
if you want the resulting journal to pass business validation, or add a rule
with a canonical `payee`.

## Troubleshooting

### Configuration not found

Pass an explicit configuration path:

```sh
pactole import-ofx statement.ofx --config ./import-ofx.toml
```

### Invalid account or currency

`bank_account`, `default_counter_account`, rule counter accounts and `currency`
must use Pactole's account and commodity syntax. The command reports an error
before writing anything when one of these values is invalid.

### Transactions are unexpectedly uncategorized

Check rule order and matching fields. Rules are case-insensitive for text
matching, but `payee_contains` and `memo_contains` perform substring matching;
they do not use regular expressions. Add a final catch-all rule or refine the
default counter account for transactions that still need review.

## Related

- [Command-line reference](/reference/cli/) — all `import-ofx` options.
- [Record a transaction](/how-to/record-a-transaction/) — the transaction
  format produced by the importer.
- [Reconcile with balance assertions](/how-to/reconcile-with-balance-assertions/)
  — verify imported balances against your statement.
