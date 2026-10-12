---
title: Install Pactole
description: Install the Pactole command-line tool from crates.io or from source.
sidebar:
  order: 1
---

This guide explains how to install the `pactole` binary on your machine.

## Prerequisites

You need a working Rust toolchain (a recent stable release). If you do not have
one, install it with [rustup](https://rustup.rs/):

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Install from crates.io

Install the published crate with Cargo:

```sh
cargo install pactole-cli
```

Cargo builds an optimized release binary and copies it to its bin directory
(`~/.cargo/bin` by default). Make sure that directory is on your `PATH`, then
verify the installation:

```sh
pactole --version
```

## Install from source

To install the development version from a clone of the repository, run:

```sh
git clone https://github.com/etienne-monier/pactole.git
cd pactole
cargo install --path crates/pactole-cli
```

## Update

To update the published version, run:

```sh
cargo install pactole-cli --force
```

For a source installation, pull the latest changes and re-run the command from
the previous section. If Cargo complains that the binary already exists, force
the reinstall:

```sh
cargo install --path crates/pactole-cli --force
```

## Uninstall

```sh
cargo uninstall pactole-cli
```

## Related

- [Getting started](/tutorials/getting-started/) — write your first ledger once
  Pactole is installed.
- [Use the language server](/how-to/use-the-lsp/) — configure editor diagnostics.
- [Command-line reference](/reference/cli/) — every command and option.
