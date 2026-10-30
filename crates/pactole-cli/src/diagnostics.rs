use pactole_storage_fs::{Diagnostic, DiagnosticSeverity};

use pactole_app::{Diagnostic as ApplicationDiagnostic, DiagnosticSeverity as ApplicationSeverity};

/// Prints filesystem diagnostics in the CLI's text format.
pub(crate) fn print(diagnostics: &[Diagnostic]) {
    for diagnostic in diagnostics {
        let severity = match diagnostic.severity {
            DiagnosticSeverity::Error => "error",
            DiagnosticSeverity::Warning => "warning",
        };
        let code = format!(" [{}]", diagnostic.code);
        eprintln!(
            "{}:{}:{}: {severity}:{code} {}",
            diagnostic.path.display(),
            diagnostic.span.start.row + 1,
            diagnostic.span.start.column + 1,
            diagnostic.message,
        );
    }
}

/// Prints application diagnostics in the CLI's text format.
pub(crate) fn print_application(diagnostics: &[ApplicationDiagnostic]) {
    for diagnostic in diagnostics {
        let severity = match diagnostic.severity {
            ApplicationSeverity::Error => "error",
            ApplicationSeverity::Warning => "warning",
        };
        let code = format!(" [{}]", diagnostic.code);
        let source = diagnostic
            .source
            .as_ref()
            .map(|source| source.0.as_str())
            .unwrap_or("<ledger>");
        let (line, column) = diagnostic
            .span
            .map(|span| (span.start.line + 1, span.start.column + 1))
            .unwrap_or((1, 1));
        eprintln!(
            "{source}:{line}:{column}: {severity}:{code} {}",
            diagnostic.message,
        );
    }
}
