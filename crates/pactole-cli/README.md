# pactole-cli

Command-line interface of Pactole: the `pactole` binary.

## Role

Built with `clap` (derive), this crate exposes the features of
`pactole-core`/`pactole-storage-fs` through the **strict** flow (see
[`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) §3.1): all or nothing —
a file is either fully parsed/validated successfully or rejected with
the first error encountered (including in an included file).

Note that the Cargo package is named `pactole-cli`, but the produced
binary is named `pactole` (see the `[[bin]]` of `Cargo.toml`), so that
`cargo install --path crates/pactole-cli` directly installs a `pactole`
command.

## Main commands

- **`pactole parse <file>`**: parses the file and prints (`Debug`) the
  resulting journal entries. Mainly useful for debugging the
  grammar/parser.
- **`pactole fmt <file> [--write]`**: reformats a `.pactole` file into
  its canonical form, printed to stdout by default, or written in place
  with `--write`. Accepts `-` as the file name to read from stdin (always
  printed to stdout in that case).
- **`pactole check <file>`**: syntax parsing **then** full business
  validation (`pactole_core::validate_journal`) — accounts opened before
  use/close, commodities declared before use, each transaction's payee
  declared, each transaction balanced.
- **`pactole register <file> [filters]`**: lists transactions, one line
  per matching posting, with a running balance per commodity. Without
  filters, all postings are listed; filters are combined (`--account`
  also matches sub-accounts, `--from`/`--to`, `--payee`/`--narration` are
  case-insensitive, `--tag`, `--status`).

See [`../../README.md`](../../README.md) for complete usage examples and
installation (`cargo install --path crates/pactole-cli`).

## Dependencies / boundaries

- `pactole-core` (validation, register), `pactole-storage-fs`
  (`PactoleFileStorage`, `format`) — only their historical, strict API.
- `clap` (argument parsing), `chrono` (`--from`/`--to` filter dates).
- Never consumes `pactole-syntax` nor the tolerant/positioned flow
  (`analysis.rs`/`loader.rs` of `pactole-storage-fs`) directly: that flow
  is reserved for `pactole-lsp`.

## Tests / validation

```sh
# End-to-end CLI integration tests
cargo test -p pactole-cli --test cli

# All tests of the crate
cargo test -p pactole-cli
cargo clippy -p pactole-cli --all-targets
```

Integration tests live in `crates/pactole-cli/tests/cli.rs`: add any new
observable CLI behavior (new command, new filter, new exit code) there
rather than in `main.rs`.

## See also

- [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) §3.1 and §6 — strict
  flow and command details.
- [`../../GRAMMAR.md`](../../GRAMMAR.md) — the `.pactole` language
  specification handled by these commands.
- [`../../README.md`](../../README.md) — installation and usage guide
  for end users.
