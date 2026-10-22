---
title: Configuration
description: The pactole.toml schema, its layers, and the options of each section.
sidebar:
  order: 3
---

Every setting of Pactole lives in a single file, `pactole.toml`. The CLI and
the language server read the same schema, so a rule configured once applies
to both.

## Where the file lives

Pactole reads up to three layers, from the lowest precedence to the highest:

1. **The global layer** — `$XDG_CONFIG_HOME/pactole/pactole.toml` when
   `XDG_CONFIG_HOME` is set and non-empty, else `%APPDATA%\pactole\pactole.toml`
   on Windows, else `~/.config/pactole/pactole.toml`.
2. **The project layer** — a `pactole.toml` next to the journal being
   processed. For `pactole import-ofx`, it is the file given by `--into`
   instead.
3. **In-memory overrides** — the CLI's root flags and the language server's
   `initializationOptions`.

The global `--config <path>` flag replaces the discovery with that single
file: the global and the project layers are then not read at all.

:::caution
Pactole does **not** walk up the directory tree looking for a `pactole.toml`.
Only the two locations above are considered, so a configuration file cannot
silently come from an unexpected parent directory.
:::

## How the layers merge

The merge is per key, and it is intentionally boring:

- a scalar key defined by a higher layer replaces the lower one;
- a table merges entry by entry: `[import.accounts]` from the global layer
  and `[import.accounts]` from the project layer are **unioned**, the project
  winning on a duplicated account identifier;
- `[[import.rules]]` is replaced **as a whole block** by the highest layer
  that defines it — rules are never merged element-wise, because a rule is
  only meaningful as a whole.

Unknown keys are rejected per section, so a typo fails loudly instead of
being silently ignored:

```toml
[validation]
payee = "warn"
payye = "warn"   # error: unknown key `payye`
```

## `[validation]`

The severity of the business checks. Each key accepts three levels:

| Level | Meaning |
|---|---|
| `off` | The check is never reported. |
| `warn` | The issue is reported as a warning; it does not fail `pactole check` unless `--strict` is passed. |
| `error` | The issue is reported as an error and fails `pactole check`. |

| Key | Default | Controls |
|---|---|---|
| `payee` | `error` | A transaction whose payee has no `payee` declaration (`payee-not-declared`). |
| `unused` | `warn` | A declaration nothing ever uses: `account-unused`, `commodity-unused` and `payee-unused`. |

```toml
[validation]
payee = "warn"
unused = "off"
```

Every other business check — account lifecycles, declared commodities,
balancing, balance assertions — is always an error.

See [Diagnostics and project analysis](/reference/diagnostics/) for the full
list of codes.

## `[format]`

The options of the canonical formatter.

| Key | Default | Meaning |
|---|---|---|
| `posting_width` | `50` | The column at which posting amounts are aligned. |

```toml
[format]
posting_width = 60
```

See [Format a ledger](/how-to/format-a-ledger/).

## `[declaration]`

The files an editor front-end should insert new declarations into, instead of
the document being edited.

| Key | Receives |
|---|---|
| `payees` | New `payee "..."` declarations. |
| `accounts` | New `DATE open ACCOUNT` declarations. |
| `commodities` | New `DATE commodity COMMODITY` declarations. |

```toml
[declaration]
payees = "payees.pactole"
accounts = "accounts.pactole"
commodities = "commodities.pactole"
```

A relative path is resolved against the directory of the journal, exactly like
an `include`. The language server only uses a declaration file when it is
actually reachable from the root journal through an `include`; otherwise the
declaration is inserted into the current document. The server never creates
the file and never adds the `include` itself.

This section only affects the language server: the CLI never writes to a
ledger.

## `[import]`

The settings of the bank statement import (`pactole import-ofx`). The command
fails with a clear message when no layer defines this section.

| Key | Meaning |
|---|---|
| `bank_account` | The Pactole account the statement is imported into (e.g. `Assets:Bank`). Required. |
| `currency` | The fallback commodity, used when a source transaction carries no currency of its own. |
| `default_counter_account` | The counter account of the transactions that no rule matches (e.g. `Expenses:To-Be-Classified`). Required. |
| `accounts` | A table mapping a source account identifier (for OFX, the statement's `ACCTID`) to a Pactole account. |

```toml
[import]
bank_account = "Assets:Checking"
currency = "EUR"
default_counter_account = "Expenses:ToBeClassified"

[import.accounts]
"00012345678" = "Assets:Checking"
```

A transaction whose source account is absent from `accounts` is posted to
`bank_account`, with a warning when the table exists but does not cover it.

### `[[import.rules]]`

Rules categorize the imported transactions. They are evaluated in order and
**the first match wins**; a transaction matched by no rule uses
`default_counter_account`.

| Key | Meaning |
|---|---|
| `counter_account` | The Pactole account to post the opposite side to. Required. |
| `payee_contains` | Matches, case-insensitively, a substring of the source payee or name. |
| `memo_contains` | Matches, case-insensitively, a substring of the source memo. |
| `trntype` | Matches the source transaction type exactly, case-insensitively (e.g. `DEBIT`, `POS`, `XFER`). |
| `direction` | Matches the direction of the movement: `debit` (money leaving the account) or `credit` (money entering it). |
| `payee` | The canonical payee to use instead of the raw source payee. |

All the criteria but `counter_account` are optional, and a rule with no
criterion at all matches every transaction — useful as a catch-all placed
after the more specific rules.

```toml
[[import.rules]]
payee_contains = "carrefour"
counter_account = "Expenses:Groceries"
payee = "Carrefour"

[[import.rules]]
trntype = "POS"
counter_account = "Expenses:Groceries"

[[import.rules]]
counter_account = "Expenses:ToBeClassified"
```

See [Import an OFX bank statement](/how-to/import-ofx-bank-statement/).

## A complete example

```toml
# The global layer: ~/.config/pactole/pactole.toml
[validation]
payee = "warn"
unused = "warn"

[format]
posting_width = 50

[import]
bank_account = "Assets:Checking"
currency = "EUR"
default_counter_account = "Expenses:ToBeClassified"
```

```toml
# The project layer: ./pactole.toml, next to the journal
[validation]
unused = "off"

[declaration]
payees = "payees.pactole"
accounts = "accounts.pactole"
```

The resolved configuration is the global one, overridden by the project one:
the payee check is a warning, the unused-declaration checks are off, and the
declaration files are the ones of the project layer.

## The language server's own options

The language server reads the same files as the CLI, but it also accepts
overrides in the `initializationOptions` of the `initialize` request:

| Option | Meaning |
|---|---|
| `journal_file` | The path to the root `.pactole` journal, resolved against the workspace root. |
| `validation` | A single level (`"off"`, `"warn"` or `"error"`) applied to every configurable check, or a table (`{ "payee": ..., "unused": ... }`) setting them one by one. |

```json
{
  "journal_file": "main.pactole",
  "validation": { "payee": "warn" }
}
```

See [Use the Pactole language server](/how-to/use-the-lsp/).

## Related

- [Command-line interface](/reference/cli/) — the commands that read this configuration.
- [Diagnostics and project analysis](/reference/diagnostics/) — what each severity means.
