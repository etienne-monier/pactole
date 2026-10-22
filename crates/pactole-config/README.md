# pactole-config

Layered `pactole.toml` configuration for Pactole tools.

## Role

Single source of truth for the `pactole.toml` schema and its resolution
rules, shared by the `pactole` command-line tool and the `pactole-lsp`
language server. This is a **leaf crate**: it depends on no other Pactole
crate, so every other crate can depend on it without cycles.

Layers, from lowest to highest precedence:

1. the user-level file `~/.config/pactole/pactole.toml`
   (`$XDG_CONFIG_HOME/pactole/pactole.toml` when that variable is set,
   `%APPDATA%\pactole\pactole.toml` on Windows);
2. the project file `pactole.toml` (next to the journal);
3. in-memory overrides (the CLI's root `--config` flag, and the LSP's
   `initializationOptions`). `--config <path>` replaces the
   discovered layers with that single file.

Layers merge per key: a key absent from a higher layer keeps the value from
the lower one. `[import.accounts]` merges per entry, while
`[[import.rules]]` is replaced in block by the highest layer defining it.
Unknown keys are rejected per section, so typos fail loudly.

## Schema

```toml
[validation]
payee = "error" # undeclared payees: "off", "warn" or "error"
unused = "warn" # never-used declarations: "off", "warn" or "error"

[declaration]
payees = "payees.pactole"
accounts = "accounts.pactole"

[format]
posting_width = 50

[import]
bank_account = "Assets:Checking"
currency = "EUR"
default_counter_account = "Expenses:To-Be-Classified"

[import.accounts]
"00012345678" = "Assets:Checking"

[[import.rules]]
payee_contains = "supermarket"
counter_account = "Expenses:Groceries"
payee = "Supermarket"
```

- `[validation] payee` controls whether a transaction whose payee has no
  `payee` declaration is ignored (`off`), a warning (`warn`), or an error
  (`error`, the default). The payee itself remains mandatory in the grammar.
- `[validation] unused` controls the severity of declarations nothing ever
  uses: an account opened but never posted to nor balance-asserted, a
  commodity no amount uses (and no `open` restricts an account to), a payee
  no transaction uses. They are reported as warnings by default, dropped
  entirely under `off`, and blocking under `error`.
- `[declaration]` (`payees`, `accounts`, `commodities`) names the files where
  an editor front-end should insert a new declaration, instead of the current
  document. Paths are kept raw here; the consumer resolves them against the
  journal's directory. See the `pactole-lsp` README.
- `[format] posting_width` is the column at which posting amounts are
  aligned (default 50).
- `[import]` holds the OFX import settings used by `pactole import-ofx`. It
  is `None` when no layer defines the section; an incomplete section is an
  error.

## Main API

- **`Config`**: the fully resolved configuration
  (`validation`, `format`, `import`).
- **`Config::resolve(&[Source], &Overrides) -> Result<Config, ConfigError>`**:
  pure, no I/O.
- **`Config::load(explicit, project, &Overrides) -> Result<Config, ConfigError>`**:
  reads the layers from disk (an explicit file must exist and be readable;
  discovered files that are absent are skipped silently).
- **`Overrides`**: in-memory values applied on top of the file layers.
- **`ValidationLevel`**, **`ValidationOptions`**, **`FormatOptions`**,
  **`ImportConfig`**, **`Rule`**, **`Direction`**: the typed settings,
  re-exported by the crates that consume them
  (`pactole-storage-fs`, `pactole-import`).
- **`config_dir()`**, **`default_config_path()`**: user-level location
  helpers.
- **`ConfigError`**: read, parse and invalid-value errors, always naming
  the file that holds the offending value.

## Dependencies / boundaries

- Only `serde`, `toml` and `thiserror`.
- No I/O beyond reading the configuration files themselves; no knowledge of
  the journal format, OFX, or tree-sitter.

## Tests / validation

```sh
cargo test -p pactole-config
cargo clippy -p pactole-config --all-targets
```

Merge/precedence/error cases live in `tests/config.rs`.

## See also

- [`../../AGENTS.md`](../../AGENTS.md) — the resolved
  configuration and layering rules in context.
- [`../../README.md`](../../README.md#configuration) — user-facing
  configuration guide.
