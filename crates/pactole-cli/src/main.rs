mod check;
mod commands;
mod config;
mod diagnostics;
mod format;
mod import_ofx;
mod init;
mod parse;
mod register;

use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use clap_complete::generate;

use crate::commands::{Cli, Command};

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Command::Parse { file } => parse::run(file),
        Command::Fmt { file, write } => format::run(file, write),
        Command::Completions { shell } => {
            let mut command = Cli::command();
            generate(shell, &mut command, "pactole", &mut std::io::stdout());
            ExitCode::SUCCESS
        }
        Command::Check { file, strict } => check::run(file, strict),
        Command::Register {
            file,
            account,
            from,
            to,
            payee,
            narration,
            tag,
            status,
        } => register::run(file, account, from, to, payee, narration, tag, status),
        Command::ImportOfx {
            file,
            config,
            into,
            write,
        } => import_ofx::run(file, config, into, write),
        Command::Init {
            ledger,
            config,
            account,
            currency,
            counter_account,
            date,
            stdout,
            no_config,
            no_ledger,
        } => init::run(init::Options {
            ledger,
            config,
            account,
            currency,
            counter_account,
            date,
            stdout,
            no_config,
            no_ledger,
        }),
    }
}
