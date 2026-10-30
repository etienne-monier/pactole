use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::process::ExitCode;

use chrono::{Datelike, Local, NaiveDate};
use pactole_app::{ComparisonRequest, ExpensesQuery, ValuationRequest};
use pactole_config::{ReportOptions, ValidationOptions};
use pactole_core::{
    AccountGrouping, AccountName, DiffDirection, ExpenseGrouping, ExpenseOptions, Period,
    ReportError, previous_period,
};

use crate::classification::classification;
use crate::commands::{CompareArg, DiffArg, ExpensesByArg, FormatArg};

#[allow(clippy::too_many_arguments)]
/// Runs the expense-breakdown command and optional period comparison.
pub(crate) fn run(
    file: PathBuf,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    by: ExpensesByArg,
    format: FormatArg,
    depth: Option<NonZeroUsize>,
    uncategorized: bool,
    inactive: bool,
    compare: Option<CompareArg>,
    compare_from: Option<NaiveDate>,
    compare_to: Option<NaiveDate>,
    diff: DiffArg,
    reverse_diff: bool,
    convert_to: Option<String>,
    at: Option<NaiveDate>,
    validation: &ValidationOptions,
    reports: &ReportOptions,
    import_config: Option<&pactole_config::ImportConfig>,
) -> ExitCode {
    let comparison_requested = compare.is_some() || compare_from.is_some() || compare_to.is_some();
    if !comparison_requested && (reverse_diff || matches!(diff, DiffArg::Percent)) {
        eprintln!("error: --diff and --reverse-diff require a comparison");
        return ExitCode::FAILURE;
    }
    if compare.is_some() && (compare_from.is_some() || compare_to.is_some()) {
        eprintln!("error: --compare cannot be combined with --compare-from or --compare-to");
        return ExitCode::FAILURE;
    }
    if compare_from.is_some() != compare_to.is_some() {
        eprintln!("error: --compare-from and --compare-to must be provided together");
        return ExitCode::FAILURE;
    }
    if depth.is_some() && !matches!(by, ExpensesByArg::Account) {
        eprintln!("error: --depth can only be used with --by account");
        return ExitCode::FAILURE;
    }

    let to_date = to.unwrap_or_else(|| Local::now().date_naive());
    let from_date =
        from.unwrap_or_else(|| NaiveDate::from_ymd_opt(to_date.year(), 1, 1).unwrap_or(to_date));

    let period = match Period::new(from_date, to_date) {
        Ok(period) => period,
        Err(ReportError::InvalidPeriod { .. }) => {
            eprintln!("error: invalid period: --from must be on or before --to");
            return ExitCode::FAILURE;
        }
        Err(error) => return failure(error),
    };

    let grouping = match by {
        ExpensesByArg::Account => ExpenseGrouping::Account(match depth {
            Some(depth) => AccountGrouping::Depth(depth),
            None => AccountGrouping::Full,
        }),
        ExpensesByArg::Payee => ExpenseGrouping::Payee,
        ExpensesByArg::Tag => ExpenseGrouping::Tag,
        ExpensesByArg::Month => ExpenseGrouping::Month,
    };

    let mut uncategorized_accounts: Vec<AccountName> = Vec::new();
    if uncategorized {
        if let Some(list) = &reports.uncategorized {
            for name in list {
                match AccountName::new(name) {
                    Ok(account) => uncategorized_accounts.push(account),
                    Err(error) => {
                        eprintln!("error: invalid account in [reports] uncategorized: {error}");
                        return ExitCode::FAILURE;
                    }
                }
            }
        } else {
            if let Some(import) = import_config {
                match AccountName::new(&import.default_counter_account) {
                    Ok(account) => uncategorized_accounts.push(account),
                    Err(error) => {
                        eprintln!("error: invalid import.default_counter_account: {error}");
                        return ExitCode::FAILURE;
                    }
                }
            }
        }
        if uncategorized_accounts.is_empty() {
            eprintln!(
                "error: --uncategorized requires [reports] uncategorized or [import] default_counter_account"
            );
            return ExitCode::FAILURE;
        }
    }

    let options = ExpenseOptions {
        uncategorized: uncategorized_accounts,
        include_uncategorized: uncategorized,
        include_inactive: inactive,
    };
    let comparison = if comparison_requested {
        if matches!(compare, Some(CompareArg::Previous)) {
            let previous = match previous_period(period) {
                Some(previous) => previous,
                None => {
                    eprintln!("error: cannot calculate the previous period at this date");
                    return ExitCode::FAILURE;
                }
            };
            Some(previous)
        } else if let (Some(cf), Some(ct)) = (compare_from, compare_to) {
            match Period::new(cf, ct) {
                Ok(previous) => Some(previous),
                Err(_) => {
                    eprintln!("error: invalid comparison period");
                    return ExitCode::FAILURE;
                }
            }
        } else {
            eprintln!("error: use --compare previous or provide --compare-from and --compare-to");
            return ExitCode::FAILURE;
        }
    } else {
        None
    };
    let comparison = comparison.map(|period| ComparisonRequest {
        period,
        direction: if reverse_diff {
            DiffDirection::PreviousMinusCurrent
        } else {
            DiffDirection::CurrentMinusPrevious
        },
        mode: diff.into(),
    });
    let valuation = match convert_to {
        Some(target) => Some(ValuationRequest {
            target: match pactole_core::CommodityName::new(target) {
                Ok(target) => target,
                Err(error) => return failure(error),
            },
            at: at.unwrap_or(period.end()),
        }),
        None => {
            if at.is_some() {
                return failure("--at requires --convert-to");
            }
            None
        }
    };
    let result = match crate::application::service(&file, validation).expenses(ExpensesQuery {
        period,
        grouping,
        options,
        classification: classification(reports),
        comparison,
        valuation,
    }) {
        Ok(result) => result,
        Err(error) => return crate::application::report_failure(error),
    };
    crate::diagnostics::print_application(&result.diagnostics);
    if let Some(comparison) = result.comparison {
        print!("{}", comparison.render(format.into()));
    } else {
        print!("{}", result.current.render(format.into()));
    }

    ExitCode::SUCCESS
}

/// Prints an expense-command error and returns the failure exit code.
fn failure(error: impl std::fmt::Display) -> ExitCode {
    eprintln!("error: {error}");
    ExitCode::FAILURE
}
