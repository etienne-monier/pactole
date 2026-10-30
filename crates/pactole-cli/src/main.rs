#![warn(missing_docs)]
#![warn(clippy::missing_docs_in_private_items)]

//! Command-line interface for Pactole ledger operations.

/// Shared application-service construction and error handling.
mod application;
/// Balance-sheet command implementation.
mod balance;
/// Ledger validation command implementation.
mod check;
/// Report account classification helpers.
mod classification;
/// Clap command-line argument definitions.
mod commands;
/// Layered configuration loading for CLI commands.
mod config;
/// Diagnostic rendering for terminal output.
mod diagnostics;
/// Expense-report command implementation.
mod expenses;
/// Canonical ledger formatting command implementation.
mod format;
/// Include-graph command implementation.
mod graph;
/// Actual Budget migration command implementation.
mod import_actual;
/// OFX import command implementation.
mod import_ofx;
/// Income-report command implementation.
mod income;
/// Starter-ledger initialization command implementation.
mod init;
/// Debug parsing command implementation.
mod parse;
/// Register-report command implementation.
mod register;
/// Report valuation rendering helpers.
mod valuation;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use clap_complete::generate;

use crate::commands::{Cli, Command};

fn main() -> ExitCode {
    let Cli {
        config: explicit_config,
        command,
    } = Cli::parse();

    match command {
        Command::Parse { file } => parse::run(file),
        Command::Fmt { file, write } => {
            let project_dir = file_project_dir(&file);
            match config::load(explicit_config.as_deref(), project_dir.as_deref()) {
                Ok(config) => format::run(file, write, &config.format),
                Err(error) => config::failure(error),
            }
        }
        Command::Completions { shell } => {
            let mut command = Cli::command();
            generate(shell, &mut command, "pactole", &mut std::io::stdout());
            ExitCode::SUCCESS
        }
        Command::Serve { file, bind, port } => {
            let project_dir = parent_dir(&file);
            match config::load(explicit_config.as_deref(), project_dir.as_deref()) {
                Ok(config) => {
                    let reader = pactole_storage_fs::FsLedgerReader::new(&file, config.validation);
                    let application = pactole_app::Application::new(reader);
                    let options = pactole_web::ServerOptions {
                        bind,
                        port,
                        source_root: project_dir,
                        reports: config.reports,
                        import: config.import,
                    };
                    match tokio::runtime::Builder::new_multi_thread()
                        .enable_all()
                        .build()
                    {
                        Ok(runtime) => {
                            match runtime.block_on(pactole_web::serve(application, options)) {
                                Ok(()) => ExitCode::SUCCESS,
                                Err(error) => {
                                    eprintln!("error: {error}");
                                    ExitCode::FAILURE
                                }
                            }
                        }
                        Err(error) => {
                            eprintln!("error: could not start async runtime: {error}");
                            ExitCode::FAILURE
                        }
                    }
                }
                Err(error) => config::failure(error),
            }
        }
        Command::Check {
            file,
            strict,
            format,
        } => {
            let project_dir = parent_dir(&file);
            match config::load(explicit_config.as_deref(), project_dir.as_deref()) {
                Ok(config) => check::run(file, strict, format, &config.validation),
                Err(error) => config::failure(error),
            }
        }
        Command::Graph { file, scan, format } => {
            let scan = scan.or_else(|| parent_dir(&file));
            graph::run(file, scan, format)
        }
        Command::Register {
            file,
            account,
            from,
            to,
            payee,
            narration,
            tag,
            status,
            format,
            convert_to,
            at,
        } => {
            let project_dir = parent_dir(&file);
            match config::load(explicit_config.as_deref(), project_dir.as_deref()) {
                Ok(config) => register::run(
                    file,
                    account,
                    from,
                    to,
                    payee,
                    narration,
                    tag,
                    status,
                    format,
                    convert_to,
                    at,
                    &config.validation,
                ),
                Err(error) => config::failure(error),
            }
        }
        Command::Balance {
            file,
            date,
            format,
            depth,
            convert_to,
            at,
        } => {
            let project_dir = parent_dir(&file);
            match config::load(explicit_config.as_deref(), project_dir.as_deref()) {
                Ok(config) => balance::run(
                    file,
                    date,
                    format,
                    depth,
                    convert_to,
                    at,
                    &config.validation,
                    &config.reports,
                ),
                Err(error) => config::failure(error),
            }
        }
        Command::Income {
            file,
            from,
            to,
            by,
            format,
            depth,
            convert_to,
            at,
        } => {
            let project_dir = parent_dir(&file);
            match config::load(explicit_config.as_deref(), project_dir.as_deref()) {
                Ok(config) => income::run(
                    file,
                    from,
                    to,
                    by,
                    format,
                    depth,
                    convert_to,
                    at,
                    &config.validation,
                    &config.reports,
                ),
                Err(error) => config::failure(error),
            }
        }
        Command::Expenses {
            file,
            from,
            to,
            by,
            format,
            depth,
            uncategorized,
            inactive,
            compare,
            compare_from,
            compare_to,
            diff,
            reverse_diff,
            convert_to,
            at,
        } => {
            let project_dir = parent_dir(&file);
            match config::load(explicit_config.as_deref(), project_dir.as_deref()) {
                Ok(config) => expenses::run(
                    file,
                    from,
                    to,
                    by,
                    format,
                    depth,
                    uncategorized,
                    inactive,
                    compare,
                    compare_from,
                    compare_to,
                    diff,
                    reverse_diff,
                    convert_to,
                    at,
                    &config.validation,
                    &config.reports,
                    config.import.as_ref(),
                ),
                Err(error) => config::failure(error),
            }
        }
        Command::ImportOfx {
            file,
            from,
            into,
            account,
            write,
            no_imported_payee,
            omit_negative_amount,
        } => {
            let project_dir = into
                .as_deref()
                .and_then(parent_dir)
                .or_else(|| std::env::current_dir().ok());
            match config::load(explicit_config.as_deref(), project_dir.as_deref()) {
                Ok(config) => import_ofx::run(
                    file,
                    import_ofx::Options {
                        from,
                        into,
                        account,
                        write,
                        no_imported_payee,
                        omit_negative_amount,
                    },
                    &config,
                ),
                Err(error) => config::failure(error),
            }
        }
        Command::ImportActual {
            file,
            output,
            currency,
            opening_equity,
            opening_date,
            payee_metadata_key,
        } => import_actual::run(
            file,
            output,
            currency,
            opening_equity,
            opening_date,
            payee_metadata_key,
        ),
        Command::Init {
            ledger,
            account,
            currency,
            counter_account,
            date,
            stdout,
            global,
            no_config,
            no_ledger,
        } => init::run(init::Options {
            ledger,
            account,
            currency,
            counter_account,
            date,
            stdout,
            global,
            no_config,
            no_ledger,
        }),
    }
}

/// Directory of the journal a command operates on, used to locate the
/// project `pactole.toml` layer.
fn parent_dir(path: &Path) -> Option<PathBuf> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Some(parent.to_path_buf())
}

/// Directory of the journal being formatted, or the current directory when
/// formatting stdin (`-`).
fn file_project_dir(file: &str) -> Option<PathBuf> {
    if file == "-" {
        std::env::current_dir().ok()
    } else {
        parent_dir(Path::new(file))
    }
}
