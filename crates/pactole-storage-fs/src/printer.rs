use crate::errors::PactoleFsStorageError;
use tree_sitter::{Node, Parser};

/// Node kinds whose children all sit on a single output line and are
/// joined by [`Printer::join_children`]: directive headers, `amount` and
/// `property`.
const INLINE_KINDS: &[&str] = &[
    "open",
    "close",
    "commodity",
    "payee_declaration",
    "balance",
    "include",
    "transaction",
    "amount",
    "property",
];

/// Reformat a `.pactole` source string into its canonical form.
///
/// The formatter walks the tree-sitter concrete syntax tree and rewrites
/// whitespace only:
///
/// - directive headers, `key: value` metadata and `account amount`
///   posting lines get canonical single-space separators (with `=` and
///   `:` glued to their neighbour, e.g. `date=effective_date`,
///   `key: value`);
/// - the directive body is indented with two spaces per level (metadata
///   and postings, four spaces for a posting's own metadata), matching
///   the style used throughout `GRAMMAR.md`;
/// - any run of one or more blank lines between top-level directives (or
///   comments) collapses to exactly one; blank lines are never inserted
///   or preserved *inside* a directive, since the grammar doesn't allow
///   them there.
///
/// The textual content of leaves (dates, numbers, strings, accounts,
/// comments, tags, links, references...) is never rewritten, so e.g. the
/// exact number of decimals or thousands separators a user typed is
/// preserved as-is.
///
/// Returns an error if `source` doesn't parse cleanly: reformatting a
/// file with syntax errors could silently corrupt it, so this refuses to
/// guess and reports a parse error instead.
pub fn format(source: &str) -> Result<String, PactoleFsStorageError> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_pactole::LANGUAGE.into())
        .map_err(|e| PactoleFsStorageError::ParseError(e.to_string()))?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| PactoleFsStorageError::ParseError("failed to parse".to_string()))?;

    let root = tree.root_node();
    if root.has_error() {
        return Err(PactoleFsStorageError::ParseError(
            "source has syntax errors, refusing to format it".to_string(),
        ));
    }

    Ok(Printer { source }.print_source_file(root))
}

struct Printer<'src> {
    source: &'src str,
}

impl<'src> Printer<'src> {
    fn text(&self, node: Node<'_>) -> &'src str {
        node.utf8_text(self.source.as_bytes())
            .expect("source is valid UTF-8")
    }

    /// Render a node that may appear as a child of an inline node: recurse
    /// into the other inline kinds (only `amount`, nested in a posting
    /// header), otherwise fall back to the node's raw source text
    /// unchanged.
    fn render(&self, node: Node<'_>) -> String {
        // Some directive keywords (`open`, `close`, `commodity`,
        // `balance`, `include`) share their literal token's text as the
        // node kind of their own header node (e.g. the anonymous `"open"`
        // token and the named `open` node are both `kind() == "open"`).
        // `payee_declaration` is the exception: its literal keyword is
        // `"payee"`, distinct from its own node kind, since `payee` is
        // already used for the payee field of a `transaction`. Only
        // recurse for the named node; the anonymous token is always a
        // plain leaf.
        if node.is_named() && INLINE_KINDS.contains(&node.kind()) {
            self.join_children(node)
        } else {
            self.text(node).to_string()
        }
    }

    /// Render a directive header, `property` or `amount` node onto a
    /// single line, joining its children (named and anonymous alike, in
    /// source order) with single spaces, except around `=` and `:`,
    /// which glue to their neighbour(s) so as to produce
    /// `date=effective_date` and `key: value`.
    fn join_children(&self, node: Node<'_>) -> String {
        let mut cursor = node.walk();
        let mut out = String::new();
        let mut prev_glue_after = false;

        for child in node.children(&mut cursor) {
            let text = self.render(child);
            let is_glue_token = !child.is_named() && (text == "=" || text == ":");

            if !out.is_empty() && !prev_glue_after && !is_glue_token {
                out.push(' ');
            }
            out.push_str(&text);

            prev_glue_after = !child.is_named() && text == "=";
        }

        out
    }

    /// Print a `directive` node: its header line, then any of its own
    /// metadata (`property`) and postings, each indented one level.
    fn print_directive(&self, node: Node<'_>, out: &mut String) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "property" => {
                    out.push_str("  ");
                    out.push_str(&self.join_children(child));
                    out.push('\n');
                }
                "posting" => self.print_posting(child, out),
                // Directive header: open/close/commodity/balance/
                // transaction/include.
                _ => {
                    out.push_str(&self.join_children(child));
                    out.push('\n');
                }
            }
        }
    }

    /// Print a `posting` node: its `account amount` header line, then its
    /// own metadata (`property`), one level deeper.
    fn print_posting(&self, node: Node<'_>, out: &mut String) {
        let mut cursor = node.walk();
        let mut header = String::new();

        for child in node.named_children(&mut cursor) {
            if matches!(child.kind(), "account" | "amount") {
                if !header.is_empty() {
                    header.push(' ');
                }
                header.push_str(&self.render(child));
            }
        }

        out.push_str("  ");
        out.push_str(&header);
        out.push('\n');

        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "property" {
                out.push_str("    ");
                out.push_str(&self.join_children(child));
                out.push('\n');
            }
        }
    }

    /// Print the whole file: each top-level directive or comment, on its
    /// own block, separated by a single blank line whenever the source
    /// had one or more blank lines there.
    fn print_source_file(&self, root: Node<'_>) -> String {
        let mut out = String::new();
        let mut cursor = root.walk();
        let mut prev_end_row: Option<usize> = None;

        for child in root.named_children(&mut cursor) {
            if let Some(prev) = prev_end_row {
                if child.start_position().row.saturating_sub(prev) >= 1 {
                    out.push('\n');
                }
            }

            match child.kind() {
                "comment" => {
                    out.push_str(self.text(child).trim_end());
                    out.push('\n');
                }
                "directive" => self.print_directive(child, &mut out),
                _ => {}
            }

            // A `directive` node's span includes the trailing newline of
            // its last line (every inner rule consumes its own
            // `_newline`), so its end row already sits one past its last
            // content row. A `comment` node does not consume the newline
            // after it, so add one to make it comparable.
            let end_row = child.end_position().row;
            prev_end_row = Some(if child.kind() == "comment" {
                end_row + 1
            } else {
                end_row
            });
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::format;

    #[test]
    fn normalizes_whitespace_and_indentation() {
        let source = "2026-09-03   open    Assets:Checking\n\
                       \x20 description:   \"desc\"\n";

        let formatted = format(source).unwrap();

        assert_eq!(
            formatted,
            "2026-09-03 open Assets:Checking\n  description: \"desc\"\n"
        );
    }

    #[test]
    fn glues_effective_date_and_metadata_colon() {
        let source = "2026-09-03=2026-09-05 * \"Payee\"\n\
                       \x20 Assets:Checking 10.00 EUR\n";

        assert_eq!(format(source).unwrap(), source);
    }

    #[test]
    fn collapses_multiple_blank_lines_between_directives_to_one() {
        let source = "2026-01-01 commodity EUR\n\n\n\n2026-01-02 commodity USD\n";

        assert_eq!(
            format(source).unwrap(),
            "2026-01-01 commodity EUR\n\n2026-01-02 commodity USD\n"
        );
    }

    #[test]
    fn keeps_directives_adjacent_when_source_has_no_blank_line() {
        let source = "2026-01-01 commodity EUR\n2026-01-02 commodity USD\n";

        assert_eq!(format(source).unwrap(), source);
    }

    #[test]
    fn keeps_a_leading_comment_attached_to_its_directive() {
        let source = "; a note\n2026-01-01 commodity EUR\n";

        assert_eq!(format(source).unwrap(), source);
    }

    #[test]
    fn preserves_number_formatting_verbatim() {
        let source = "2026-09-03 balance Assets:Checking 1,234.560 ~ 0.010 EUR\n";

        assert_eq!(format(source).unwrap(), source);
    }

    #[test]
    fn is_idempotent() {
        let source = "2026-09-03   open   Assets:Checking\n\n\n2026-01-01 commodity EUR\n";

        let once = format(source).unwrap();
        let twice = format(&once).unwrap();

        assert_eq!(once, twice);
    }

    #[test]
    fn rejects_source_with_syntax_errors() {
        let source = "not a valid pactole file at all\n";

        assert!(format(source).is_err());
    }
}
