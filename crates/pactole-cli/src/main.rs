use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use chrono::NaiveDate;
use clap::{Parser, Subcommand};
use pactole_core::ReadableStorage;
use pactole_core::{RegisterFilter, TransactionStatus};
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

    /// Check that a `.pactole` file is valid, both grammatically and
    /// from a business point of view.
    ///
    /// This first parses the file like `parse` does, then additionally
    /// validates the resulting journal: entries are sorted
    /// chronologically, accounts must be open before being used in a
    /// transaction, a balance assertion or being closed, commodities
    /// must be declared before being used in a transaction, and every
    /// transaction must balance.
    Check {
        /// Path to the `.pactole` file to check.
        file: PathBuf,
    },

    /// List the transactions of a `.pactole` file, one line per
    /// matching posting, with a running balance.
    ///
    /// Without any filter, every posting of every transaction is
    /// listed. Filters can be combined; a posting/transaction must
    /// satisfy all of them to be shown.
    Register {
        /// Path to the `.pactole` file to read.
        file: PathBuf,

        /// Only show postings on this account, or on one of its
        /// sub-accounts (e.g. `Depenses` also matches
        /// `Depenses:Alimentation`).
        #[arg(short, long)]
        account: Option<String>,

        /// Only show transactions on or after this date (`YYYY-MM-DD`).
        #[arg(long)]
        from: Option<NaiveDate>,

        /// Only show transactions on or before this date (`YYYY-MM-DD`).
        #[arg(long)]
        to: Option<NaiveDate>,

        /// Only show transactions whose payee contains this text
        /// (case-insensitive).
        #[arg(long)]
        payee: Option<String>,

        /// Only show transactions whose narration contains this text
        /// (case-insensitive).
        #[arg(long)]
        narration: Option<String>,

        /// Only show transactions carrying this tag.
        #[arg(long)]
        tag: Option<String>,

        /// Only show transactions with this status.
        #[arg(long, value_enum)]
        status: Option<StatusArg>,
    },
}

/// CLI-facing mirror of [`pactole_core::TransactionStatus`], since the
/// latter does not implement `clap::ValueEnum`.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum StatusArg {
    Uncleared,
    Pending,
    Cleared,
}

impl From<StatusArg> for pactole_core::TransactionStatus {
    fn from(value: StatusArg) -> Self {
        match value {
            StatusArg::Uncleared => pactole_core::TransactionStatus::Uncleared,
            StatusArg::Pending => pactole_core::TransactionStatus::Pending,
            StatusArg::Cleared => pactole_core::TransactionStatus::Cleared,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Command::Parse { file } => run_parse(file),
        Command::Fmt { file, write } => run_fmt(file, write),
        Command::Check { file } => run_check(file),
        Command::Register {
            file,
            account,
            from,
            to,
            payee,
            narration,
            tag,
            status,
        } => run_register(file, account, from, to, payee, narration, tag, status),
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

    let journal = match storage.journal() {
        Ok(journal) => journal,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    };

    for entry in journal.entries() {
        println!("{entry:#?}");
    }

    ExitCode::SUCCESS
}

fn run_check(file: PathBuf) -> ExitCode {
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

    let journal = match storage.journal() {
        Ok(journal) => journal,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(err) = pactole_core::validate_journal(journal) {
        eprintln!("error: {err}");
        return ExitCode::FAILURE;
    }

    println!("OK");
    ExitCode::SUCCESS
}

#[allow(clippy::too_many_arguments)]
fn run_register(
    file: PathBuf,
    account: Option<String>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    payee: Option<String>,
    narration: Option<String>,
    tag: Option<String>,
    status: Option<StatusArg>,
) -> ExitCode {
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

    let journal = match storage.journal() {
        Ok(journal) => journal,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    };

    let journal = match pactole_core::validate_journal(journal) {
        Ok(journal) => journal,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    };

    let account = match account {
        Some(account) => match pactole_core::AccountName::new(account) {
            Ok(account) => Some(account),
            Err(err) => {
                eprintln!("error: {err}");
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };

    let filter = RegisterFilter {
        account,
        from,
        to,
        payee,
        narration,
        tag,
        status: status.map(TransactionStatus::from),
    };

    for entry in pactole_core::register(&journal, &filter) {
        let status = match entry.status {
            TransactionStatus::Uncleared => "?",
            TransactionStatus::Pending => "!",
            TransactionStatus::Cleared => "*",
        };
        let description = [entry.payee.as_deref(), entry.narration.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        let amount = match &entry.amount {
            Some(amount) => format!("{} {}", amount.number, amount.commodity),
            None => String::new(),
        };
        let balance = entry
            .balance
            .iter()
            .map(|(commodity, sum)| format!("{sum} {commodity}"))
            .collect::<Vec<_>>()
            .join(", ");

        println!(
            "{}  {}  {:<40}  {:<25}  {:>15}  {:>15}",
            entry.date, status, description, entry.account, amount, balance
        );
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
