use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use pactole_core::ReadableStorage;
use pactole_storage_fs::PactoleFileStorage;

/// Pactole command-line interface.
#[derive(Debug, Parser)]
#[command(name = "pactole", version, about = "Tools around the Pactole ledger format")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Parse a `.pactole` file and print the resulting journal entries.
    ///
    /// This is mainly meant for debugging the grammar/parser: it prints
    /// each entry of the journal on its own using its `Debug`
    /// representation.
    Parse {
        /// Path to the `.pactole` file to parse.
        file: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Command::Parse { file } => run_parse(file),
    }
}

fn run_parse(file: PathBuf) -> ExitCode {
    let mut storage = match PactoleFileStorage::try_from(file) {
        Ok(storage) => storage,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(err) = storage.parse() {
        eprintln!("error: {err}");
        return ExitCode::FAILURE;
    }

    let entries = match storage.get_all() {
        Ok(entries) => entries,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    };

    for entry in entries {
        println!("{entry:#?}");
    }

    ExitCode::SUCCESS
}
