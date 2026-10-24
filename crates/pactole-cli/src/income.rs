use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::process::ExitCode;

use chrono::{Datelike, Local, NaiveDate};
use pactole_config::{ReportOptions, ValidationOptions};
use pactole_core::{AccountGrouping, IncomeGrouping, Period, ReportError, income_statement};
use pactole_storage_fs::FsSourceLoader;

use crate::classification::classification;
use crate::commands::{ByArg, FormatArg};

#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    file: PathBuf,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    by: ByArg,
    format: FormatArg,
    depth: Option<NonZeroUsize>,
    validation: &ValidationOptions,
    reports: &ReportOptions,
) -> ExitCode {
    if depth.is_some() && !matches!(by, ByArg::Account) {
        eprintln!("error: --depth can only be used with --by account");
        return ExitCode::FAILURE;
    }

    let to_date = to.unwrap_or_else(|| Local::now().date_naive());
    let from_date = from.unwrap_or_else(|| NaiveDate::from_ymd_opt(to_date.year(), 1, 1).unwrap());

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
        ByArg::Account => IncomeGrouping::Account(match depth {
            Some(depth) => AccountGrouping::Depth(depth),
            None => AccountGrouping::Full,
        }),
        ByArg::Payee => IncomeGrouping::Payee,
        ByArg::Tag => IncomeGrouping::Tag,
    };

    let report = income_statement(&journal, period, &classification(reports), grouping);
    print!("{}", report.render(format.into()));

    ExitCode::SUCCESS
}

fn failure(error: impl std::fmt::Display) -> ExitCode {
    eprintln!("error: {error}");
    ExitCode::FAILURE
}
