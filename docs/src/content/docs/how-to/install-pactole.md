---
title: Install Pactole
description: Install the pactole command-line tool from source and keep it up to date.
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

## Install from source

From a clone of the repository, run:

```sh
cargo install --path crates/pactole-cli
```

Cargo builds an optimized release binary and copies it to its bin directory
(`~/.cargo/bin` by default). Make sure that directory is on your `PATH`, then
verify the installation:

```sh
pactole --version
```

## Update

After pulling new changes, re-run the same `cargo install` command. If Cargo
complains that the binary already exists, force the reinstall:

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
- [Command-line reference](/reference/cli/) — every command and option.
