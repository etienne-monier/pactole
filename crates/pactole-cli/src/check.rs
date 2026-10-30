use std::path::{Path, PathBuf};
use std::process::ExitCode;

use pactole_app::{
    Diagnostic as ApplicationDiagnostic, DiagnosticSeverity as ApplicationSeverity,
    DiagnosticsQuery,
};
use pactole_config::ValidationOptions;
use pactole_storage_fs::{Diagnostic, DiagnosticSeverity};

use crate::commands::CheckFormatArg;

/// Runs ledger validation and renders diagnostics in the requested format.
pub(crate) fn run(
    file: PathBuf,
    strict: bool,
    format: CheckFormatArg,
    validation: &ValidationOptions,
) -> ExitCode {
    let result =
        match crate::application::service(&file, validation).diagnostics(DiagnosticsQuery {
            include_warnings: true,
        }) {
            Ok(result) => result,
            Err(error) => {
                return match format {
                    CheckFormatArg::Text => failure(&file, error),
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
                };
            }
        };

    let diagnostics = result.diagnostics;
    let failed = diagnostics.iter().any(ApplicationDiagnostic::is_error)
        || (strict && diagnostics.iter().any(ApplicationDiagnostic::is_warning));
    match format {
        CheckFormatArg::Text => {
            crate::diagnostics::print_application(&diagnostics);
            if failed {
                ExitCode::FAILURE
            } else {
                println!("OK");
                ExitCode::SUCCESS
            }
        }
        CheckFormatArg::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": !failed,
                    "diagnostics": diagnostics
                        .iter()
                        .map(application_diagnostic_json)
                        .collect::<Vec<_>>(),
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

/// Converts an application diagnostic to the check command's JSON shape.
pub(crate) fn application_diagnostic_json(diagnostic: &ApplicationDiagnostic) -> serde_json::Value {
    let severity = match diagnostic.severity {
        ApplicationSeverity::Error => "error",
        ApplicationSeverity::Warning => "warning",
    };
    let (file, range) = match (&diagnostic.source, diagnostic.span) {
        (Some(source), Some(span)) => (
            source.0.clone(),
            serde_json::json!({
                "start": {
                    "line": span.start.line + 1,
                    "column": span.start.column + 1,
                },
                "end": {
                    "line": span.end.line + 1,
                    "column": span.end.column + 1,
                },
            }),
        ),
        (Some(source), None) => (source.0.clone(), serde_json::Value::Null),
        (None, Some(span)) => (
            String::from("<ledger>"),
            serde_json::json!({
                "start": {
                    "line": span.start.line + 1,
                    "column": span.start.column + 1,
                },
                "end": {
                    "line": span.end.line + 1,
                    "column": span.end.column + 1,
                },
            }),
        ),
        (None, None) => (String::from("<ledger>"), serde_json::Value::Null),
    };
    serde_json::json!({
        "file": file,
        "range": range,
        "severity": severity,
        "code": diagnostic.code.to_string(),
        "message": diagnostic.message,
    })
}

/// Prints a check error and returns the failure exit code.
fn failure(file: &Path, error: impl std::fmt::Display) -> ExitCode {
    eprintln!("{}: error: {error}", file.display());
    ExitCode::FAILURE
}
