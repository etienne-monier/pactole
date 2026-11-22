---
title: Migrate from Actual Budget
description: Create a Pactole journal from an Actual Budget SQLite database.
sidebar:
  order: 5
---

`pactole import-actual` performs a one-time migration from an Actual Budget
SQLite database. It creates a new journal and does not use or modify the
project's `pactole.toml`.

## Run the migration

The currency is required. Without `--output`, the generated journal is written
to standard output:

```sh
pactole import-actual actual.sqlite --currency EUR
```

Write the result to a file instead:

```sh
pactole import-actual actual.sqlite \
  --currency EUR --output ledger.pactole
```

## Configure opening balances

The migration creates an explicit opening transaction. Customize its date or
counter-account when needed:

```sh
pactole import-actual actual.sqlite \
  --currency EUR \
  --opening-date 2026-01-01 \
  --opening-equity Equity:Opening \
  --output ledger.pactole
```

The defaults are `1970-01-01` and `Equity:Opening`.

## Preserve the original payee

Actual descriptions and payees can differ from the normalized payee used by
the generated transaction. Preserve the original value under a metadata key:

```sh
pactole import-actual actual.sqlite \
  --currency EUR \
  --payee-metadata-key imported-payee \
  --output ledger.pactole
```

## What is generated

The migration creates:

- account declarations;
- payee declarations;
- a commodity declaration;
- an explicit opening transaction;
- multi-posting transactions for Actual splits;
- balanced double-entry transactions for transfers.

Actual rules using `contains`, regular-expression `matches` and `oneOf` are
applied to imported descriptions.

## Check the result

After writing the journal, format and validate it:

```sh
pactole fmt --write ledger.pactole
pactole check ledger.pactole
pactole balance ledger.pactole
```

The command is a migration tool, not an incremental synchronizer. It does not
deduplicate against an existing Pactole journal and does not append to one. For
repeated bank-statement imports, use
[the OFX importer](/how-to/import-ofx-bank-statement/) instead.
