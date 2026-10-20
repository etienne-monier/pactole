# pactole-core

Pure business domain of Pactole: models, validation invariants, and
financial calculations.

## Role

`pactole-core` contains only domain logic: **no I/O, no tree-sitter
dependency**. It must never depend on `pactole-storage-fs`,
`pactole-syntax`, `tree-sitter`, or any filesystem operation. Any
conversion from a `.pactole` file (concrete syntax → models) is the
responsibility of `pactole-storage-fs`.

## Main API

Re-exported from `src/lib.rs`:

- **Models** (`models/`): `Journal`, `Entry`, `Transaction`, `Posting`,
  `Amount`, `Balance`, `Open`, `Close`, `Commodity`, `Payee`,
  `TransactionStatus`, `Metadata`, as well as validated newtypes
  (`AccountName`, `CommodityName`, `MetadataKey`).
- **`validate_journal(journal: Journal) -> Result<Journal, ValidationError>`**
  (`validation.rs`): consumes the input `Journal` and checks the business
  invariants — chronological ordering of entries, account lifecycle
  (opened before use/close, not used after close), commodities declared
  before use, payees declared before use in a transaction, and balancing
  of each transaction per commodity. On success, it returns the `Journal`
  (sorted chronologically and with self-balanced transactions); on
  failure, it returns a `ValidationError`.
- **`register(journal: &Journal, filter: &RegisterFilter) -> Vec<RegisterEntry>`**
  (`register.rs`): computes a "register" report (running balance per
  commodity), filterable by account (including sub-accounts), date range,
  payee, narration, tag, status.
- **`ReadableStorage`** (`traits.rs`): storage abstraction trait,
  implemented by `pactole-storage-fs::PactoleFileStorage` (and intended
  to be implemented by a future database storage crate).
- **`ModelError`, `ValidationError`** (`errors.rs`): structured errors
  via `thiserror`.

## Dependencies / boundaries

- `chrono` (dates), `rust_decimal` (financial arithmetic — never `f32`/`f64`
  for amounts), `thiserror` (structured errors).
- Depends on no other workspace crate.

## Tests / validation

```sh
cargo test -p pactole-core
cargo clippy -p pactole-core --all-targets
```

Unit tests for domain and validation logic live in this crate (see the
`validation.rs`, `register.rs`, and `models/` modules).

## See also

- [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) — the role of
  `pactole-core` in the overall architecture and its boundaries with the
  other crates.
- [`../../GRAMMAR.md`](../../GRAMMAR.md) — the language specification of
  which these models and invariants are the Rust expression.
