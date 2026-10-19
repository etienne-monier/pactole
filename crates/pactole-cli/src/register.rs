use std::path::PathBuf;
use std::process::ExitCode;

use chrono::NaiveDate;
use pactole_config::ValidationOptions;
use pactole_core::{RegisterFilter, TransactionStatus};
use pactole_storage_fs::FsSourceLoader;

use crate::commands::StatusArg;

#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    file: PathBuf,
    account: Option<String>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    payee: Option<String>,
    narration: Option<String>,
    tag: Option<String>,
    status: Option<StatusArg>,
    validation: &ValidationOptions,
) -> ExitCode {
    let project = match pactole_storage_fs::analyze_file_with_loader_with_options(
        &file,
        &FsSourceLoader,
        validation,
    ) {
        Ok(project) => project,
        Err(err) => return failure(err),
    };

    let diagnostics = project.diagnostics();
    crate::diagnostics::print(&diagnostics);
    let journal = match project.validated_journal() {
        Ok(journal) => journal,
        Err(_) => return ExitCode::FAILURE,
    };

    let account = match account {
        Some(account) => match pactole_core::AccountName::new(account) {
            Ok(account) => Some(account),
            Err(err) => return failure(err),
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

fn failure(error: impl std::fmt::Display) -> ExitCode {
    eprintln!("error: {error}");
    ExitCode::FAILURE
}
