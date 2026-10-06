//! Diagnostics reported while tolerantly parsing a `.pactole` source text.

use crate::span::Span;

/// The nature of a syntax diagnostic, mirroring tree-sitter's own error taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticKind {
    /// The parser encountered a sequence of tokens it could not fit into the grammar.
    /// Corresponds to a tree-sitter `ERROR` node.
    Error,
    /// The parser inferred that a token was expected but absent from the input.
    /// Corresponds to a tree-sitter "missing" node.
    Missing,
}

/// A single syntax-level diagnostic produced by tolerant parsing.
///
/// Diagnostics are purely syntactic: they do not reference `pactole-core` domain
/// concepts and only describe where the concrete syntax tree deviates from a
/// fully valid parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxDiagnostic {
    /// Whether this diagnostic reports an `ERROR` node or a `MISSING` node.
    pub kind: DiagnosticKind,
    /// The span in the source text this diagnostic refers to.
    pub span: Span,
    /// A human-readable description of the diagnostic (e.g. "missing an
    /// account name in an `open` directive", or "unexpected syntax after a
    /// date: `foo bar`"), derived from the tree-sitter node's kind and its
    /// surrounding context (parent node kind) rather than a raw
    /// tree-sitter S-expression.
    pub message: String,
}

impl SyntaxDiagnostic {
    /// Builds a new [`SyntaxDiagnostic`].
    pub fn new(kind: DiagnosticKind, span: Span, message: impl Into<String>) -> Self {
        Self {
            kind,
            span,
            message: message.into(),
        }
    }
}
