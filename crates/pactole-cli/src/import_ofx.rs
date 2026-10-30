use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chrono::NaiveDate;
use pactole_core::Entry;
use pactole_storage_fs::FsSourceLoader;

use pactole_config::Config;

/// Options controlling one OFX import invocation.
pub(crate) struct Options {
    /// Import transactions on or after this date.
    pub(crate) from: Option<NaiveDate>,
    /// Existing ledger to inspect and optionally append to.
    pub(crate) into: Option<PathBuf>,
    /// Overrides the configured bank account.
    pub(crate) account: Option<String>,
    /// Appends generated transactions to the target ledger.
    pub(crate) write: bool,
    /// Suppresses source-payee metadata.
    pub(crate) no_imported_payee: bool,
    /// Omits negative posting amounts in generated output.
    pub(crate) omit_negative_amount: bool,
}

/// Imports an OFX statement and prints or appends canonical Pactole transactions.
pub(crate) fn run(file: PathBuf, options: Options, config: &Config) -> ExitCode {
    let Options {
        from,
        into,
        account,
        write,
        no_imported_payee,
        omit_negative_amount,
    } = options;
    let Some(mut import_config) = config.import.clone() else {
        eprintln!(
            "error: no `[import]` section configured; add one to `pactole.toml` \
             (see `pactole init`)"
        );
        return ExitCode::FAILURE;
    };

    // `--account` overrides the configured bank account for this run: every
    // imported transaction is posted to it, whatever the statement accounts
    // are, so the `[import.accounts]` mapping is dropped accordingly.
    if let Some(account) = account {
        import_config.bank_account = account;
        import_config.accounts.clear();
    }

    let mut known_fitids = HashSet::new();
    let mut existing = None;
    if let Some(path) = &into {
        match fs::read_to_string(path) {
            Ok(source) => {
                known_fitids = match collect_fitids(path) {
                    Ok(fitids) => fitids,
                    Err(err) => {
                        eprintln!("error: cannot analyze {}: {err}", path.display());
                        return ExitCode::FAILURE;
                    }
                };
                existing = Some(source);
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                eprintln!("error: cannot read {}: {err}", path.display());
                return ExitCode::FAILURE;
            }
        }
    }

    // OFX sources are not necessarily UTF-8: decode the raw bytes according
    // to the encoding the document declares.
    let source = match fs::read(&file) {
        Ok(bytes) => pactole_import::decode_ofx_source(&bytes),
        Err(err) => {
            eprintln!("error: cannot read {}: {err}", file.display());
            return ExitCode::FAILURE;
        }
    };

    let outcome = match pactole_import::import_ofx_with_options(
        &source,
        &import_config,
        &known_fitids,
        pactole_import::ImportOptions {
            from,
            no_imported_payee: no_imported_payee.then_some(true),
            omit_negative_amount: omit_negative_amount.then_some(true),
        },
    ) {
        Ok(outcome) => outcome,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    };

    for warning in &outcome.warnings {
        eprintln!("warning: {}", describe_warning(warning, &import_config));
    }

    let generated =
        pactole_storage_fs::format_transactions_with_options(&outcome.transactions, &config.format);

    if write {
        let path = into.as_deref().unwrap_or_else(|| Path::new(""));
        let mut content = existing.unwrap_or_default();
        if !content.is_empty() {
            if !content.ends_with('\n') {
                content.push('\n');
            }
            content.push('\n');
        }
        content.push_str(&generated);

        if let Err(err) = fs::write(path, content) {
            eprintln!("error: cannot write {}: {err}", path.display());
            return ExitCode::FAILURE;
        }
    } else {
        print!("{generated}");
    }

    eprintln!(
        "imported {} transaction(s), skipped {}",
        outcome.transactions.len(),
        outcome.skipped
    );

    ExitCode::SUCCESS
}

/// Collects FITIDs from the complete include graph of an existing ledger.
fn collect_fitids(path: &Path) -> Result<HashSet<String>, pactole_storage_fs::SourceLoadError> {
    // Analyze `path` as the entry point of a project so `include` directives
    // are followed: FITIDs recorded in any reachable file are recognized and
    // re-imports stay idempotent even when the target journal is split across
    // several files.
    let project = pactole_storage_fs::analyze_file_with_loader(path, &FsSourceLoader)?;

    Ok(project
        .files()
        .iter()
        .flat_map(|file| file.file.entries())
        .flat_map(|parsed_entry| match &parsed_entry.entry {
            Entry::Transaction(transaction) => transaction
                .postings
                .iter()
                .filter_map(|posting| posting.meta.get("fitid").cloned())
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect())
}

/// Formats an import warning with the configured fallback account.
fn describe_warning(
    warning: &pactole_import::ImportWarning,
    config: &pactole_import::ImportConfig,
) -> String {
    match warning {
        pactole_import::ImportWarning::MissingPayee { fitid } => match fitid {
            Some(fitid) => {
                format!("transaction `{fitid}` has no payee; using the `Unknown` placeholder")
            }
            None => "a transaction has no payee; using the `Unknown` placeholder".to_string(),
        },
        pactole_import::ImportWarning::UnmappedAccount { account_id } => format!(
            "statement account `{account_id}` has no `[import.accounts]` mapping; using `{}` \
             (`bank_account`)",
            config.bank_account
        ),
    }
}
