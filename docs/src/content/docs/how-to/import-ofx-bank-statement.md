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

The importer reads the `[import]` section of `pactole.toml`. `pactole init`
already creates one next to your ledger; otherwise create a
`pactole.toml` next to the journal you import into (or in the user
configuration directory, `~/.config/pactole/pactole.toml`, for a global
default).

Start with a minimal configuration:

```toml
[import]
bank_account = "Assets:Checking"
currency = "EUR"
default_counter_account = "Expenses:ToReview"
```

- `bank_account` is the Pactole account represented by the bank statement.
  It is the fallback for statement accounts not covered by the
  `[import.accounts]` mapping (see
  [Importing several accounts](#importing-several-accounts)).
- `currency` is used when an OFX transaction does not provide its own currency.
- `default_counter_account` receives transactions that match no rule. It is a
  useful holding account for transactions that need to be categorized later.

## 2. Add categorization rules

Rules select the counter account for an imported transaction. They are evaluated
in order and the **first matching rule wins**:

```toml
[[import.rules]]
payee_contains = "carrefour"
counter_account = "Expenses:Groceries"
payee = "Carrefour"

[[import.rules]]
memo_contains = "salary"
direction = "credit"
counter_account = "Income:Salary"

[[import.rules]]
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

Use the global `--config` flag when the configuration is somewhere else:

```sh
pactole --config ./pactole.toml import-ofx statement.qfx
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
  --into ledger.pactole \
  --write
```

`--write` requires `--into`. In write mode, the generated transactions are not
printed to stdout; the target file is updated and the import summary remains on
stderr.

## Importing several accounts

An OFX export identifies its account by the statement's `ACCTID`. When one
configuration serves several bank accounts, map each `ACCTID` to its Pactole
account in an `[import.accounts]` table:

```toml
[import]
bank_account = "Assets:Checking"

[import.accounts]
"00012345678" = "Assets:Checking"
"00098765432" = "Assets:Savings"
```

Each generated transaction is then posted to the account mapped from the
`ACCTID` of the statement it comes from. A statement account absent from the
table falls back to `bank_account`, with a warning on stderr naming the
unmapped account (reported once per account per import). When no
`[import.accounts]` table is configured at all, `bank_account` is simply the
account for every statement.

To post one statement to a single account without touching the
configuration, pass `--account`:

```sh
pactole import-ofx savings.ofx --account Assets:Savings
```

`--account` overrides both `bank_account` and the `[import.accounts]` mapping
for that run.

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

### No `[import]` section

The command fails when the resolved `pactole.toml` defines no `[import]`
section. Create one (see [step 1](#1-prepare-an-import-configuration)), or
point at an explicit configuration file with the global `--config` flag:

```sh
pactole --config ./pactole.toml import-ofx statement.ofx
```

### Invalid account or currency

`bank_account`, the `[import.accounts]` mapping, `default_counter_account`,
rule counter accounts and `currency` must use Pactole's account and commodity
syntax. The command reports an error before writing anything when one of these
values is invalid.

### Transactions are unexpectedly uncategorized

Check rule order and matching fields. Rules are case-insensitive for text
matching, but `payee_contains` and `memo_contains` perform substring matching;
they do not use regular expressions. Add a final catch-all rule or refine the
default counter account for transactions that still need review.

## Related

- [Configuration reference](/reference/configuration/) — the whole `pactole.toml`
  schema, its layers and the merge rules.
- [Command-line reference](/reference/cli/) — all `import-ofx` options.
- [Record a transaction](/how-to/record-a-transaction/) — the transaction
  format produced by the importer.
- [Reconcile with balance assertions](/how-to/reconcile-with-balance-assertions/)
  — verify imported balances against your statement.
