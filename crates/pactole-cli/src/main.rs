use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use pactole_core::ReadableStorage;
use pactole_storage_fs::PactoleFileStorage;

/// Pactole command-line interface.
#[derive(Debug, Parser)]
#[command(
    name = "pactole",
    version,
    about = "Tools around the Pactole ledger format"
)]
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

    /// Reformat a `.pactole` file into its canonical form.
    ///
    /// Prints the result to stdout by default; pass `--write` to update
    /// the file in place. Use `-` as the file to read from stdin (the
    /// result is then always printed to stdout).
    Fmt {
        /// Path to the `.pactole` file to format, or `-` for stdin.
        file: String,

        /// Write the result back to `file` instead of printing it.
        #[arg(long)]
        write: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Command::Parse { file } => run_parse(file),
        Command::Fmt { file, write } => run_fmt(file, write),
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

fn run_fmt(file: String, write: bool) -> ExitCode {
    let source = if file == "-" {
        let mut buf = String::new();
        if let Err(err) = std::io::stdin().read_to_string(&mut buf) {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
        buf
    } else {
        match fs::read_to_string(&file) {
            Ok(source) => source,
            Err(err) => {
                eprintln!("error: {err}");
                return ExitCode::FAILURE;
            }
        }
    };

    let formatted = match pactole_storage_fs::format(&source) {
        Ok(formatted) => formatted,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    };

    if write && file != "-" {
        if let Err(err) = fs::write(&file, formatted) {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    } else {
        print!("{formatted}");
    }

    ExitCode::SUCCESS
}
