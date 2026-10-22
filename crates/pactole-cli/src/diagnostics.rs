use pactole_storage_fs::{Diagnostic, DiagnosticSeverity};

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
