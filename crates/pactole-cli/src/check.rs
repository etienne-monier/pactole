use std::path::PathBuf;
use std::process::ExitCode;

use pactole_core::{Entry, ReadableStorage, ValidationError};
use pactole_storage_fs::{FsSourceLoader, PactoleFileStorage};

pub(crate) fn run(file: PathBuf) -> ExitCode {
    if let Ok(project) = pactole_storage_fs::analyze_file_with_loader(&file, &FsSourceLoader) {
        let has_analysis_errors = project.files().iter().fold(false, |has_errors, analyzed| {
            let syntax_errors = analyzed.file.syntax_diagnostics();
            for diagnostic in syntax_errors {
                print_location(
                    &analyzed.path,
                    diagnostic.span.start.row + 1,
                    diagnostic.span.start.column + 1,
                    &diagnostic.message,
                );
            }

            let lowering_errors = analyzed.file.lowering_diagnostics();
            for diagnostic in lowering_errors {
                print_location(
                    &analyzed.path,
                    diagnostic.span.start.row + 1,
                    diagnostic.span.start.column + 1,
                    &diagnostic.message,
                );
            }

            has_errors || !syntax_errors.is_empty() || !lowering_errors.is_empty()
        });

        let mut has_include_errors = false;
        for issue in project.include_issues() {
            print_location(
                &issue.path,
                issue.include.span.start.row + 1,
                issue.include.span.start.column + 1,
                &issue.message,
            );
            has_include_errors = true;
        }

        if has_analysis_errors || has_include_errors {
            return ExitCode::FAILURE;
        }
    }

    let mut storage = match PactoleFileStorage::try_from(file.clone()) {
        Ok(storage) => storage,
        Err(err) => return failure(&file, err),
    };

    if let Err(err) = storage.parse() {
        return failure(&file, err);
    }

    let journal = match storage.journal() {
        Ok(journal) => journal,
        Err(err) => return failure(&file, err),
    };

    let report = pactole_core::validate_journal_all(journal);
    if !report.errors.is_empty() {
        let project = pactole_storage_fs::analyze_file_with_loader(&file, &FsSourceLoader).ok();
        for error in &report.errors {
            let location = project
                .as_ref()
                .and_then(|project| locate_validation_error(project, error));
            print_error_at(&file, location, error);
        }
        return ExitCode::FAILURE;
    }

    println!("OK");
    ExitCode::SUCCESS
}

fn failure(file: &std::path::Path, error: impl std::fmt::Display) -> ExitCode {
    let message = error.to_string();
    if let Some((line, column)) = position_from_message(&message) {
        print_location(file, line, column, &message);
    } else {
        eprintln!("{}: error: {message}", file.display());
    }
    ExitCode::FAILURE
}

fn print_error_at(
    file: &std::path::Path,
    location: Option<(PathBuf, (usize, usize))>,
    error: impl std::fmt::Display,
) {
    let message = error.to_string();
    if let Some((path, (line, column))) = location {
        print_location(&path, line, column, &message);
    } else {
        eprintln!("{}: error: {message}", file.display());
    }
}

fn print_location(file: &std::path::Path, line: usize, column: usize, message: &str) {
    eprintln!("{}:{line}:{column}: error: {message}", file.display());
}

fn position_from_message(message: &str) -> Option<(usize, usize)> {
    let position = message.split_once("at ")?.1;
    let (line, column) = position.split_once(':')?;
    let column = column
        .split(|character: char| !character.is_ascii_digit())
        .next()?;
    Some((line.parse().ok()?, column.parse().ok()?))
}

fn locate_validation_error(
    project: &pactole_storage_fs::AnalyzedProject,
    error: &ValidationError,
) -> Option<(PathBuf, (usize, usize))> {
    project.files().iter().find_map(|file| {
        file.file
            .entries()
            .iter()
            .find(|entry| entry_matches_error(&entry.entry, error))
            .map(|entry| {
                (
                    file.path.clone(),
                    (entry.span.start.row + 1, entry.span.start.column + 1),
                )
            })
    })
}

fn entry_matches_error(entry: &Entry, error: &ValidationError) -> bool {
    match error {
        ValidationError::AccountAlreadyOpen { account, date } => {
            matches!(entry, Entry::Open(open) if open.account == *account && open.date == *date)
        }
        ValidationError::AccountNotOpen { account, date } => match entry {
            Entry::Open(open) => open.account == *account && open.date == *date,
            Entry::Close(close) => close.account == *account && close.date == *date,
            Entry::Balance(balance) => balance.account == *account && balance.date == *date,
            Entry::Transaction(transaction) => {
                transaction.date == *date
                    && transaction
                        .postings
                        .iter()
                        .any(|posting| posting.account == *account)
            }
            _ => false,
        },
        ValidationError::CommodityAlreadyDeclared { commodity, date }
        | ValidationError::CommodityNotDeclared { commodity, date } => match entry {
            Entry::Commodity(declaration) => {
                declaration.name == *commodity && declaration.date == *date
            }
            Entry::Balance(balance) => {
                balance.amount.commodity == *commodity && balance.date == *date
            }
            Entry::Transaction(transaction) => {
                transaction.date == *date
                    && transaction.postings.iter().any(|posting| {
                        posting
                            .amount
                            .as_ref()
                            .is_some_and(|amount| amount.commodity == *commodity)
                    })
            }
            _ => false,
        },
        ValidationError::PayeeAlreadyDeclared { payee } => {
            matches!(entry, Entry::Payee(declaration) if declaration.name == *payee)
        }
        ValidationError::PayeeNotDeclared { payee, date } => {
            matches!(entry, Entry::Transaction(transaction) if transaction.date == *date && transaction.payee.as_deref() == Some(payee))
        }
        ValidationError::TooManyPostingsWithoutAmount { date, .. }
        | ValidationError::AmbiguousAutoBalanceCommodity { date, .. }
        | ValidationError::TransactionNotBalanced { date, .. } => {
            matches!(entry, Entry::Transaction(transaction) if transaction.date == *date)
        }
        ValidationError::CommodityNotAllowed {
            account,
            commodity,
            date,
        } => match entry {
            Entry::Balance(balance) => {
                balance.account == *account
                    && balance.amount.commodity == *commodity
                    && balance.date == *date
            }
            Entry::Transaction(transaction) => {
                transaction.date == *date
                    && transaction.postings.iter().any(|posting| {
                        posting.account == *account
                            && posting
                                .amount
                                .as_ref()
                                .is_some_and(|amount| amount.commodity == *commodity)
                    })
            }
            _ => false,
        },
        ValidationError::BalanceAssertionFailed {
            account,
            commodity,
            date,
            ..
        } => {
            matches!(entry, Entry::Balance(balance) if balance.account == *account && balance.amount.commodity == *commodity && balance.date == *date)
        }
    }
}
