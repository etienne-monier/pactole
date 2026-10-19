use std::path::PathBuf;

use chrono::NaiveDate;
use clap::{Parser, Subcommand};

/// Pactole command-line interface.
#[derive(Debug, Parser)]
#[command(
    name = "pactole",
    version,
    about = "Tools around the Pactole ledger format"
)]
pub(crate) struct Cli {
    /// Path to an explicit `pactole.toml`, replacing the global and project
    /// configuration files.
    #[arg(long, value_name = "PATH")]
    pub(crate) config: Option<PathBuf>,

    /// Override the `payee` validation level from `pactole.toml`.
    #[arg(long, value_enum, value_name = "LEVEL")]
    pub(crate) validation: Option<ValidationArg>,

    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Parse a `.pactole` file and print the resulting journal entries.
    Parse {
        /// Path to the `.pactole` file to parse.
        file: PathBuf,
    },

    /// Reformat a `.pactole` file into its canonical form.
    Fmt {
        /// Path to the `.pactole` file to format, or `-` for stdin.
        file: String,

        /// Write the result back to `file` instead of printing it.
        #[arg(long)]
        write: bool,
    },

    /// Generate shell completion scripts and print them to stdout.
    Completions {
        /// Shell for which to generate completions.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    /// Check that a `.pactole` file is valid, both grammatically and from a
    /// business point of view.
    Check {
        /// Path to the `.pactole` file to check.
        file: PathBuf,

        /// Treat warnings as errors.
        #[arg(long)]
        strict: bool,
    },

    /// List the transactions of a `.pactole` file with a running balance.
    Register {
        /// Path to the `.pactole` file to read.
        file: PathBuf,

        /// Only show postings on this account or one of its sub-accounts.
        #[arg(short, long)]
        account: Option<String>,

        /// Only show transactions on or after this date (`YYYY-MM-DD`).
        #[arg(long)]
        from: Option<NaiveDate>,

        /// Only show transactions on or before this date (`YYYY-MM-DD`).
        #[arg(long)]
        to: Option<NaiveDate>,

        /// Only show transactions whose payee contains this text.
        #[arg(long)]
        payee: Option<String>,

        /// Only show transactions whose narration contains this text.
        #[arg(long)]
        narration: Option<String>,

        /// Only show transactions carrying this tag.
        #[arg(long)]
        tag: Option<String>,

        /// Only show transactions with this status.
        #[arg(long, value_enum)]
        status: Option<StatusArg>,
    },

    /// Import transactions from an OFX statement into canonical `.pactole`
    /// transaction directives.
    ImportOfx {
        /// Path to the OFX/QFX file to import.
        file: PathBuf,

        /// Path to an existing `.pactole` file used for deduplication.
        #[arg(long, value_name = "PATH")]
        into: Option<PathBuf>,

        /// Post every imported transaction to this Pactole account,
        /// overriding `import.bank_account` and the `[import.accounts]`
        /// mapping for this run.
        #[arg(long, value_name = "ACCOUNT")]
        account: Option<String>,

        /// Append generated transactions to `--into` instead of printing them.
        #[arg(long, requires = "into")]
        write: bool,
    },

    /// Initialize a ledger and, when missing, the `pactole.toml` configuration.
    Init {
        /// Path to the ledger file to create.
        #[arg(default_value = "ledger.pactole")]
        ledger: PathBuf,

        /// Main bank account.
        #[arg(long, default_value = "Assets:Checking")]
        account: String,

        /// Initial commodity/currency.
        #[arg(long, default_value = "EUR")]
        currency: String,

        /// Default counter account for uncategorized imports.
        #[arg(long, default_value = "Expenses:ToReview")]
        counter_account: String,

        /// Date for the initial declarations. Defaults to today.
        #[arg(long)]
        date: Option<NaiveDate>,

        /// Print the generated ledger instead of creating the ledger file.
        #[arg(long)]
        stdout: bool,

        /// Write `pactole.toml` into the user configuration directory
        /// instead of next to the ledger.
        #[arg(long)]
        global: bool,

        /// Do not create `pactole.toml`.
        #[arg(long)]
        no_config: bool,

        /// Do not create the ledger file.
        #[arg(long)]
        no_ledger: bool,
    },
}

/// CLI-facing mirror of [`pactole_config::ValidationLevel`].
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum ValidationArg {
    /// Report configurable checks as warnings instead of errors.
    Warn,
    /// Report configurable checks as errors (the default).
    Error,
    /// Disable the configurable check.
    Off,
}

impl From<ValidationArg> for pactole_config::ValidationLevel {
    fn from(value: ValidationArg) -> Self {
        match value {
            ValidationArg::Warn => pactole_config::ValidationLevel::Warning,
            ValidationArg::Error => pactole_config::ValidationLevel::Error,
            ValidationArg::Off => pactole_config::ValidationLevel::Off,
        }
    }
}

/// CLI-facing mirror of [`pactole_core::TransactionStatus`].
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum StatusArg {
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
