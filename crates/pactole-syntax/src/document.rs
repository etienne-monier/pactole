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

    // Collect with source text available, so `ERROR` diagnostics can quote
    // the unexpected text rather than falling back to a bare tree-sitter
    // S-expression.
    let diagnostics = collect_diagnostics(&tree, Some(source));

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
///
/// This variant has no access to the original source text, so `ERROR`
/// diagnostics cannot quote the unexpected text; prefer
/// [`collect_error_nodes_with_source`] when the source text is available
/// (as [`parse_document`] does internally).
pub fn collect_error_nodes(tree: &Tree) -> Vec<SyntaxDiagnostic> {
    collect_diagnostics(tree, None)
}

/// Like [`collect_error_nodes`], but also takes the original source text so
/// that `ERROR` diagnostics can include a snippet of the unexpected text,
/// in addition to the syntactic context (surrounding directive/node kind).
pub fn collect_error_nodes_with_source(tree: &Tree, source: &str) -> Vec<SyntaxDiagnostic> {
    collect_diagnostics(tree, Some(source))
}

fn collect_diagnostics(tree: &Tree, source: Option<&str>) -> Vec<SyntaxDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut cursor = tree.walk();
    let mut visited_children = false;

    loop {
        let node = cursor.node();

        if !visited_children {
            push_diagnostic(&node, source, &mut diagnostics);
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

fn push_diagnostic(node: &Node, source: Option<&str>, diagnostics: &mut Vec<SyntaxDiagnostic>) {
    if node.is_missing() {
        diagnostics.push(SyntaxDiagnostic::new(
            DiagnosticKind::Missing,
            Span::from_node(node),
            describe_missing(node),
        ));
    } else if node.is_error() {
        // A nested `ERROR` node inside another `ERROR` node (at any
        // ancestor depth, not just a direct parent) does not describe a
        // distinct problem: it is the GLR parser's own internal recovery
        // structure for the same unexpected input already covered by the
        // enclosing `ERROR`'s span. Reporting both would just be noise for
        // the same issue.
        let mut ancestor = node.parent();
        while let Some(current) = ancestor {
            if current.is_error() {
                return;
            }
            ancestor = current.parent();
        }

        diagnostics.push(SyntaxDiagnostic::new(
            DiagnosticKind::Error,
            Span::from_node(node),
            describe_error(node, source),
        ));
    }
}

/// Describes a "missing" node in a human-readable way, using its own kind
/// and (when known) its parent's kind to name the surrounding directive or
/// construct, e.g. `missing an account name in an \`open\` directive`.
fn describe_missing(node: &Node) -> String {
    let kind = node.kind();

    // A missing closing `"` is common enough (an unterminated string) to
    // deserve its own, more specific wording rather than the generic
    // "missing a `\"`" phrasing.
    if kind == "\"" {
        return "unterminated string: missing closing `\"`".to_string();
    }

    let subject = describe_node_kind(kind);
    match node.parent().and_then(|parent| describe_context(&parent)) {
        Some(context) => format!("missing {subject} in {context}"),
        None => format!("missing {subject}"),
    }
}

/// Describes an `ERROR` node by listing the already-recognized named
/// children (if any) and, when the source text is available, a snippet of
/// the unexpected text covered by the node's span.
fn describe_error(node: &Node, source: Option<&str>) -> String {
    let mut recognized = Vec::new();
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.is_named() && !child.is_error() && !child.is_missing() {
                recognized.push(describe_node_kind(child.kind()));
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    let snippet = source
        .and_then(|src| node.utf8_text(src.as_bytes()).ok())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(snippet_for_display);

    match (recognized.is_empty(), snippet) {
        (false, Some(snippet)) => {
            format!(
                "unexpected syntax after {}: `{snippet}`",
                recognized.join(", ")
            )
        }
        (false, None) => format!("unexpected syntax after {}", recognized.join(", ")),
        (true, Some(snippet)) => format!("unexpected syntax: `{snippet}`"),
        (true, None) => "unexpected syntax".to_string(),
    }
}

/// Truncates `text` to its first line and a bounded number of characters,
/// so an `ERROR` diagnostic's message stays a short, single-line summary
/// even when the unexpected input spans multiple lines.
fn snippet_for_display(text: &str) -> String {
    const MAX_CHARS: usize = 60;

    let first_line = text.lines().next().unwrap_or(text);
    if first_line.chars().count() > MAX_CHARS {
        let truncated: String = first_line.chars().take(MAX_CHARS).collect();
        format!("{truncated}…")
    } else {
        first_line.to_string()
    }
}

/// A short, human-readable phrase (with an article) describing what a
/// node of the given tree-sitter `kind` represents, e.g. `"account"` ->
/// `"an account name"`.
fn describe_node_kind(kind: &str) -> String {
    match kind {
        "date" => "a date".to_string(),
        "account" => "an account name".to_string(),
        "commodity_name" => "a commodity code".to_string(),
        "commodity_list" => "a commodity list".to_string(),
        "number" => "a number".to_string(),
        "tolerance" => "a tolerance value".to_string(),
        "string" => "a quoted string".to_string(),
        "status" => "a status flag (`*`, `!`, or `?`)".to_string(),
        "payee" => "a payee".to_string(),
        "narration" => "a narration".to_string(),
        "key" => "a metadata key".to_string(),
        "value" => "a metadata value".to_string(),
        "path" => "a file path".to_string(),
        "comment" => "a comment".to_string(),
        "amount" => "an amount".to_string(),
        "reference" => "a reference".to_string(),
        "tag" => "a tag".to_string(),
        "link" => "a link".to_string(),
        // `_newline` is a hidden tree-sitter rule (its name starts with
        // `_`): such rules are never materialized as nodes in the CST, so
        // this arm can never actually match. There is intentionally no
        // dedicated case for it here.
        other if other.chars().all(|c| !c.is_alphanumeric() && c != '_') => {
            format!("`{other}`")
        }
        other => format!("a `{other}`"),
    }
}

/// A short, human-readable phrase describing the directive/construct a
/// node belongs to, used to give a "missing"/"unexpected" diagnostic
/// surrounding context, e.g. an `open` node's parent -> `"an \`open\`
/// directive"`. Returns `None` for contexts that would not add useful
/// context.
fn describe_context(parent: &Node) -> Option<&'static str> {
    let kind = parent.kind();

    // `_newline` and directive-level `property` nodes are direct children
    // of the generic `directive` wrapper node, not of `open`/`close`/etc.
    // themselves (see `directive` in the grammar): look at the wrapper's
    // first named child to name the actual directive kind. In practice, a
    // "missing" node directly under `directive` (e.g. a missing trailing
    // `_newline` at end-of-file) is not reachable through normal tree
    // traversal in this version of tree-sitter, so this branch mostly
    // guards future grammar/tree-sitter changes rather than firing today.
    if kind == "directive" {
        return parent
            .named_child(0)
            .and_then(|child| describe_context(&child));
    }

    match kind {
        "open" => Some("an `open` directive"),
        "close" => Some("a `close` directive"),
        "commodity" => Some("a `commodity` directive"),
        "payee_declaration" => Some("a `payee` declaration"),
        "include" => Some("an `include` directive"),
        "balance" => Some("a `balance` directive"),
        "transaction" => Some("a transaction"),
        "posting" => Some("a posting"),
        "property" => Some("a metadata property"),
        "string" => Some("a string"),
        "amount" => Some("an amount"),
        "commodity_list" => Some("a commodity list"),
        _ => None,
    }
}
