use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::process::ExitCode;

use chrono::{Local, NaiveDate};
use pactole_config::{ReportOptions, ValidationOptions};
use pactole_core::{AccountGrouping, ReferenceDate, balance_sheet};
use pactole_storage_fs::FsSourceLoader;

use crate::classification::classification;
use crate::commands::FormatArg;

pub(crate) fn run(
    file: PathBuf,
    date: Option<NaiveDate>,
    format: FormatArg,
    depth: Option<NonZeroUsize>,
    validation: &ValidationOptions,
    reports: &ReportOptions,
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

    let as_of = ReferenceDate::new(date.unwrap_or_else(|| Local::now().date_naive()));
    let grouping = match depth {
        Some(depth) => AccountGrouping::Depth(depth),
        None => AccountGrouping::Full,
    };

    let report = balance_sheet(&journal, as_of, &classification(reports), grouping);
    print!("{}", report.render(format.into()));

    ExitCode::SUCCESS
}

fn failure(error: impl std::fmt::Display) -> ExitCode {
    eprintln!("error: {error}");
    ExitCode::FAILURE
}
