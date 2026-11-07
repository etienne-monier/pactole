use std::path::{Path, PathBuf};
use std::process::ExitCode;

use pactole_config::Config;
use pactole_storage_fs::{
    ConfigDiagnostic, Diagnostic, DiagnosticSeverity, FsSourceLoader,
    analyze_file_with_loader_with_options, check_config_references,
};

use crate::commands::CheckFormatArg;

/// Runs ledger validation and renders diagnostics in the requested format.
pub(crate) fn run(
    file: PathBuf,
    strict: bool,
    format: CheckFormatArg,
    config: &Config,
) -> ExitCode {
    let project =
        match analyze_file_with_loader_with_options(&file, &FsSourceLoader, &config.validation) {
            Ok(project) => project,
            Err(error) => return failure(format, &file, error),
        };

    let diagnostics = project.diagnostics();
    let missing = project
        .partial()
        .map(|partial| partial.missing.clone())
        .unwrap_or_default();
    let config_diagnostics =
        check_config_references(config, &project.source_order_journal(), &missing);

    let failed = diagnostics.iter().any(Diagnostic::is_error)
        || config_diagnostics.iter().any(ConfigDiagnostic::is_error)
        || (strict
            && (diagnostics.iter().any(Diagnostic::is_warning)
                || config_diagnostics.iter().any(ConfigDiagnostic::is_warning)));

    match format {
        CheckFormatArg::Text => {
            crate::diagnostics::print(&diagnostics);
            crate::diagnostics::print_config(&config_diagnostics);
            if failed {
                ExitCode::FAILURE
            } else {
                println!("OK");
                ExitCode::SUCCESS
            }
        }
        CheckFormatArg::Json => {
            let mut rendered = diagnostics
                .iter()
                .map(|diagnostic| {
                    diagnostic_json_with_file(diagnostic, diagnostic.path.display().to_string())
                })
                .collect::<Vec<_>>();
            rendered.extend(config_diagnostics.iter().map(config_diagnostic_json));
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": !failed,
                    "diagnostics": rendered,
                }))
                .unwrap_or_else(|_| String::from("{\"ok\":false,\"diagnostics\":[]}"))
            );
            if failed {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
    }
}

/// Converts a filesystem diagnostic to JSON while using a caller-provided file path.
pub(crate) fn diagnostic_json_with_file(
    diagnostic: &Diagnostic,
    file: String,
) -> serde_json::Value {
    let severity = match diagnostic.severity {
        DiagnosticSeverity::Error => "error",
        DiagnosticSeverity::Warning => "warning",
    };
    serde_json::json!({
        "file": file,
        "range": {
            "start": {
                "line": diagnostic.span.start.row + 1,
                "column": diagnostic.span.start.column + 1,
            },
            "end": {
                "line": diagnostic.span.end.row + 1,
                "column": diagnostic.span.end.column + 1,
            },
        },
        "severity": severity,
        "code": diagnostic.code.to_string(),
        "message": diagnostic.message,
    })
}

/// Converts a configuration diagnostic to the check command's JSON shape.
///
/// A configuration value has no source span: it is located by its TOML key
/// path instead, so `range` is `null` and `keyPath` carries the position.
pub(crate) fn config_diagnostic_json(diagnostic: &ConfigDiagnostic) -> serde_json::Value {
    let severity = match diagnostic.severity {
        DiagnosticSeverity::Error => "error",
        DiagnosticSeverity::Warning => "warning",
    };
    serde_json::json!({
        "file": diagnostic.path.display().to_string(),
        "range": serde_json::Value::Null,
        "keyPath": diagnostic.key_path.to_string(),
        "severity": severity,
        "code": diagnostic.code.to_string(),
        "message": diagnostic.message,
    })
}

/// Prints a check error and returns the failure exit code.
fn failure(format: CheckFormatArg, file: &Path, error: impl std::fmt::Display) -> ExitCode {
    match format {
        CheckFormatArg::Text => {
            eprintln!("{}: error: {error}", file.display());
            ExitCode::FAILURE
        }
        CheckFormatArg::Json => {
            println!(
                "{}",
                serde_json::json!({
                    "ok": false,
                    "error": {
                        "file": file.display().to_string(),
                        "message": error.to_string(),
                    },
                    "diagnostics": [],
                })
            );
            ExitCode::FAILURE
        }
    }
}
