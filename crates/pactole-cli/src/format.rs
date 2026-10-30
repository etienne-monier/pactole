use std::fs;
use std::io::Read;
use std::process::ExitCode;

use pactole_config::FormatOptions;

/// Formats a ledger from a file or stdin and optionally writes it back.
pub(crate) fn run(file: String, write: bool, options: &FormatOptions) -> ExitCode {
    let source = if file == "-" {
        let mut buf = String::new();
        if let Err(err) = std::io::stdin().read_to_string(&mut buf) {
            return failure(err);
        }
        buf
    } else {
        match fs::read_to_string(&file) {
            Ok(source) => source,
            Err(err) => return failure(err),
        }
    };

    let formatted = match pactole_storage_fs::format_with_options(&source, options) {
        Ok(formatted) => formatted,
        Err(err) => return failure(err),
    };

    if write && file != "-" {
        if let Err(err) = fs::write(&file, formatted) {
            return failure(err);
        }
    } else {
        print!("{formatted}");
    }

    ExitCode::SUCCESS
}

/// Prints a formatting error and returns the failure exit code.
fn failure(error: impl std::fmt::Display) -> ExitCode {
    eprintln!("error: {error}");
    ExitCode::FAILURE
}
