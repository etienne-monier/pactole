use std::path::PathBuf;
use std::process::ExitCode;

use pactole_core::ReadableStorage;
use pactole_storage_fs::PactoleFileStorage;

pub(crate) fn run(file: PathBuf) -> ExitCode {
    let mut storage = match PactoleFileStorage::try_from(file) {
        Ok(storage) => storage,
        Err(err) => return failure(err),
    };

    if let Err(err) = storage.parse() {
        return failure(err);
    }

    let journal = match storage.journal() {
        Ok(journal) => journal,
        Err(err) => return failure(err),
    };

    if let Err(err) = pactole_core::validate_journal(journal) {
        return failure(err);
    }

    println!("OK");
    ExitCode::SUCCESS
}

fn failure(error: impl std::fmt::Display) -> ExitCode {
    eprintln!("error: {error}");
    ExitCode::FAILURE
}
