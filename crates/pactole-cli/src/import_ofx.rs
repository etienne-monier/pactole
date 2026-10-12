use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use pactole_core::Entry;

use crate::config::default_import_config_path;

pub(crate) fn run(
    file: PathBuf,
    config: Option<PathBuf>,
    into: Option<PathBuf>,
    write: bool,
) -> ExitCode {
    let config_path = match config.or_else(default_import_config_path) {
        Some(path) => path,
        None => {
            eprintln!(
                "error: cannot determine the Pactole configuration directory \
                 (set XDG_CONFIG_HOME or HOME, or pass --config)"
            );
            return ExitCode::FAILURE;
        }
    };

    let config_source = match fs::read_to_string(&config_path) {
        Ok(source) => source,
        Err(err) => {
            eprintln!(
                "error: cannot read configuration {}: {err}",
                config_path.display()
            );
            return ExitCode::FAILURE;
        }
    };

    let config: pactole_import::ImportConfig = match toml::from_str(&config_source) {
        Ok(config) => config,
        Err(err) => {
            eprintln!(
                "error: cannot parse configuration {}: {err}",
                config_path.display()
            );
            return ExitCode::FAILURE;
        }
    };

    let mut known_fitids = HashSet::new();
    let mut existing = None;
    if let Some(path) = &into {
        match fs::read_to_string(path) {
            Ok(source) => {
                known_fitids = collect_fitids(&source);
                existing = Some(source);
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                eprintln!("error: cannot read {}: {err}", path.display());
                return ExitCode::FAILURE;
            }
        }
    }

    let source = match fs::read_to_string(&file) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("error: cannot read {}: {err}", file.display());
            return ExitCode::FAILURE;
        }
    };

    let outcome = match pactole_import::import_ofx(&source, &config, &known_fitids) {
        Ok(outcome) => outcome,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::FAILURE;
        }
    };

    for warning in &outcome.warnings {
        eprintln!("warning: {}", describe_warning(warning));
    }

    let generated = pactole_storage_fs::format_transactions(&outcome.transactions);

    if write {
        let path = into.as_deref().unwrap_or_else(|| Path::new(""));
        let mut content = existing.unwrap_or_default();
        if !content.is_empty() {
            if !content.ends_with('\n') {
                content.push('\n');
            }
            content.push('\n');
        }
        content.push_str(&generated);

        if let Err(err) = fs::write(path, content) {
            eprintln!("error: cannot write {}: {err}", path.display());
            return ExitCode::FAILURE;
        }
    } else {
        print!("{generated}");
    }

    eprintln!(
        "imported {} transaction(s), skipped {}",
        outcome.transactions.len(),
        outcome.skipped
    );

    ExitCode::SUCCESS
}

fn collect_fitids(source: &str) -> HashSet<String> {
    let analyzed = pactole_storage_fs::analyze_file(source);

    analyzed
        .entries()
        .iter()
        .filter_map(|parsed_entry| match &parsed_entry.entry {
            Entry::Transaction(transaction) => transaction.meta.get("fitid").cloned(),
            _ => None,
        })
        .collect()
}

fn describe_warning(warning: &pactole_import::ImportWarning) -> String {
    match warning {
        pactole_import::ImportWarning::MissingPayee { fitid } => match fitid {
            Some(fitid) => {
                format!("transaction `{fitid}` has no payee; using the `Unknown` placeholder")
            }
            None => "a transaction has no payee; using the `Unknown` placeholder".to_string(),
        },
    }
}
