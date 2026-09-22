![Pactole logo](logo.png)

A ledger tool written in Rust

## Installation

The `pactole` CLI can be installed system-wide with `cargo install`,
which builds a release binary and copies it to Cargo's bin directory
(`~/.cargo/bin` by default, make sure it is on your `PATH`):

```sh
cargo install --path crates/pactole-cli
```

This installs a `pactole` binary. Once installed, you can run it from
anywhere, e.g.:

```sh
pactole parse path/to/file.pactole
```

To upgrade after pulling new changes, just re-run the same
`cargo install` command (add `--force` if Cargo complains that the
binary already exists). To uninstall:

```sh
cargo uninstall pactole-cli
```
