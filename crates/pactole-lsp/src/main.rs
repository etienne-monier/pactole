#![warn(missing_docs)]
#![warn(clippy::missing_docs_in_private_items)]

//! `pactole-lsp` binary entry point: runs a minimal Language Server Protocol
//! server for `.pactole` files over stdio.

use clap::Parser;

/// Pactole command-line interface.
#[derive(Debug, Parser)]
#[command(
    name = "pactole-lsp",
    version,
    about = "Language server for Pactole ledgers"
)]
struct Cli {
    /// Accepted for compatibility with editor configurations that pass
    /// `--stdio`: stdio is the only supported transport.
    #[arg(long)]
    stdio: bool,
}

fn main() -> std::process::ExitCode {
    // Parsed before the logger is set up: `--version` and `--help` are
    // answered by clap and exit from here, without touching the logs.
    let cli = Cli::parse();

    // Logs must never go to stdout: it is the LSP protocol's own transport
    // (stdio), so any stray write there would corrupt the message stream.
    // `env_logger::Target::Stderr` makes this explicit rather than relying
    // on the (already stderr) default. Verbosity is controlled by `RUST_LOG`
    // (e.g. `RUST_LOG=pactole_lsp=debug`); with no `RUST_LOG` set, logging
    // stays silent.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("pactole_lsp=info"))
        .target(env_logger::Target::Stderr)
        .init();

    if cli.stdio {
        log::debug!("stdio transport requested (the only supported transport)");
    }

    match pactole_lsp::server::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("pactole-lsp: fatal error: {err}");
            std::process::ExitCode::FAILURE
        }
    }
}
