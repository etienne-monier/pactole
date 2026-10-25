use std::path::{Path, PathBuf};
use std::process::ExitCode;

use pactole_config::ValidationOptions;
use pactole_storage_fs::{Diagnostic, DiagnosticSeverity, FsSourceLoader, SourceLoadError};

use crate::commands::CheckFormatArg;

pub(crate) fn run(
    file: PathBuf,
    strict: bool,
    format: CheckFormatArg,
    validation: &ValidationOptions,
) -> ExitCode {
    let project = match pactole_storage_fs::analyze_file_with_loader_with_options(
        &file,
        &FsSourceLoader,
        validation,
    ) {
        Ok(project) => project,
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

    let diagnostics = project.diagnostics();
    let failed = project.has_errors() || (strict && project.has_warnings());
    match format {
        CheckFormatArg::Text => {
            crate::diagnostics::print(&diagnostics);
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
                    "diagnostics": diagnostics.iter().map(diagnostic_json).collect::<Vec<_>>(),
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

pub(crate) fn diagnostic_json(diagnostic: &Diagnostic) -> serde_json::Value {
    let severity = match diagnostic.severity {
        DiagnosticSeverity::Error => "error",
        DiagnosticSeverity::Warning => "warning",
    };
    serde_json::json!({
        "file": diagnostic.path.display().to_string(),
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

fn failure(file: &Path, error: SourceLoadError) -> ExitCode {
    eprintln!("{}: error: {error}", file.display());
    ExitCode::FAILURE
}
