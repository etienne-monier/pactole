//! Tolerant syntax analysis for `.pactole` source text.
//!
//! `pactole-syntax` sits between the raw tree-sitter grammar
//! (`tree-sitter-pactole`) and higher-level consumers such as `pactole-storage-fs`
//! or a future language server. It exposes:
//!
//! - [`span`]: byte-offset and row/column positions ([`span::Point`], [`span::Span`]).
//! - [`diagnostics`]: syntax-level diagnostics ([`diagnostics::SyntaxDiagnostic`],
//!   [`diagnostics::DiagnosticKind`]) that do not reference `pactole-core`.
//! - [`document`]: tolerant parsing entry points ([`document::parse_document`],
//!   [`document::analyze`]) producing a [`document::ParsedDocument`], plus
//!   [`document::collect_error_nodes`] to walk a tree-sitter [`tree_sitter::Tree`]
//!   for `ERROR`/`MISSING` nodes. [`document::ParsedDocument::into_result`] offers a
//!   strict conversion to `Result` for consumers (e.g. `pactole-storage-fs`) that only
//!   want to proceed on a syntactically clean document.
//!
//! This crate performs no I/O and has no dependency on `pactole-core`: it only
//! deals with the concrete syntax tree and its diagnostics.

pub mod diagnostics;
pub mod document;
pub mod span;

pub use diagnostics::{DiagnosticKind, SyntaxDiagnostic};
pub use document::{ParsedDocument, analyze, collect_error_nodes, parse_document};
pub use span::{Point, Span};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_source_without_diagnostics() {
        let source = "2024-01-01 open Assets:Cash\n";
        let doc = parse_document(source);

        assert!(!doc.has_errors());
        assert!(doc.diagnostics().is_empty());
        assert_eq!(doc.source(), source);
    }

    #[test]
    fn reports_positioned_diagnostic_for_invalid_source() {
        let source = "2024-01-01 open\n";
        let doc = parse_document(source);

        assert!(doc.has_errors());
        assert!(!doc.diagnostics().is_empty());

        let diagnostic = &doc.diagnostics()[0];
        // The diagnostic must be anchored somewhere within the source text.
        assert!(diagnostic.span.start.byte <= source.len());
        assert!(diagnostic.span.end.byte <= source.len());
        assert!(diagnostic.span.start.row == 0);
    }

    #[test]
    fn into_result_succeeds_for_valid_source() {
        let source = "2024-01-01 open Assets:Cash\n";
        let doc = parse_document(source).into_result();

        assert!(doc.is_ok());
    }

    #[test]
    fn into_result_fails_for_invalid_source() {
        let source = "2024-01-01 open\n";
        let result = parse_document(source).into_result();

        let diagnostics = match result {
            Ok(_) => panic!("expected an error"),
            Err(diagnostics) => diagnostics,
        };
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn analyze_is_an_alias_for_parse_document() {
        let source = "2024-01-01 open Assets:Cash\n";
        let doc = analyze(source);

        assert!(!doc.has_errors());
    }

    #[test]
    fn collect_error_nodes_matches_document_diagnostics_count() {
        let source = "2024-01-01 open\n";
        let doc = parse_document(source);

        let diagnostics = collect_error_nodes(doc.tree());
        assert_eq!(diagnostics.len(), doc.diagnostics().len());
    }
}
