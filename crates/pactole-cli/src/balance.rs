use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::process::ExitCode;

use chrono::{Local, NaiveDate};
use pactole_app::{BalanceQuery, ValuationRequest};
use pactole_config::{ReportOptions, ValidationOptions};
use pactole_core::{AccountGrouping, ReferenceDate};

use crate::classification::classification;
use crate::commands::FormatArg;

#[allow(clippy::too_many_arguments)]
/// Runs the balance-sheet command and renders its report.
pub(crate) fn run(
    file: PathBuf,
    date: Option<NaiveDate>,
    format: FormatArg,
    depth: Option<NonZeroUsize>,
    convert_to: Option<String>,
    at: Option<NaiveDate>,
    validation: &ValidationOptions,
    reports: &ReportOptions,
) -> ExitCode {
    let as_of = ReferenceDate::new(date.unwrap_or_else(|| Local::now().date_naive()));
    let grouping = match depth {
        Some(depth) => AccountGrouping::Depth(depth),
        None => AccountGrouping::Full,
    };

    let valuation = match convert_to {
        Some(target) => Some(ValuationRequest {
            target: match pactole_core::CommodityName::new(target) {
                Ok(target) => target,
                Err(error) => return failure(error),
            },
            at: at.unwrap_or(as_of.date()),
        }),
        None => {
            if at.is_some() {
                return failure("--at requires --convert-to");
            }
            None
        }
    };
    let result = match crate::application::service(&file, validation).balance(BalanceQuery {
        as_of,
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

/// Prints a command-level error and returns the failure exit code.
fn failure(error: impl std::fmt::Display) -> ExitCode {
    eprintln!("error: {error}");
    ExitCode::FAILURE
}
