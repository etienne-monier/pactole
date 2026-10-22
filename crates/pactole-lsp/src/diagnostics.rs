//! Builds LSP [`lsp_types::Diagnostic`]s from `pactole-storage-fs`'s
//! positioned analysis results (syntax, lowering, include, and business
//! diagnostics), converting spans to LSP ranges via
//! [`crate::conversion`].

use std::collections::HashMap;
use std::path::PathBuf;

use lsp_types::{Diagnostic, DiagnosticSeverity};
use pactole_storage_fs::{
    AnalyzedProject, Diagnostic as ProjectDiagnostic,
    DiagnosticSeverity as ProjectDiagnosticSeverity, DiagnosticSource, ParsedFile,
};

use crate::conversion::span_to_range;

/// Builds the diagnostics for a single, standalone file (as produced by
/// [`pactole_storage_fs::analyze_file`]): its syntax diagnostics followed by
/// its lowering diagnostics, in that order.
///
/// This deliberately does not include include or business diagnostics: a
/// standalone [`ParsedFile`] has no project context in which those checks
/// would be sound.
pub fn standalone_diagnostics(file: &ParsedFile, path: &std::path::Path) -> Vec<Diagnostic> {
    let source = file.document().source();
    file.diagnostics(path)
        .iter()
        .map(|diagnostic| to_lsp_diagnostic(diagnostic, source))
        .collect()
}

fn to_lsp_diagnostic(diagnostic: &ProjectDiagnostic, source: &str) -> Diagnostic {
    Diagnostic {
        range: span_to_range(source, &diagnostic.span),
        severity: Some(match diagnostic.severity {
            ProjectDiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
            ProjectDiagnosticSeverity::Warning => DiagnosticSeverity::WARNING,
        }),
        code: Some(lsp_types::NumberOrString::String(
            diagnostic.code.as_str().to_string(),
        )),
        source: Some(
            match diagnostic.source {
                DiagnosticSource::Syntax => "pactole-syntax",
                DiagnosticSource::Lowering => "pactole-lowering",
                DiagnosticSource::Include => "pactole-include",
                DiagnosticSource::Business => "pactole-business",
            }
            .to_string(),
        ),
        message: diagnostic.message.clone(),
        ..Default::default()
    }
}

/// Builds the diagnostics for every file reached by a multi-file
/// [`AnalyzedProject`] (as produced by
/// [`pactole_storage_fs::analyze_file_with_loader`]), keyed by file path.
///
/// Each file's syntax, lowering, include, and business diagnostics are
/// converted through the same LSP conversion function.
///
/// Files with no diagnostics at all are still present in the returned map
/// (mapped to an empty `Vec`), so callers can clear stale diagnostics for
/// them by publishing an empty list.
pub fn project_diagnostics(project: &AnalyzedProject) -> HashMap<PathBuf, Vec<Diagnostic>> {
    let mut by_path: HashMap<PathBuf, Vec<Diagnostic>> = project
        .files()
        .iter()
        .map(|f| (f.path.clone(), Vec::new()))
        .collect();

    let sources: HashMap<&std::path::Path, &str> = project
        .files()
        .iter()
        .map(|file| (file.path.as_path(), file.file.document().source()))
        .collect();

    for diagnostic in project.diagnostics() {
        let Some(source) = sources.get(diagnostic.path.as_path()) else {
            debug_assert!(false, "project diagnostic has no analyzed source file");
            continue;
        };

        by_path
            .entry(diagnostic.path.clone())
            .or_default()
            .push(to_lsp_diagnostic(&diagnostic, source));
    }

    by_path
}

#[cfg(test)]
mod tests {
    use super::*;
    use pactole_config::{ValidationLevel, ValidationOptions};
    use pactole_storage_fs::{
        InMemorySourceLoader, analyze_file, analyze_file_with_loader,
        analyze_file_with_loader_with_options,
    };
    use std::path::Path;

    #[test]
    fn valid_source_yields_no_diagnostics() {
        let file = analyze_file("2026-09-03 open Assets:Checking\n");
        assert!(standalone_diagnostics(&file, Path::new("/journal.pactole")).is_empty());
    }

    #[test]
    fn invalid_syntax_yields_an_error_diagnostic() {
        let file = analyze_file("2026-09-03 open\n");
        let diagnostics = standalone_diagnostics(&file, Path::new("/journal.pactole"));
        assert!(!diagnostics.is_empty());
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(
            diagnostics[0].code,
            Some(lsp_types::NumberOrString::String(
                "syntax-error".to_string()
            ))
        );
        assert_eq!(diagnostics[0].source.as_deref(), Some("pactole-syntax"));
    }

    #[test]
    fn standalone_and_project_diagnostics_have_the_same_lsp_shape() {
        let path = Path::new("/journal/main.pactole");
        let source = "2026-01-01 open\n";
        let standalone = standalone_diagnostics(&analyze_file(source), path);

        let mut loader = InMemorySourceLoader::new();
        loader.insert(path.to_path_buf(), source);
        let project = analyze_file_with_loader(path, &loader).unwrap();
        let project_diagnostics = project_diagnostics(&project);

        assert_eq!(standalone, project_diagnostics[path]);
    }

    #[test]
    fn missing_include_is_reported_as_an_error_on_the_including_file() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert("/journal/main.pactole", "include \"missing.pactole\"\n");

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();
        let by_path = project_diagnostics(&project);

        let main_diags = &by_path[Path::new("/journal/main.pactole")];
        assert_eq!(main_diags.len(), 1);
        assert_eq!(main_diags[0].severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(
            main_diags[0].code,
            Some(lsp_types::NumberOrString::String(
                "include-not-found".to_string()
            ))
        );
        assert_eq!(main_diags[0].source.as_deref(), Some("pactole-include"));
    }

    #[test]
    fn every_analyzed_file_is_present_in_the_map_even_without_diagnostics() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            "/journal/main.pactole",
            "include \"other.pactole\"\n2026-09-03 open Assets:Checking\n",
        );
        loader.insert("/journal/other.pactole", "2026-09-04 open Assets:Savings\n");

        // Without this, both files would carry an `account-unused` warning
        // (nothing posts to those accounts); the point of this test is a
        // file with *no* diagnostic at all, so the check is turned off.
        let project = analyze_file_with_loader_with_options(
            Path::new("/journal/main.pactole"),
            &loader,
            &ValidationOptions {
                unused: ValidationLevel::Off,
                ..ValidationOptions::default()
            },
        )
        .unwrap();
        let by_path = project_diagnostics(&project);

        assert!(by_path.contains_key(Path::new("/journal/main.pactole")));
        assert!(by_path.contains_key(Path::new("/journal/other.pactole")));
        assert!(by_path[Path::new("/journal/other.pactole")].is_empty());
    }

    #[test]
    fn unused_declarations_are_published_as_warnings() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert("/journal/main.pactole", "2026-01-01 open Assets:Checking\n");

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();
        let by_path = project_diagnostics(&project);
        let diagnostics = &by_path[Path::new("/journal/main.pactole")];

        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(
            diagnostics[0].code,
            Some(lsp_types::NumberOrString::String(
                "account-unused".to_string()
            ))
        );
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(diagnostics[0].source.as_deref(), Some("pactole-business"));
        assert_eq!(
            diagnostics[0].range,
            lsp_types::Range::new(
                lsp_types::Position::new(0, 16),
                lsp_types::Position::new(0, 31),
            )
        );
    }

    #[test]
    fn business_diagnostic_is_published_on_the_originating_file() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert(
            "/journal/main.pactole",
            "2026-01-01 open Assets:Checking\n\
             2026-01-01 open Expenses:Food\n\
             2026-01-01 commodity EUR\n\
             include \"transactions.pactole\"\n",
        );
        loader.insert(
            "/journal/transactions.pactole",
            "2026-01-02 * \"Unknown\"\n\
             \u{20}\u{20}Assets:Checking -10 EUR\n\
             \u{20}\u{20}Expenses:Food 10 EUR\n",
        );

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();
        let by_path = project_diagnostics(&project);
        let diagnostics = &by_path[Path::new("/journal/transactions.pactole")];

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].code,
            Some(lsp_types::NumberOrString::String(
                "payee-not-declared".to_string()
            ))
        );
        assert_eq!(diagnostics[0].source.as_deref(), Some("pactole-business"));
        assert_eq!(diagnostics[0].range.start, lsp_types::Position::new(0, 13));
    }
}
