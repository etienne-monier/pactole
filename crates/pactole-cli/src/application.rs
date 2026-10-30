use std::path::Path;
use std::process::ExitCode;

use pactole_app::{Application, ApplicationError};
use pactole_config::ValidationOptions;
use pactole_storage_fs::FsLedgerReader;

/// Filesystem-backed application service used by CLI commands.
pub(crate) type Service = Application<FsLedgerReader>;

/// Builds the application service backed by the filesystem ledger at `path`.
pub(crate) fn service(path: &Path, validation: &ValidationOptions) -> Service {
    Application::new(FsLedgerReader::new(path.to_path_buf(), *validation))
}

/// Prints an application error and returns the CLI failure exit code.
pub(crate) fn report_failure(error: ApplicationError) -> ExitCode {
    match error {
        ApplicationError::LedgerUnavailable { diagnostics } => {
            crate::diagnostics::print_application(&diagnostics);
        }
        error => eprintln!("error: {error}"),
    }
    ExitCode::FAILURE
}
