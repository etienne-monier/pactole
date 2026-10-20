# pactole-cli

Command-line interface of Pactole: the `pactole` binary.

## Role

Built with `clap` (derive), this crate exposes the features of
`pactole-core`/`pactole-storage-fs`. Formatting and `parse` use the
**strict** flow (see [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) §3.1):
all or nothing. `check` and `register` use the positioned multi-file analysis
(§3.2) so that diagnostics from included files keep their provenance, while
`check` still fails on any error-severity diagnostic.

Note that the Cargo package is named `pactole-cli`, but the produced
binary is named `pactole` (see the `[[bin]]` of `Cargo.toml`), so that
`cargo install --path crates/pactole-cli` directly installs a `pactole`
command.

## Main commands

The root flag `--config <path>` is accepted before the subcommand to use a
specific `pactole.toml` as the only configuration layer. Validation levels are
configured in `pactole.toml`.

- **`pactole parse <file>`**: parses the file and prints (`Debug`) the
  resulting journal entries. Mainly useful for debugging the
  grammar/parser.
- **`pactole fmt <file> [--write]`**: reformats a `.pactole` file into
  its canonical form, printed to stdout by default, or written in place
  with `--write`. Accepts `-` as the file name to read from stdin (always
  printed to stdout in that case). Uses the resolved `[format]` options.
- **`pactole completions <shell>`**: generates shell completions to stdout
  for Bash, Elvish, Fish, PowerShell or Zsh.
- **`pactole check <file> [--strict]`**: syntax parsing **then** full
  business validation (`pactole_core::validate_journal`) — accounts opened
  before use/close, commodities declared before use, each transaction's
  payee declared, each transaction balanced. Configurable checks follow the
  resolved `[validation]` levels; `--strict` turns warnings into a failure.
- **`pactole register <file> [filters]`**: lists transactions, one line
  per matching posting, with a running balance per commodity. Without
  filters, all postings are listed; filters are combined (`--account`
  also matches sub-accounts, `--from`/`--to`, `--payee`/`--narration` are
  case-insensitive, `--tag`, `--status`).
- **`pactole init [ledger] [--global]`**: creates a starter ledger and, when
  missing, a `pactole.toml` next to it (or in the user configuration
  directory with `--global`).
- **`pactole import-ofx <file> [--into <path>] [--account <account>]
  [--write]`**: imports an OFX/QFX statement into canonical transactions,
  using the `[import]` section of the resolved configuration.

See [`../../README.md`](../../README.md) for complete usage examples and
installation (`cargo install --path crates/pactole-cli`).

## Dependencies / boundaries

- `pactole-core` (validation, register), `pactole-storage-fs`
  (`analyze_file_with_loader_with_options`, `format`), `pactole-import` (OFX
  import), `pactole-config` (layered `pactole.toml`).
- `clap` (argument parsing), `chrono` (`--from`/`--to` filter dates).
- `check` and `register` use the loader-based, positioned analysis of
  `pactole-storage-fs::analysis` so that diagnostics from included files keep
  their provenance; `parse` and `fmt` use the historical strict parser.

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
