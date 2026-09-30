//! Builds LSP [`lsp_types::Diagnostic`]s from `pactole-storage-fs`'s
//! positioned analysis results (syntax diagnostics, lowering diagnostics,
//! and include issues), converting spans to LSP ranges via
//! [`crate::conversion`].

use std::collections::HashMap;
use std::path::PathBuf;

use lsp_types::{Diagnostic, DiagnosticSeverity};
use pactole_storage_fs::{AnalyzedProject, ParsedFile};
use pactole_syntax::DiagnosticKind;

use crate::conversion::span_to_range;

/// Builds the diagnostics for a single, standalone file (as produced by
/// [`pactole_storage_fs::analyze_file`]): its syntax diagnostics followed by
/// its lowering diagnostics, in that order.
///
/// This does not include any include-related diagnostics, since a
/// standalone [`ParsedFile`] never resolves includes (see
/// [`pactole_storage_fs::analysis`]'s module docs).
pub fn diagnostics_for_file(file: &ParsedFile) -> Vec<Diagnostic> {
    let source = file.document().source();
    let mut diagnostics = Vec::new();

    for diagnostic in file.syntax_diagnostics() {
        let severity = match diagnostic.kind {
            DiagnosticKind::Error | DiagnosticKind::Missing => DiagnosticSeverity::ERROR,
        };
        diagnostics.push(Diagnostic {
            range: span_to_range(source, &diagnostic.span),
            severity: Some(severity),
            source: Some("pactole-syntax".to_string()),
            message: diagnostic.message.clone(),
            ..Default::default()
        });
    }

    for diagnostic in file.lowering_diagnostics() {
        diagnostics.push(Diagnostic {
            range: span_to_range(source, &diagnostic.span),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("pactole-lowering".to_string()),
            message: diagnostic.message.clone(),
            ..Default::default()
        });
    }

    diagnostics
}

/// Builds the diagnostics for every file reached by a multi-file
/// [`AnalyzedProject`] (as produced by
/// [`pactole_storage_fs::analyze_file_with_loader`]), keyed by file path.
///
/// Each file's own syntax/lowering diagnostics (see [`diagnostics_for_file`])
/// are combined with any [`pactole_storage_fs::IncludeIssue`]s whose
/// `include` directive lives in that file, reported as warnings so a single
/// unresolved include does not read as a hard parse failure.
///
/// Files with no diagnostics at all are still present in the returned map
/// (mapped to an empty `Vec`), so callers can clear stale diagnostics for
/// them by publishing an empty list.
pub fn project_diagnostics(project: &AnalyzedProject) -> HashMap<PathBuf, Vec<Diagnostic>> {
    let mut by_path: HashMap<PathBuf, Vec<Diagnostic>> = project
        .files()
        .iter()
        .map(|f| (f.path.clone(), diagnostics_for_file(&f.file)))
        .collect();

    for issue in project.include_issues() {
        let source = project
            .files()
            .iter()
            .find(|f| f.path == issue.path)
            .map(|f| f.file.document().source());

        let range = match source {
            Some(source) => span_to_range(source, &issue.include.span),
            // The including file itself is not in `project.files()` (should
            // not normally happen); fall back to a zero-width range at the
            // document start rather than dropping the diagnostic.
            None => lsp_types::Range::new(
                lsp_types::Position::new(0, 0),
                lsp_types::Position::new(0, 0),
            ),
        };

        by_path
            .entry(issue.path.clone())
            .or_default()
            .push(Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::WARNING),
                source: Some("pactole-include".to_string()),
                message: issue.message.clone(),
                ..Default::default()
            });
    }

    by_path
}

#[cfg(test)]
mod tests {
    use super::*;
    use pactole_storage_fs::{InMemorySourceLoader, analyze_file, analyze_file_with_loader};
    use std::path::Path;

    #[test]
    fn valid_source_yields_no_diagnostics() {
        let file = analyze_file("2026-09-03 open Assets:Checking\n");
        assert!(diagnostics_for_file(&file).is_empty());
    }

    #[test]
    fn invalid_syntax_yields_an_error_diagnostic() {
        let file = analyze_file("2026-09-03 open\n");
        let diagnostics = diagnostics_for_file(&file);
        assert!(!diagnostics.is_empty());
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(diagnostics[0].source.as_deref(), Some("pactole-syntax"));
    }

    #[test]
    fn missing_include_is_reported_as_a_warning_on_the_including_file() {
        let mut loader = InMemorySourceLoader::new();
        loader.insert("/journal/main.pactole", "include \"missing.pactole\"\n");

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();
        let by_path = project_diagnostics(&project);

        let main_diags = &by_path[Path::new("/journal/main.pactole")];
        assert_eq!(main_diags.len(), 1);
        assert_eq!(main_diags[0].severity, Some(DiagnosticSeverity::WARNING));
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

        let project =
            analyze_file_with_loader(Path::new("/journal/main.pactole"), &loader).unwrap();
        let by_path = project_diagnostics(&project);

        assert!(by_path.contains_key(Path::new("/journal/main.pactole")));
        assert!(by_path.contains_key(Path::new("/journal/other.pactole")));
        assert!(by_path[Path::new("/journal/other.pactole")].is_empty());
    }
}
