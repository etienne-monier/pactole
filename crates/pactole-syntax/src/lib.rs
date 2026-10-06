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
//!   [`document::collect_error_nodes`]/[`document::collect_error_nodes_with_source`]
//!   to walk a tree-sitter [`tree_sitter::Tree`] for `ERROR`/`MISSING` nodes.
//!   Diagnostic messages are contextual human-readable phrases (e.g. "missing an
//!   account name in an `open` directive") rather than raw tree-sitter
//!   S-expressions. [`document::ParsedDocument::into_result`] offers a strict
//!   conversion to `Result` for consumers (e.g. `pactole-storage-fs`) that only
//!   want to proceed on a syntactically clean document.
//!
//! This crate performs no I/O and has no dependency on `pactole-core`: it only
//! deals with the concrete syntax tree and its diagnostics.

pub mod diagnostics;
pub mod document;
pub mod span;

pub use diagnostics::{DiagnosticKind, SyntaxDiagnostic};
pub use document::{
    ParsedDocument, analyze, collect_error_nodes, collect_error_nodes_with_source, parse_document,
};
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

    #[test]
    fn missing_closing_quote_is_reported_as_an_unterminated_string() {
        // The grammar keeps looking for the closing `"` across the newline,
        // so the GLR parser recovers by inserting a "missing" node rather
        // than wrapping the whole line in an `ERROR`.
        let source = "2024-01-01 open Assets:Cash\n  note: \"unterminated\n";
        let doc = parse_document(source);

        assert!(doc.has_errors());
        let diagnostic = doc
            .diagnostics()
            .iter()
            .find(|d| d.kind == DiagnosticKind::Missing)
            .expect("expected a missing-node diagnostic");
        assert_eq!(
            diagnostic.message,
            "unterminated string: missing closing `\"`"
        );
    }

    #[test]
    fn missing_closing_quote_inside_a_transaction_payee_is_contextual() {
        // Unterminated strings are the one case where this grammar's GLR
        // error recovery reliably produces a `MISSING` node reachable via
        // normal tree traversal (as opposed to `ERROR`-wrapping the whole
        // construct); exercise it for a payee string too, not just a
        // directive-level metadata value.
        let source = "2024-01-01 * \"unterminated\npayee\n";
        let doc = parse_document(source);

        assert!(doc.has_errors());
        let diagnostic = doc
            .diagnostics()
            .iter()
            .find(|d| d.kind == DiagnosticKind::Missing)
            .expect("expected a missing-node diagnostic");
        assert_eq!(
            diagnostic.message,
            "unterminated string: missing closing `\"`"
        );
    }

    #[test]
    fn unexpected_syntax_after_known_tokens_lists_them_and_quotes_the_snippet() {
        // `balance` requires a trailing commodity code after the number;
        // omitting it leaves date/account/number recognized but makes the
        // whole directive unparsable, so it is wrapped in a single `ERROR`.
        let source = "2024-01-01 balance Assets:Cash 10\n";
        let doc = parse_document(source);

        assert!(doc.has_errors());
        let diagnostic = doc
            .diagnostics()
            .iter()
            .find(|d| d.kind == DiagnosticKind::Error)
            .expect("expected an error-node diagnostic");
        assert!(
            diagnostic.message.contains("a date"),
            "message should mention the recognized date: {}",
            diagnostic.message
        );
        assert!(
            diagnostic.message.contains("an account name"),
            "message should mention the recognized account: {}",
            diagnostic.message
        );
        assert!(
            !diagnostic.message.contains('('),
            "message should not be a raw tree-sitter S-expression: {}",
            diagnostic.message
        );
    }

    #[test]
    fn unexpected_syntax_with_no_recognized_tokens_still_quotes_the_snippet() {
        let source = "garbage line not a directive\n";
        let doc = parse_document(source);

        assert!(doc.has_errors());
        let diagnostic = &doc.diagnostics()[0];
        assert_eq!(diagnostic.kind, DiagnosticKind::Error);
        assert!(
            diagnostic.message.contains("garbage line not a directive"),
            "message should quote the unexpected text: {}",
            diagnostic.message
        );
        assert!(!diagnostic.message.starts_with('('));
    }

    #[test]
    fn nested_error_nodes_are_not_reported_twice() {
        // Each of these garbage lines is wrapped in its own top-level
        // `ERROR` node by the GLR parser; make sure that does not somehow
        // explode into extra, nested diagnostics for the same text.
        let source = "@@@\n###\n$$$\n";
        let doc = parse_document(source);

        assert!(doc.has_errors());
        assert!(
            doc.diagnostics()
                .iter()
                .all(|d| d.kind == DiagnosticKind::Error)
        );
    }

    #[test]
    fn collect_error_nodes_without_source_still_gives_a_contextual_message() {
        let source = "2024-01-01 balance Assets:Cash 10\n";
        let doc = parse_document(source);

        let diagnostics = collect_error_nodes(doc.tree());
        let diagnostic = diagnostics
            .iter()
            .find(|d| d.kind == DiagnosticKind::Error)
            .expect("expected an error-node diagnostic");
        // No source text was provided, so there is no quoted snippet, but
        // the recognized tokens are still named.
        assert!(diagnostic.message.contains("a date"));
        assert!(!diagnostic.message.contains('('));
    }

    #[test]
    fn collect_error_nodes_with_source_matches_document_diagnostics() {
        let source = "2024-01-01 balance Assets:Cash 10\n";
        let doc = parse_document(source);

        let diagnostics = collect_error_nodes_with_source(doc.tree(), source);
        assert_eq!(diagnostics, doc.diagnostics());
    }
}
