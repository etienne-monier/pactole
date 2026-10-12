# pactole-import

Import external financial formats (currently OFX/QFX) into Pactole
domain models.

## Role

`pactole-import` is the input-side counterpart to `pactole-storage-fs`:
an edge adapter that translates a foreign format into the pure Pactole
domain. It depends only on `pactole-core` and performs **no I/O** — the
caller reads the source text and passes it in as a `&str`. It knows
nothing about tree-sitter, the `.pactole` text format, `pactole-storage-fs`,
or the configuration file format, which keeps it reusable by any
front-end (the `pactole` CLI today, a future database-backed storage
crate, the language server tomorrow).

The pipeline is deliberately split in two steps so future importers can
reuse the rule engine:

1. a format-specific parser turns the source text into format-neutral
   `ImportRecord`s (`ofx.rs` does this for OFX, via the `ofx-rs` crate);
2. a shared rule engine (`rules.rs`) turns `ImportRecord`s into balanced
   `pactole_core::Transaction`s, resolving the counter account (and an
   optional canonical payee) from the configured rules.

## Main API

Re-exported from `src/lib.rs`:

- **`import_ofx(source: &str, config: &ImportConfig, known_fitids: &HashSet<String>) -> Result<ImportOutcome, ImportError>`**:
  parses an OFX document (1.x SGML or 2.x XML), skips any transaction whose
  `FITID` is already in `known_fitids` (counted in `ImportOutcome::skipped`),
  and lowers the rest into transactions. Every produced transaction carries
  its `FITID` as `fitid` metadata, so a later run can rebuild the known set.
- **`ImportConfig`, `Rule`, `Direction`** (`config.rs`): serde types
  describing the bank account, the default counter account, an optional
  fallback currency, and the ordered categorization rules (first match
  wins; a rule may also override the payee). These types fix no file
  format: the caller chooses one (the CLI uses TOML).
- **`ImportRecord`** (`record.rs`): the format-neutral intermediate
  representation.
- **`ImportOutcome`**, **`ImportWarning`**, **`ImportError`**.

Produced transactions are `Cleared`, with two postings: the configured
counter account (negated amount) and the bank account (the signed source
amount), in the source currency.

## Dependencies / boundaries

- `pactole-core` (domain models), `ofx-rs` (OFX parsing), `chrono`,
  `rust_decimal` (never `f32`/`f64` for amounts), `serde`
  (configuration types), `thiserror` (structured errors).
- No dependency on `pactole-storage-fs`, `pactole-syntax`, or tree-sitter.
- Serializing the produced transactions to `.pactole` text is **not** done
  here: that is `pactole_storage_fs::format_transactions`.

## Tests / validation

```sh
cargo test -p pactole-import
cargo clippy -p pactole-import --all-targets
```

Integration tests live in `tests/import_ofx.rs`, with OFX fixtures under
`tests/fixtures/`.

## See also

- [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) — the role of
  `pactole-import` in the overall architecture and its boundaries.
- [`../../README.md`](../../README.md) — the `pactole import-ofx`
  command and its `~/.config/pactole/import-ofx.toml` configuration.
