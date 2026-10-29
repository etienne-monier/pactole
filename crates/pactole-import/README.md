# pactole-import

Import external financial formats (currently OFX/QFX) into Pactole
domain models.

## Role

`pactole-import` is the input-side counterpart to `pactole-storage-fs`:
an edge adapter that translates a foreign format into the pure Pactole
domain. It depends only on `pactole-core` and `pactole-config` and
performs **no I/O** — the caller reads the source text and passes it in
as a `&str`. It knows nothing about tree-sitter, the `.pactole` text
format, `pactole-storage-fs`, or where its configuration comes from,
which keeps it reusable by any front-end (the `pactole` CLI today, a
future database-backed storage crate, the language server tomorrow).

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
- **`ImportConfig`, `Rule`, `Direction`** (defined in `pactole-config`, the
  `[import]` section of `pactole.toml`, and re-exported here): describe the
  bank account, the optional `[import.accounts]` source-account mapping, the
  default counter account, an optional fallback currency, and the ordered
  categorization rules (evaluated in order; every matching rule is applied,
  and a rule may also override the payee seen by subsequent rules).
- **`ImportRecord`** (`record.rs`): the format-neutral intermediate
  representation, including the source account identifier (OFX `ACCTID`)
  when the format provides one.
- **`ImportOutcome`**, **`ImportWarning`** (missing payee, unmapped source
  account), **`ImportError`**.

Produced transactions are `Cleared`, with two postings: the configured
counter account (negated amount) and the bank account (the signed source
amount), in the source currency. The bank-side account is resolved per
transaction from `ImportConfig::accounts` (keyed by the source account
identifier), falling back to `ImportConfig::bank_account`.

## Dependencies / boundaries

- `pactole-core` (domain models), `pactole-config` (import settings),
  `ofx-rs` (OFX parsing), `chrono`, `rust_decimal` (never `f32`/`f64` for
  amounts), `thiserror` (structured errors).
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

- [`../../AGENTS.md`](../../AGENTS.md) — the role of
  `pactole-import` in the overall architecture and its boundaries.
- [`../../README.md`](../../README.md) — the `pactole import-ofx`
  command and its `[import]` section in `pactole.toml`.
