use std::path::PathBuf;
use std::process::ExitCode;

use chrono::NaiveDate;
use pactole_core::ReadableStorage;
use pactole_core::{RegisterFilter, TransactionStatus};
use pactole_storage_fs::PactoleFileStorage;

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
) -> ExitCode {
    let mut storage = match PactoleFileStorage::try_from(file) {
        Ok(storage) => storage,
        Err(err) => return failure(err),
    };

    if let Err(err) = storage.parse() {
        return failure(err);
    }

    let journal = match storage.journal() {
        Ok(journal) => journal,
        Err(err) => return failure(err),
    };

    let journal = match pactole_core::validate_journal(journal) {
        Ok(journal) => journal,
        Err(err) => return failure(err),
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
