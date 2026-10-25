mod balance;
mod check;
mod classification;
mod commands;
mod config;
mod diagnostics;
mod expenses;
mod format;
mod graph;
mod import_actual;
mod import_ofx;
mod income;
mod init;
mod parse;
mod register;

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
        } => {
            let project_dir = parent_dir(&file);
            match config::load(explicit_config.as_deref(), project_dir.as_deref()) {
                Ok(config) => balance::run(
                    file,
                    date,
                    format,
                    depth,
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
                    &config.validation,
                    &config.reports,
                    config.import.as_ref(),
                ),
                Err(error) => config::failure(error),
            }
        }
        Command::ImportOfx {
            file,
            into,
            account,
            write,
        } => {
            let project_dir = into
                .as_deref()
                .and_then(parent_dir)
                .or_else(|| std::env::current_dir().ok());
            match config::load(explicit_config.as_deref(), project_dir.as_deref()) {
                Ok(config) => import_ofx::run(file, into, account, write, &config),
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
    Some(path.parent().unwrap_or(Path::new("")).to_path_buf())
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
