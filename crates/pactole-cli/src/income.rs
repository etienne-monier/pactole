use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::process::ExitCode;

use chrono::{Datelike, Local, NaiveDate};
use pactole_app::{IncomeQuery, ValuationRequest};
use pactole_config::{ReportOptions, ValidationOptions};
use pactole_core::{AccountGrouping, IncomeGrouping, Period, ReportError};

use crate::classification::classification;
use crate::commands::{ByArg, FormatArg};

#[allow(clippy::too_many_arguments)]
/// Runs the income-statement command and renders its report.
pub(crate) fn run(
    file: PathBuf,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    by: ByArg,
    format: FormatArg,
    depth: Option<NonZeroUsize>,
    convert_to: Option<String>,
    at: Option<NaiveDate>,
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
        Err(error) => return failure(error),
    };

    let grouping = match by {
        ByArg::Account => IncomeGrouping::Account(match depth {
            Some(depth) => AccountGrouping::Depth(depth),
            None => AccountGrouping::Full,
        }),
        ByArg::Payee => IncomeGrouping::Payee,
        ByArg::Tag => IncomeGrouping::Tag,
    };

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
    let result = match crate::application::service(&file, validation).income(IncomeQuery {
        period,
        grouping,
        classification: classification(reports),
        valuation,
    }) {
        Ok(result) => result,
        Err(error) => return crate::application::report_failure(error),
    };
    crate::diagnostics::print_application(&result.diagnostics);
    print!("{}", result.report.render(format.into()));

    ExitCode::SUCCESS
}

/// Prints an income-command error and returns the failure exit code.
fn failure(error: impl std::fmt::Display) -> ExitCode {
    eprintln!("error: {error}");
    ExitCode::FAILURE
}
