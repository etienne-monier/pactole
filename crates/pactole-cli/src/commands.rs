use std::num::NonZeroUsize;
use std::path::PathBuf;

use chrono::NaiveDate;
use clap::{Parser, Subcommand};

/// Pactole command-line interface.
#[derive(Debug, Parser)]
#[command(
    name = "pactole",
    version,
    about = "Tools around the Pactole ledger format",
    subcommand_help_heading = "Commands"
)]
pub(crate) struct Cli {
    /// Path to an explicit `pactole.toml`, replacing the global and project
    /// configuration files.
    #[arg(long, value_name = "PATH")]
    pub(crate) config: Option<PathBuf>,

    #[command(subcommand)]
    /// Command to execute.
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
/// Subcommands supported by the Pactole CLI.
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

        /// Output format.
        #[arg(long, value_enum, default_value = "text")]
        format: CheckFormatArg,
    },

    /// Display the include graph and report repeated, cyclic and unreachable files.
    Graph {
        /// Root `.pactole` file.
        file: PathBuf,

        /// Directory recursively searched for unreachable `.pactole` files.
        /// Defaults to the root journal's directory.
        #[arg(long, value_name = "DIR")]
        scan: Option<PathBuf>,

        /// Output format.
        #[arg(long, value_enum, default_value = "text")]
        format: GraphFormatArg,
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

        /// Output format.
        #[arg(long, value_enum, default_value = "text")]
        format: RegisterFormatArg,

        /// Convert amounts to this commodity using market prices.
        #[arg(long, value_name = "COMMODITY")]
        convert_to: Option<String>,

        /// Market-price valuation date; defaults to --to or today.
        #[arg(long, value_name = "DATE", requires = "convert_to")]
        at: Option<NaiveDate>,
    },
    /// Show the balance sheet of a `.pactole` file at a given date.
    Balance {
        /// Path to the `.pactole` file to read.
        file: PathBuf,

        /// Reference date of the sheet (`YYYY-MM-DD`). Defaults to today.
        #[arg(long)]
        date: Option<NaiveDate>,

        /// Output format.
        #[arg(long, value_enum, default_value = "text")]
        format: FormatArg,

        /// Fold the accounts deeper than this many levels.
        #[arg(long)]
        depth: Option<NonZeroUsize>,

        /// Convert amounts to this commodity using market prices.
        #[arg(long, value_name = "COMMODITY")]
        convert_to: Option<String>,

        /// Market-price valuation date; defaults to --date or today.
        #[arg(long, value_name = "DATE", requires = "convert_to")]
        at: Option<NaiveDate>,
    },

    /// Show the income statement of a `.pactole` file over a period.
    Income {
        /// Path to the `.pactole` file to read.
        file: PathBuf,

        /// Start of the period (`YYYY-MM-DD`). Defaults to January 1 of the
        /// year containing `--to`.
        #[arg(long)]
        from: Option<NaiveDate>,

        /// End of the period (`YYYY-MM-DD`). Defaults to today.
        #[arg(long)]
        to: Option<NaiveDate>,

        /// Grouping of the report.
        #[arg(long, value_enum, default_value = "account")]
        by: ByArg,

        /// Output format.
        #[arg(long, value_enum, default_value = "text")]
        format: FormatArg,

        /// Fold the accounts deeper than this many levels (only when
        /// `--by account`).
        #[arg(long)]
        depth: Option<NonZeroUsize>,

        /// Convert amounts to this commodity using market prices.
        #[arg(long, value_name = "COMMODITY")]
        convert_to: Option<String>,

        /// Market-price valuation date; defaults to --to or today.
        #[arg(long, value_name = "DATE", requires = "convert_to")]
        at: Option<NaiveDate>,
    },

    /// Show the expenses breakdown of a `.pactole` file over a period.
    Expenses {
        /// Path to the `.pactole` file to read.
        file: PathBuf,

        /// Start of the period (`YYYY-MM-DD`). Defaults to January 1 of the
        /// year containing `--to`.
        #[arg(long)]
        from: Option<NaiveDate>,

        /// End of the period (`YYYY-MM-DD`). Defaults to today.
        #[arg(long)]
        to: Option<NaiveDate>,

        /// Grouping of the report.
        #[arg(long, value_enum, default_value = "account")]
        by: ExpensesByArg,

        /// Output format.
        #[arg(long, value_enum, default_value = "text")]
        format: FormatArg,

        /// Fold the accounts deeper than this many levels (only when
        /// `--by account`).
        #[arg(long)]
        depth: Option<NonZeroUsize>,

        /// Show uncategorized transactions.
        #[arg(long)]
        uncategorized: bool,

        /// Show inactive expense accounts.
        #[arg(long)]
        inactive: bool,

        /// Compare against previous period or another period.
        #[arg(long, value_enum)]
        compare: Option<CompareArg>,

        /// Start of the comparison period.
        #[arg(long)]
        compare_from: Option<NaiveDate>,

        /// End of the comparison period.
        #[arg(long)]
        compare_to: Option<NaiveDate>,

        /// Difference display mode.
        #[arg(long, value_enum, default_value = "absolute")]
        diff: DiffArg,

        /// Reverse difference direction (use previous − current).
        #[arg(long)]
        reverse_diff: bool,

        /// Convert amounts to this commodity using market prices.
        #[arg(long, value_name = "COMMODITY")]
        convert_to: Option<String>,

        /// Market-price valuation date; defaults to --to or today.
        #[arg(long, value_name = "DATE", requires = "convert_to")]
        at: Option<NaiveDate>,
    },
    /// Import transactions from an OFX statement into canonical `.pactole`
    /// transaction directives.
    ImportOfx {
        /// Path to the OFX/QFX file to import.
        file: PathBuf,

        /// Only import transactions on or after this date (`YYYY-MM-DD`).
        #[arg(long, value_name = "DATE")]
        from: Option<NaiveDate>,

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

        /// Do not preserve the source payee in transaction metadata.
        #[arg(long)]
        no_imported_payee: bool,

        /// Omit the negative posting amount and let Pactole infer it.
        #[arg(long)]
        omit_negative_amount: bool,
    },

    /// Migrate an Actual Budget database into a new `.pactole` journal.
    ImportActual {
        /// Path to the Actual Budget sqlite file.
        file: PathBuf,

        /// Commodity used for every imported amount (for example `EUR`).
        #[arg(long)]
        currency: String,

        /// Output journal path. When omitted, the journal is written to stdout.
        #[arg(long, value_name = "PATH")]
        output: Option<PathBuf>,

        /// Equity account used as the counter-account for opening balances.
        #[arg(long, default_value = "Equity:Opening")]
        opening_equity: String,

        /// Date of the explicit opening transaction.
        #[arg(long, default_value = "1970-01-01")]
        opening_date: NaiveDate,

        /// Metadata key receiving the original Actual payee, when supplied.
        #[arg(long, value_name = "KEY")]
        payee_metadata_key: Option<String>,
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

/// Output format for validation results.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum CheckFormatArg {
    /// Human-readable validation output.
    Text,
    /// JSON validation output.
    Json,
}

/// Output format for the register listing.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum RegisterFormatArg {
    /// Human-readable register output.
    Text,
    /// JSON register output.
    Json,
}

/// Output format for the include graph.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum GraphFormatArg {
    /// Human-readable include-graph output.
    Text,
    /// JSON include-graph output.
    Json,
}

/// CLI-facing mirror of [`pactole_core::TransactionStatus`].
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum StatusArg {
    /// Uncleared transaction status.
    Uncleared,
    /// Pending transaction status.
    Pending,
    /// Cleared transaction status.
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

/// Output format of the reporting commands.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum FormatArg {
    /// Human-readable report output.
    Text,
    /// JSON report output.
    Json,
    /// CSV report output.
    Csv,
}

impl From<FormatArg> for pactole_core::ReportFormat {
    fn from(value: FormatArg) -> Self {
        match value {
            FormatArg::Text => pactole_core::ReportFormat::Text,
            FormatArg::Json => pactole_core::ReportFormat::Json,
            FormatArg::Csv => pactole_core::ReportFormat::Csv,
        }
    }
}

/// Grouping for reports that can break down income and expenses.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum ByArg {
    /// Group by account.
    Account,
    /// Group by payee.
    Payee,
    /// Group by tag.
    Tag,
}

/// Grouping for the expenses breakdown.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum ExpensesByArg {
    /// Group by account.
    Account,
    /// Group by payee.
    Payee,
    /// Group by tag.
    Tag,
    /// Group by calendar month.
    Month,
}

/// Difference mode for comparison.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum DiffArg {
    /// Show an absolute difference.
    Absolute,
    /// Show a percentage difference.
    Percent,
}

/// Named comparison period.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum CompareArg {
    /// Compare with the immediately preceding period.
    Previous,
}

impl From<DiffArg> for pactole_core::DiffMode {
    fn from(value: DiffArg) -> Self {
        match value {
            DiffArg::Absolute => pactole_core::DiffMode::Absolute,
            DiffArg::Percent => pactole_core::DiffMode::Percent,
        }
    }
}
