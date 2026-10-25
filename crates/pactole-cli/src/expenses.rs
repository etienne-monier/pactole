use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::process::ExitCode;

use chrono::{Datelike, Local, NaiveDate};
use pactole_config::{ReportOptions, ValidationOptions};
use pactole_core::{
    AccountGrouping, AccountName, DiffDirection, DiffMode, ExpenseGrouping, ExpenseOptions, Period,
    ReportError, compare_reports, expense_breakdown_with_options, periods_have_same_length,
    previous_period,
};
use pactole_storage_fs::FsSourceLoader;

use crate::classification::classification;
use crate::commands::{CompareArg, DiffArg, ExpensesByArg, FormatArg};

#[allow(clippy::too_many_arguments)]
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
    };

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
    let report = expense_breakdown_with_options(
        &journal,
        period,
        &classification(reports),
        grouping,
        &options,
    );

    if comparison_requested {
        if matches!(by, ExpensesByArg::Month) {
            eprintln!("error: --compare cannot be combined with --by month");
            return ExitCode::FAILURE;
        }
        if uncategorized || inactive {
            eprintln!("error: --compare cannot be combined with --uncategorized or --inactive");
            return ExitCode::FAILURE;
        }
        let previous = if matches!(compare, Some(CompareArg::Previous)) {
            match previous_period(period) {
                Some(previous) => previous,
                None => {
                    eprintln!("error: cannot calculate the previous period at this date");
                    return ExitCode::FAILURE;
                }
            }
        } else if let (Some(cf), Some(ct)) = (compare_from, compare_to) {
            match Period::new(cf, ct) {
                Ok(p) => p,
                Err(_) => {
                    eprintln!("error: invalid comparison period");
                    return ExitCode::FAILURE;
                }
            }
        } else {
            eprintln!("error: use --compare previous or provide --compare-from and --compare-to");
            return ExitCode::FAILURE;
        };
        if !periods_have_same_length(period, previous) {
            eprintln!("error: comparison periods must have the same length");
            return ExitCode::FAILURE;
        }
        let previous_report = expense_breakdown_with_options(
            &journal,
            previous,
            &classification(reports),
            grouping,
            &options,
        );
        let direction = if reverse_diff {
            DiffDirection::PreviousMinusCurrent
        } else {
            DiffDirection::CurrentMinusPrevious
        };
        let mode: DiffMode = diff.into();
        let comp = compare_reports(&report, &previous_report, direction, mode);
        print!("{}", comp.render(format.into()));
        return ExitCode::SUCCESS;
    }

    print!("{}", report.render(format.into()));

    ExitCode::SUCCESS
}

fn failure(error: impl std::fmt::Display) -> ExitCode {
    eprintln!("error: {error}");
    ExitCode::FAILURE
}
