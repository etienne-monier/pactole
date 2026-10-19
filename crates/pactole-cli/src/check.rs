use std::path::{Path, PathBuf};
use std::process::ExitCode;

use pactole_config::ValidationOptions;
use pactole_storage_fs::{FsSourceLoader, SourceLoadError};

pub(crate) fn run(file: PathBuf, strict: bool, validation: &ValidationOptions) -> ExitCode {
    let project = match pactole_storage_fs::analyze_file_with_loader_with_options(
        &file,
        &FsSourceLoader,
        validation,
    ) {
        Ok(project) => project,
        Err(error) => return failure(&file, error),
    };

    let diagnostics = project.diagnostics();
    crate::diagnostics::print(&diagnostics);

    if project.has_errors() || (strict && project.has_warnings()) {
        ExitCode::FAILURE
    } else {
        println!("OK");
        ExitCode::SUCCESS
    }
}

fn failure(file: &Path, error: SourceLoadError) -> ExitCode {
    eprintln!("{}: error: {error}", file.display());
    ExitCode::FAILURE
}
