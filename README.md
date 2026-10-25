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

Initialize a starter ledger and the project configuration:

```sh
pactole init ledger.pactole
```

The command creates `ledger.pactole` and, if missing, a `pactole.toml`
next to it. Existing files are never overwritten. Use options such as
`--account`, `--currency` and `--counter-account` to customize the
generated declarations, or `--global` to write `pactole.toml` into the
user configuration directory (`~/.config/pactole/pactole.toml`, or
`$XDG_CONFIG_HOME/pactole/pactole.toml`) instead.

A Pactole ledger is a plain `.pactole` text file made of directives
(`open`, `commodity`, `payee`, `balance`, `transaction`...). See
[`GRAMMAR.md`](GRAMMAR.md) for the full syntax. Transaction statuses are
optional and default to uncleared (`?`), for example:

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

Every check is reported as an error by default. Two checks are configurable
in `pactole.toml`: the mandatory payee declaration (`[validation] payee`,
an error by default) and never-used declarations (`[validation] unused`, a
warning by default): an account opened but never posted to, a commodity or
payee declared but never used. Each accepts `"off"`, `"warn"` or
`"error"`; `pactole check --strict` turns warnings into a failure.

Generate shell completion scripts to stdout:

```sh
pactole completions bash > ~/.local/share/bash-completion/completions/pactole
pactole completions zsh > ~/.zfunc/_pactole
pactole completions fish > ~/.config/fish/completions/pactole.fish
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
Override the width in the `[format]` section of `pactole.toml`:

```toml
[format]
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

Build the balance sheet of a file as of a date. Liabilities and equity are
shown with a positive sign, and the net result of the income and expense
accounts closes into equity, so a fully classified, balanced journal
satisfies `Assets = Liabilities + Equity` per commodity:

```sh
pactole balance path/to/file.pactole
pactole balance path/to/file.pactole --date 2026-03-31 --format json
pactole balance path/to/file.pactole --depth 1
```

Build the income statement of a file over a period. Income and expenses are
shown as positive amounts and the total is the net result (`income −
expenses`) per commodity:

```sh
pactole income path/to/file.pactole
pactole income path/to/file.pactole --from 2026-01-01 --to 2026-03-31
pactole income path/to/file.pactole --by payee
pactole income path/to/file.pactole --by tag --format json
```

Build the expenses breakdown of a file over a period. Only expense accounts
are included. Amounts keep their accounting sign and the total is the
period's spending per commodity:

```sh
pactole expenses path/to/file.pactole
pactole expenses path/to/file.pactole --from 2026-01-01 --to 2026-03-31
pactole expenses path/to/file.pactole --by payee
pactole expenses path/to/file.pactole --by tag --format json
pactole expenses path/to/file.pactole --by month
pactole expenses path/to/file.pactole --by account --depth 1
pactole expenses path/to/file.pactole --uncategorized --inactive
pactole expenses path/to/file.pactole --from 2026-02-01 --to 2026-02-28 --compare previous --diff percent
pactole expenses path/to/file.pactole --from 2026-02-01 --to 2026-02-28 --compare previous --reverse-diff
```

Uncategorized accounts can be configured in `[reports].uncategorized`;
otherwise `--uncategorized` uses `[import].default_counter_account` when set.
`--compare previous` compares the immediately preceding period. Differences
default to `current − previous` (positive means spending increased);
`--reverse-diff` switches the direction.

Accounts are classified by their root (the first `:`-separated segment)
through the `[reports]` section of `pactole.toml` (see
[Configuration](#configuration)). The defaults are the English `Assets`,
`Liabilities`, `Equity`, `Income` and `Expenses`; a journal using another
convention only configures the roots it needs:

```toml
[reports]
assets = ["Actifs"]
liabilities = ["Passif"]
equity = ["Capitaux"]
income = ["Revenus"]
expenses = ["Depenses", "Impots"]
```

## Importing a bank statement (OFX)

`pactole import-ofx` turns an OFX/QFX statement (OFX 1.x SGML or OFX 2.x
XML, as exported by most banks) into canonical `.pactole` transactions,
with the bank account and the categorization rules resolved from the
`[import]` section of `pactole.toml`:

```sh
pactole import-ofx statement.ofx
```

The configuration is the resolved `pactole.toml` (see
[Configuration](#configuration)); override it for one run with the global
`--config` flag (`pactole --config ./pactole.toml import-ofx statement.ofx`):

```toml
[import]
bank_account = "Actifs:Banque"
currency = "EUR"
default_counter_account = "Depenses:A-Classer"

[import.accounts]
"00012345678" = "Actifs:Courant"
"00098765432" = "Actifs:Epargne"

[[import.rules]]
payee_contains = "carrefour"
counter_account = "Depenses:Alimentation"
payee = "Carrefour"

[[import.rules]]
memo_contains = "salaire"
direction = "credit"
counter_account = "Revenus:Salaire"
```

For statements covering several physical accounts, the optional
`[import.accounts]` table maps each statement account identifier (the OFX
`ACCTID`) to its Pactole account; `bank_account` remains the fallback for
accounts it does not cover (warning emitted). `pactole import-ofx --account
Actifs:Epargne` instead posts every imported transaction to a single account
for that run, whatever the configuration says:

```sh
pactole import-ofx statement.ofx --account Actifs:Epargne
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

## Migrating from Actual Budget

For a one-time migration, use `import-actual`. It creates a new complete
journal and does not use or modify `pactole.toml`:

```sh
pactole import-actual actual.sqlite --currency EUR \
  --opening-date 2026-04-30 --output ledger.pactole \
  --payee-metadata-key imported-payee
```

The command generates account, payee and commodity declarations, an explicit
opening transaction against `Equity:Opening`, multi-posting transactions for
Actual splits, and balanced entries for transfers. Actual `contains`, regular
expression (`matches`) and `oneOf` rules are applied to imported descriptions.
When `--output` is omitted, the generated journal is written to stdout.

## Configuration

Every command reads a single `pactole.toml`, resolved from lowest to
highest precedence:

1. the global file `~/.config/pactole/pactole.toml` (or
   `$XDG_CONFIG_HOME/pactole/pactole.toml`);
2. the project file `pactole.toml` next to the journal being operated on;
3. in-memory flags: `--config <path>` (a single explicit file replacing
   discovery). Validation is configured in `pactole.toml`.

Layers merge per key, so a project file only needs to override what it
changes. Unknown keys are rejected, so typos fail loudly:

```toml
[validation]
payee = "error" # undeclared payees: "off", "warn" or "error"
unused = "warn" # never-used declarations: "off", "warn" or "error"

[format]
posting_width = 50

[reports]
assets = ["Assets"]
liabilities = ["Liabilities"]
equity = ["Equity"]
income = ["Income"]
expenses = ["Expenses"]

[import]
bank_account = "Actifs:Banque"
currency = "EUR"
default_counter_account = "Depenses:A-Classer"
```

Optional, for editors: the `[declaration]` section names the files that
receive new payee/account/commodity declarations (see the `pactole-lsp`
README).

```toml
[declaration]
payees = "payees.pactole"
accounts = "accounts.pactole"
```

More commands (budgets) are coming.
