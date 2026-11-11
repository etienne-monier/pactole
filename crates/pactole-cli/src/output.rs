use std::io::{self, Write};
use std::process::ExitCode;

/// Writes command output to stdout without treating a closed pipe as an error.
pub(crate) fn write_stdout(output: &str) -> ExitCode {
    let result = io::stdout().lock().write_all(output.as_bytes());

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: failed to write stdout: {error}");
            ExitCode::FAILURE
        }
    }
}
