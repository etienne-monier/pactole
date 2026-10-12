---
title: Split a journal with includes
description: Keep a large ledger manageable by spreading it across several files.
sidebar:
  order: 3
---

When a journal grows, keeping every account declaration, payee and transaction
in a single file becomes unwieldy. The `include` directive lets you split it
across several `.pactole` files while `pactole` still sees one journal.

## Basic usage

An `include` directive takes a quoted path:

```pactole
include "accounts.pactole"
include "ledger/2026.pactole"
```

Pactole reads the included file and inlines its entries **in place** of the
`include` directive, recursively (an included file may itself include others).

## How paths are resolved

A relative path is resolved relative to the directory of the file that contains
the `include` directive. In this example, `2026.pactole` must sit next to the
root file:

```text
/journal.pactole          -> include "2026.pactole"
/journal/2026.pactole     -> include "accounts/checking.pactole"
/journal/accounts/checking.pactole
```

:::note
Includes are resolved without a sandbox: any path the process can read will be
read. Pointing an `include` at a file outside your journal tree is allowed but
not recommended.
:::

## A typical layout

```text
ledger.pactole
commodities.pactole
accounts.pactole
payees.pactole
2026/
  transactions.pactole
```

`ledger.pactole` ties everything together:

```pactole
include "commodities.pactole"
include "accounts.pactole"
include "payees.pactole"
include "2026/transactions.pactole"
```

Declarations and transactions can live in any order across files, since
`pactole check` evaluates the whole flattened journal:

```sh
pactole check ledger.pactole
```

## Related

- [Grammar reference](/reference/grammar/) — where `include` fits in the grammar.
- [Directives reference](/reference/directives/) — the exact `include` syntax.
