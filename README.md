![Pactole logo](logo.png)

Pactole is a plain-text ledger tool written in Rust: you record your
transactions as human-readable, git-friendly `.pactole` files (à la
[Beancount](https://beancount.github.io/docs/) or Ledger-cli), and
Pactole parses, checks and formats them for you.

Its name comes from the **Pactole**, the ancient name of a small river
in Lydia (Asia Minor) whose sands were said to carry gold washed down
from Mount Tmolus — the source of King Midas's legendary fortune. In
French, *"un pactole"* has since become a common word for a
windfall or a small fortune. A fitting name, then, for a tool that
helps you keep track of yours.

## Installation

The `pactole` CLI can be installed system-wide with `cargo install`,
which builds a release binary and copies it to Cargo's bin directory
(`~/.cargo/bin` by default, make sure it is on your `PATH`):

```sh
cargo install --path crates/pactole-cli
```

This installs a `pactole` binary. Once installed, you can run it from
anywhere.

To upgrade after pulling new changes, just re-run the same
`cargo install` command (add `--force` if Cargo complains that the
binary already exists). To uninstall:

```sh
cargo uninstall pactole-cli
```

## Usage

A Pactole ledger is a plain `.pactole` text file made of directives
(`open`, `commodity`, `balance`, `transaction`...). See
[`GRAMMAR.md`](GRAMMAR.md) for the full syntax, for example:

```pactole
2026-09-03 open Assets:Checking

2026-01-01 commodity EUR

2026-09-03 * "Whole Foods" "Weekly groceries"
  Expenses:Groceries 45.30 EUR
  Assets:Checking -45.30 EUR
```

Check that a file parses correctly and inspect the resulting entries
(mainly useful for debugging):

```sh
pactole parse path/to/file.pactole
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

More commands (querying balances, reports, etc.) are coming.
