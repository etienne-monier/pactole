//! Tolerant parsing of `.pactole` source text into a concrete syntax tree.

use tree_sitter::{Node, Parser, Tree};

use crate::diagnostics::{DiagnosticKind, SyntaxDiagnostic};
use crate::span::Span;

/// A parsed `.pactole` source text, holding the original source, the
/// tree-sitter concrete syntax tree, and any syntax diagnostics collected
/// while walking the tree.
///
/// Parsing is tolerant: it always succeeds and produces a tree, even for
/// invalid input. Errors are surfaced as [`SyntaxDiagnostic`]s rather than
/// as a `Result::Err`.
pub struct ParsedDocument {
    source: String,
    tree: Tree,
    diagnostics: Vec<SyntaxDiagnostic>,
}

impl ParsedDocument {
    /// The original source text that was parsed.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The tree-sitter concrete syntax tree produced for this document.
    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// The root node of the concrete syntax tree.
    pub fn root_node(&self) -> Node<'_> {
        self.tree.root_node()
    }

    /// Whether the concrete syntax tree contains any `ERROR` or `MISSING` node.
    pub fn has_errors(&self) -> bool {
        self.root_node().has_error()
    }

    /// The syntax diagnostics collected while walking the tree, in document order.
    pub fn diagnostics(&self) -> &[SyntaxDiagnostic] {
        &self.diagnostics
    }

    /// Converts this parsed document into a strict `Result`: `Ok(self)` if
    /// the concrete syntax tree contains no `ERROR`/`MISSING` node, or
    /// `Err` with the collected diagnostics otherwise.
    ///
    /// This is a convenience for consumers (such as `pactole-storage-fs`)
    /// that only want to proceed on a syntactically clean document, without
    /// duplicating the `has_errors` check themselves.
    pub fn into_result(self) -> Result<Self, Vec<SyntaxDiagnostic>> {
        if self.has_errors() {
            Err(self.diagnostics)
        } else {
            Ok(self)
        }
    }
}

/// Parses `source` as a `.pactole` document and collects syntax diagnostics.
///
/// This function never fails: malformed input yields a [`ParsedDocument`] whose
/// [`ParsedDocument::diagnostics`] describe the `ERROR`/`MISSING` nodes found in
/// the resulting concrete syntax tree.
///
/// # Panics
///
/// Panics if the tree-sitter parser fails to load the Pactole grammar, which
/// would indicate a build-time inconsistency rather than a user-facing error.
pub fn parse_document(source: &str) -> ParsedDocument {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_pactole::LANGUAGE.into())
        .expect("Error loading Pactole parser");

    let tree = parser
        .parse(source, None)
        .expect("tree-sitter parsing should always succeed for a string input");

    let diagnostics = collect_error_nodes(&tree);

    ParsedDocument {
        source: source.to_string(),
        tree,
        diagnostics,
    }
}

/// Alias for [`parse_document`], provided for callers that prefer an `analyze`
/// naming convention (e.g. editor tooling or language servers).
pub fn analyze(source: &str) -> ParsedDocument {
    parse_document(source)
}

/// Walks a tree-sitter [`Tree`] and collects a [`SyntaxDiagnostic`] for every
/// `ERROR` node and every "missing" node it contains, in document order.
///
/// This is purely a syntactic operation: it does not attempt to convert
/// anything to `pactole-core` domain models.
pub fn collect_error_nodes(tree: &Tree) -> Vec<SyntaxDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut cursor = tree.walk();
    let mut visited_children = false;

    loop {
        let node = cursor.node();

        if !visited_children {
            push_diagnostic(&node, &mut diagnostics);
        }

        if !visited_children && cursor.goto_first_child() {
            continue;
        }

        if cursor.goto_next_sibling() {
            visited_children = false;
            continue;
        }

        if !cursor.goto_parent() {
            break;
        }
        visited_children = true;
    }

    diagnostics
}

fn push_diagnostic(node: &Node, diagnostics: &mut Vec<SyntaxDiagnostic>) {
    if node.is_missing() {
        diagnostics.push(SyntaxDiagnostic::new(
            DiagnosticKind::Missing,
            Span::from_node(node),
            format!("missing {}", node.kind()),
        ));
    } else if node.is_error() {
        diagnostics.push(SyntaxDiagnostic::new(
            DiagnosticKind::Error,
            Span::from_node(node),
            node.to_sexp(),
        ));
    }
}
