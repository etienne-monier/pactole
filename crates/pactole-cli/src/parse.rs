use std::path::PathBuf;
use std::process::ExitCode;

use pactole_storage_fs::{FsSourceLoader, SourceLoadError};

/// Parses a project and prints its lowered journal entries.
pub(crate) fn run(file: PathBuf) -> ExitCode {
    let project = match pactole_storage_fs::analyze_file_with_loader(&file, &FsSourceLoader) {
        Ok(project) => project,
        Err(error) => return failure(error),
    };

    let structural = project.structural_diagnostics();
    crate::diagnostics::print(&structural);
    let journal = match project.journal() {
        Ok(journal) => journal,
        Err(_) => return ExitCode::FAILURE,
    };

    for entry in journal.entries() {
        println!("{entry:#?}");
    }

    ExitCode::SUCCESS
}

/// Prints a source-loading error and returns the failure exit code.
fn failure(error: SourceLoadError) -> ExitCode {
    eprintln!("error: {error}");
    ExitCode::FAILURE
}
