<div align="center">
  <img src="logo.png" alt="Pactole logo" width="180">
  <h1>Pactole</h1>
</div>

Pactole is a plain-text ledger tool written in Rust: you record your
transactions as human-readable, git-friendly `.pactole` files (in the
style of [Beancount](https://beancount.github.io/docs/) or Ledger-cli), and
Pactole parses, checks and formats them for you.

Its name comes from the **Pactole**, the ancient name of a small river
in Lydia (Asia Minor) whose sands were said to carry gold washed down
from Mount Tmolus — the source of King Midas's legendary fortune. In
French, *"un pactole"* has since become a common word for a
windfall or a small fortune. A fitting name, then, for a tool that
helps you keep track of yours.

## Installation

The published `pactole-cli` crate can be installed system-wide with
`cargo install`, which builds a release binary and copies it to Cargo's bin
directory (`~/.cargo/bin` by default, make sure it is on your `PATH`):

```sh
cargo install pactole-cli
```

This installs a `pactole` binary. Once installed, you can run it from
anywhere.

To upgrade to the latest published version, run `cargo install pactole-cli
--force`. To install the development version from a clone, use
`cargo install --path crates/pactole-cli` instead. To uninstall:

```sh
cargo uninstall pactole-cli
```

For editor diagnostics, install the published language server separately:

```sh
cargo install pactole-lsp
```

See the [language server guide](https://etienne-monier.github.io/pactole/how-to/use-the-lsp/)
for editor configuration and current limitations.

## Usage

Initialize a starter ledger and the default OFX import configuration:

```sh
pactole init ledger.pactole
```

The command creates `ledger.pactole` and, if missing,
`~/.config/pactole/import-ofx.toml`. Existing files are never overwritten. Use
options such as `--account`, `--currency` and `--counter-account` to customize
the generated declarations.

A Pactole ledger is a plain `.pactole` text file made of directives
(`open`, `commodity`, `payee`, `balance`, `transaction`...). See
[`GRAMMAR.md`](GRAMMAR.md) for the full syntax, for example:

```pactole
2026-09-03 open Assets:Checking

2026-01-01 commodity EUR

payee "Whole Foods"

2026-09-03 * "Whole Foods" "Weekly groceries"
  Expenses:Groceries 45.30 EUR
  Assets:Checking -45.30 EUR
```

Check that a file parses correctly and inspect the resulting entries
(mainly useful for debugging):

```sh
pactole parse path/to/file.pactole
```

Check that a file is fully valid: not only that it parses correctly,
but also that it makes sense from a business point of view (accounts
are opened before being used or closed, commodities are declared
before being used, every transaction's payee is a known `payee`, and
every transaction balances):

```sh
pactole check path/to/file.pactole
```

Reformat a file into its canonical form, printed to stdout by default,
or written back in place with `--write`:

```sh
pactole fmt path/to/file.pactole
pactole fmt --write path/to/file.pactole
```

`fmt` also reads from stdin when given `-` as the file, always
printing the result to stdout:

```sh
cat path/to/file.pactole | pactole fmt -
```

Posting amounts and commodities are right-aligned to 50 columns by default.
Override the width in `~/.config/pactole/format.toml` (or
`$XDG_CONFIG_HOME/pactole/format.toml`):

```toml
posting_width = 50
```

List the transactions of a file, one line per matching posting, with
a running balance kept per commodity (like the `register` report of
other plain-text accounting tools). Without any filter, every posting
is listed; filters can be combined and must all be satisfied
(`--account` also matches sub-accounts, e.g. `Depenses` matches
`Depenses:Alimentation`):

```sh
pactole register path/to/file.pactole
pactole register path/to/file.pactole --account Depenses:Alimentation
pactole register path/to/file.pactole --from 2026-01-01 --to 2026-12-31
pactole register path/to/file.pactole --payee Carrefour --status cleared
pactole register path/to/file.pactole --tag vacances
```

## Importing a bank statement (OFX)

`pactole import-ofx` turns an OFX/QFX statement (OFX 1.x SGML or OFX 2.x
XML, as exported by most banks) into canonical `.pactole` transactions,
with the bank account and the categorization rules resolved from a
configuration file:

```sh
pactole import-ofx statement.ofx
```

By default the configuration is read from
`~/.config/pactole/import-ofx.toml` (or `$XDG_CONFIG_HOME/pactole/import-ofx.toml`
when `XDG_CONFIG_HOME` is set); override it with `--config`:

```toml
bank_account = "Actifs:Banque"
currency = "EUR"
default_counter_account = "Depenses:A-Classer"

[[rules]]
payee_contains = "carrefour"
counter_account = "Depenses:Alimentation"
payee = "Carrefour"

[[rules]]
memo_contains = "salaire"
direction = "credit"
counter_account = "Revenus:Salaire"
```

Each transaction records its OFX `FITID` as `fitid` metadata, which makes
re-imports idempotent. Pass `--into` an existing `.pactole` file to skip
transactions whose `FITID` is already there, and add `--write` to append
the new transactions to that file instead of printing them:

```sh
pactole import-ofx statement.ofx --into ledger.pactole --write
```

Generated transactions are a fragment (no `open`/`commodity`/`payee`
declarations), so `pactole check` on them alone is expected to fail until
they are merged into a full journal.

More commands (querying balances, reports, etc.) are coming.
