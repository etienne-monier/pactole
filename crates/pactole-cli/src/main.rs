mod check;
mod commands;
mod config;
mod format;
mod import_ofx;
mod init;
mod parse;
mod register;

use std::process::ExitCode;

use clap::Parser;

use crate::commands::{Cli, Command};

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Command::Parse { file } => parse::run(file),
        Command::Fmt { file, write } => format::run(file, write),
        Command::Check { file } => check::run(file),
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
