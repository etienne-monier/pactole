use crate::errors::PactoleFsStorageError;
use pactole_core::{Transaction, TransactionStatus};
use pactole_syntax::parse_document;
use tree_sitter::Node;

/// Node kinds whose children all sit on a single output line and are
/// joined by [`Printer::join_children`]: directive headers, amounts,
/// transaction costs and properties.
const INLINE_KINDS: &[&str] = &[
    "open",
    "close",
    "commodity",
    "price",
    "payee_declaration",
    "balance",
    "include",
    "pushtag",
    "poptag",
    "transaction",
    "amount",
    "transaction_cost",
    "property",
    "commodity_list",
];

/// Reformat a `.pactole` source string into its canonical form.
///
/// The formatter walks the tree-sitter concrete syntax tree and rewrites
/// whitespace only:
///
/// - directive headers and `key: value` metadata get canonical single-space
///   separators (with `=` and `:` glued to their neighbour, e.g.
///   `date=effective_date`, `key: value`); posting amounts are aligned to the
///   configured posting width;
/// - the directive body is indented with two spaces per level (metadata
///   and postings, four spaces for a posting's own metadata), matching
///   the style used throughout `GRAMMAR.md`;
/// - any run of one or more blank lines between top-level directives (or
///   comments) collapses to exactly one; blank lines are never inserted
///   or preserved *inside* a directive, since the grammar doesn't allow
///   them there;
/// - a trailing comment on a directive header, a `property` line or a
///   posting's own header line stays glued to that same line; a
///   standalone comment line between two `property`/`posting` items is
///   kept on its own line, at the same indentation as its neighbours
///   (except a standalone comment right after a posting, which the
///   grammar attaches as trailing content of that *preceding* posting and
///   is therefore printed one indent level deeper — see
///   `_property_or_comment` in `grammar.js`).
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
    format_with_options(source, &FormatOptions::default())
}

// `FormatOptions` lives in `pactole-config` (the whole `pactole.toml` schema
// is owned by that crate); it is re-exported here to keep this crate's
// formatting API self-contained.
pub use pactole_config::FormatOptions;

/// Reformat a `.pactole` source string using explicit formatting options.
pub fn format_with_options(
    source: &str,
    options: &FormatOptions,
) -> Result<String, PactoleFsStorageError> {
    let doc = parse_document(source)
        .into_result()
        .map_err(|diagnostics| {
            PactoleFsStorageError::ParseError(format!(
                "source has syntax errors, refusing to format it: {}",
                crate::describe_syntax_diagnostics(&diagnostics)
            ))
        })?;

    Ok(Printer { source, options }.print_source_file(doc.root_node()))
}

/// Render a list of [`Transaction`]s into canonical `.pactole` source text.
///
/// This is the inverse of parsing for the subset of directives produced by
/// importing an external statement (see `pactole-import`): each transaction is
/// printed as a header line, then its metadata, then its postings, with the
/// layout `pactole fmt` would produce. Transactions are separated by a single
/// blank line and the result ends with a newline.
///
/// This is a best-effort, output-only serializer: since the grammar's strings
/// cannot contain `"` or a newline, any such character inside a payee,
/// narration or string metadata value is replaced by a space. Metadata values
/// that look like a date or a number are printed unquoted, everything else is
/// quoted as a string.
pub fn format_transactions(transactions: &[Transaction]) -> String {
    format_transactions_with_options(transactions, &FormatOptions::default())
}

/// Render transactions using explicit formatting options.
pub fn format_transactions_with_options(
    transactions: &[Transaction],
    options: &FormatOptions,
) -> String {
    let mut out = String::new();

    for (index, transaction) in transactions.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        print_transaction(transaction, options, &mut out);
    }

    out
}

/// Print one transaction, ending with a trailing newline.
fn print_transaction(transaction: &Transaction, options: &FormatOptions, out: &mut String) {
    out.push_str(&transaction.date.format("%Y-%m-%d").to_string());
    if let Some(effective_date) = transaction.effective_date {
        out.push('=');
        out.push_str(&effective_date.format("%Y-%m-%d").to_string());
    }

    out.push(' ');
    out.push(match transaction.status {
        TransactionStatus::Cleared => '*',
        TransactionStatus::Pending => '!',
        TransactionStatus::Uncleared => '?',
    });

    out.push(' ');
    out.push_str(&quote_string(transaction.payee.as_deref().unwrap_or("")));
    if let Some(narration) = &transaction.narration {
        out.push(' ');
        out.push_str(&quote_string(narration));
    }

    for tag in &transaction.tags {
        out.push_str(" #");
        out.push_str(tag);
    }
    for link in &transaction.links {
        out.push_str(" ^");
        out.push_str(link);
    }
    if let Some(reference) = &transaction.reference {
        out.push_str(" (");
        out.push_str(reference);
        out.push(')');
    }
    out.push('\n');

    // Directive-level metadata must come before the postings.
    for (key, value) in &transaction.meta {
        out.push_str("  ");
        out.push_str(key.as_str());
        out.push_str(": ");
        out.push_str(&render_metadata_value(value));
        out.push('\n');
    }

    for posting in &transaction.postings {
        // Align on the current line only: `out` holds everything printed so
        // far, so its length cannot be used to pad the amount.
        let mut line = String::new();
        line.push_str(posting.account.as_str());
        if let Some(amount) = &posting.amount {
            let rendered_amount = format!("{} {}", amount.number, amount.commodity);
            append_aligned_amount(
                &mut line,
                &rendered_amount,
                options.posting_width.saturating_sub(2),
            );
        }
        if let Some(cost) = &posting.transaction_cost {
            append_transaction_cost(&mut line, cost);
        }
        out.push_str("  ");
        out.push_str(&line);
        out.push('\n');

        for (key, value) in &posting.meta {
            out.push_str("    ");
            out.push_str(key.as_str());
            out.push_str(": ");
            out.push_str(&render_metadata_value(value));
            out.push('\n');
        }
    }
}

/// Quote a raw string value for the grammar, which does not support escapes:
/// any `"` or line break is replaced by a space so the output stays parseable.
fn quote_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        out.push(match character {
            '"' | '\n' | '\r' => ' ',
            other => other,
        });
    }
    out.push('"');
    out
}

/// Render a metadata value, leaving dates and numbers unquoted and quoting
/// everything else as a string.
fn render_metadata_value(value: &str) -> String {
    if is_grammar_date(value) || is_grammar_number(value) {
        value.to_string()
    } else {
        quote_string(value)
    }
}

/// Append an amount so that its right edge reaches `posting_width`, while
/// keeping at least one separator after the account when the line is already
/// wider than the configured width.
fn append_aligned_amount(out: &mut String, amount: &str, posting_width: usize) {
    let padding = posting_width
        .saturating_sub(out.len() + amount.len())
        .max(1);
    out.extend(std::iter::repeat_n(' ', padding));
    out.push_str(amount);
}

/// Whether `value` matches the grammar's `date` rule exactly
/// (`YYYY-MM-DD`), so it can be emitted unquoted.
fn is_grammar_date(value: &str) -> bool {
    let bytes = value.as_bytes();

    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

/// Whether `value` matches the grammar's `number` rule exactly
/// (`-?\d+(,\d{3})*(\.\d+)?`), so it can be emitted unquoted.
///
/// This deliberately does *not* go through `Decimal::from_str`: that parser
/// accepts scientific notation (e.g. an alphanumeric FITID such as
/// `20260903e0001`), which would then be emitted unquoted and reparsed as a
/// different value. Matching the grammar's own syntax guarantees the emitted
/// text reparses to the exact same string.
fn is_grammar_number(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;

    if bytes.first() == Some(&b'-') {
        index = 1;
    }

    let integer_start = index;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if index == integer_start {
        return false;
    }

    // Optional thousands separators, each followed by exactly three digits.
    while index < bytes.len() && bytes[index] == b',' {
        if index + 3 >= bytes.len()
            || !bytes[index + 1].is_ascii_digit()
            || !bytes[index + 2].is_ascii_digit()
            || !bytes[index + 3].is_ascii_digit()
        {
            return false;
        }
        index += 4;
    }

    // Optional fractional part.
    if index < bytes.len() && bytes[index] == b'.' {
        index += 1;
        let fraction_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index == fraction_start {
            return false;
        }
    }

    index == bytes.len()
}

/// Canonical formatter state for one source document.
struct Printer<'src> {
    /// Original source text used by syntax nodes.
    source: &'src str,
    /// Formatting options selected by the caller.
    options: &'src FormatOptions,
}

impl<'src> Printer<'src> {
    /// Returns the source text covered by a syntax node.
    fn text(&self, node: Node<'_>) -> &'src str {
        match node.utf8_text(self.source.as_bytes()) {
            Ok(text) => text,
            Err(error) => {
                unreachable!("tree-sitter returned a non-UTF-8 source span: {error}")
            }
        }
    }

    /// Render a node that may appear as a child of an inline node: recurse
    /// into the other inline kinds (such as `amount`, nested in a posting
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
            let is_glue_token = !child.is_named() && (text == "=" || text == ":" || text == ",");

            if !out.is_empty() && !prev_glue_after && !is_glue_token {
                out.push(' ');
            }
            out.push_str(&text);

            prev_glue_after = !child.is_named() && (text == "=" || text == ",");
        }

        out
    }

    /// Print a `directive` node: its header line, then any of its own
    /// metadata (`property`), postings and standalone comment lines,
    /// each indented one level, in source order.
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
                // A standalone comment line between two `property`/
                // `posting` items (never the header's own trailing
                // comment, which is nested inside the header node itself
                // and rendered as part of it below).
                "comment" => {
                    out.push_str("  ");
                    out.push_str(self.text(child).trim_end());
                    out.push('\n');
                }
                // Directive header: open/close/commodity/price/balance/
                // transaction/include. Its own trailing comment, if any,
                // is a fielded child of the header node and gets rendered
                // inline by `join_children`.
                _ => {
                    out.push_str(&self.join_children(child));
                    out.push('\n');
                }
            }
        }
    }

    /// Print a `posting` node: its `account amount [cost] [; comment]` header
    /// line, then its own metadata (`property`) and standalone comment
    /// lines, one level deeper, in source order.
    fn print_posting(&self, node: Node<'_>, out: &mut String) {
        let mut cursor = node.walk();
        let mut header = String::new();

        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "account" => header.push_str(self.render(child).trim()),
                "amount" => {
                    let rendered_amount = self.render(child);
                    append_aligned_amount(
                        &mut header,
                        &rendered_amount,
                        self.options.posting_width.saturating_sub(2),
                    );
                }
                "transaction_cost" => {
                    header.push(' ');
                    header.push_str(&self.render(child));
                }
                _ => {}
            }
        }

        // The posting's own trailing comment, if any, is a fielded child
        // of the `posting` node itself (as opposed to a standalone
        // comment line among its `property` children below).
        let header_comment = node.child_by_field_name("comment");
        if let Some(comment) = header_comment {
            header.push(' ');
            header.push_str(self.text(comment));
        }

        out.push_str("  ");
        out.push_str(&header);
        out.push('\n');

        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "property" => {
                    out.push_str("    ");
                    out.push_str(&self.join_children(child));
                    out.push('\n');
                }
                // Skip the header's own fielded trailing comment, already
                // rendered above; only print standalone comment lines.
                "comment" if Some(child) != header_comment => {
                    out.push_str("    ");
                    out.push_str(self.text(child).trim_end());
                    out.push('\n');
                }
                _ => {}
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
            if let Some(prev) = prev_end_row
                && child.start_position().row.saturating_sub(prev) >= 1
            {
                out.push('\n');
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

/// Appends a posting's transaction-cost annotation to a formatted line.
fn append_transaction_cost(line: &mut String, cost: &pactole_core::TransactionCost) {
    line.push(' ');
    match cost.annotation {
        pactole_core::TransactionCostAnnotation::Unit => {
            line.push('@');
            line.push(' ');
            line.push_str(&cost.amount.number.to_string());
            line.push(' ');
            line.push_str(cost.amount.commodity.as_str());
        }
        pactole_core::TransactionCostAnnotation::Total => {
            line.push_str("@@ ");
            line.push_str(&cost.amount.number.to_string());
            line.push(' ');
            line.push_str(cost.amount.commodity.as_str());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FormatOptions, format, format_with_options};

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

        assert_eq!(
            format(source).unwrap(),
            "2026-09-03=2026-09-05 * \"Payee\"\n  Assets:Checking                        10.00 EUR\n"
        );
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
    fn formats_a_price_directive_canonically() {
        let source = "2026-01-01 commodity USD\n2026-01-01 commodity EUR\n\
                       2026-01-02    price   USD   0.920   EUR ; market quote\n";

        assert_eq!(
            format(source).unwrap(),
            "2026-01-01 commodity USD\n2026-01-01 commodity EUR\n\
             2026-01-02 price USD 0.920 EUR ; market quote\n"
        );
    }

    #[test]
    fn preserves_tag_inheritance_directives_without_expanding_them() {
        let source = "pushtag   #year\n\
                       poptag #year\n";

        assert_eq!(format(source).unwrap(), "pushtag #year\npoptag #year\n");
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

    #[test]
    fn glues_commas_in_open_commodity_list() {
        let source = "2026-09-03 open Assets:Broker EUR,USD,BRK'A\n";

        assert_eq!(format(source).unwrap(), source);
    }

    #[test]
    fn keeps_a_trailing_comment_on_a_directive_header() {
        let source = "2026-01-01 commodity EUR ; a note\n";

        assert_eq!(format(source).unwrap(), source);
    }

    #[test]
    fn keeps_a_trailing_comment_on_a_property_line() {
        let source = "2026-09-03 open Assets:Checking\n  \
                       description: \"desc\" ; a note\n";

        assert_eq!(format(source).unwrap(), source);
    }

    #[test]
    fn keeps_a_standalone_comment_line_between_two_properties() {
        let source = "2026-09-03 open Assets:Checking\n  \
                       description: \"desc\"\n  \
                       ; a note\n  \
                       opened_on: 2026-09-03\n";

        assert_eq!(format(source).unwrap(), source);
    }

    #[test]
    fn keeps_a_trailing_comment_on_a_posting() {
        let source = "2026-09-03 * \"Carrefour\" \"Courses\"\n  \
                       Expenses:Groceries 45.30 EUR ; a note\n  \
                       Assets:Checking -45.30 EUR\n";

        assert_eq!(
            format(source).unwrap(),
            "2026-09-03 * \"Carrefour\" \"Courses\"\n  \
             Expenses:Groceries                     45.30 EUR ; a note\n  \
             Assets:Checking                       -45.30 EUR\n"
        );
    }

    #[test]
    fn moves_a_standalone_comment_between_two_postings_under_the_preceding_one() {
        // A standalone comment line right after a posting is
        // indistinguishable, in the grammar, from that posting's own
        // trailing metadata (both start with the same flexible leading
        // whitespace), so it is parsed and printed as trailing content of
        // the *preceding* posting, one indent level deeper, rather than
        // at the postings' own indentation. This is a deliberate,
        // documented trade-off (see `_property_or_comment` in
        // `grammar.js`), not a bug.
        let source = "2026-09-03 * \"Carrefour\" \"Courses\"\n  \
                       Expenses:Groceries 45.30 EUR\n  \
                       ; a note\n  \
                       Assets:Checking -45.30 EUR\n";

        assert_eq!(
            format(source).unwrap(),
            "2026-09-03 * \"Carrefour\" \"Courses\"\n  \
             Expenses:Groceries                     45.30 EUR\n    \
             ; a note\n  \
             Assets:Checking                       -45.30 EUR\n"
        );
    }

    #[test]
    fn keeps_a_trailing_comment_on_a_transaction_header() {
        let source = "2026-09-03 * \"Carrefour\" \"Courses\" ; a note\n  \
                       Expenses:Groceries 45.30 EUR\n  \
                       Assets:Checking -45.30 EUR\n";

        assert_eq!(
            format(source).unwrap(),
            "2026-09-03 * \"Carrefour\" \"Courses\" ; a note\n  \
             Expenses:Groceries                     45.30 EUR\n  \
             Assets:Checking                       -45.30 EUR\n"
        );
    }

    #[test]
    fn aligns_amounts_and_commodities_to_the_configured_width() {
        let source = "2026-09-03 * \"Shop\"\n  \
                       Expenses:Groceries 45.30 EUR\n  \
                       Assets:Checking -1.00 EUR\n";

        let formatted = format_with_options(source, &FormatOptions { posting_width: 30 }).unwrap();

        assert_eq!(
            formatted,
            "2026-09-03 * \"Shop\"\n  \
             Expenses:Groceries 45.30 EUR\n  \
             Assets:Checking    -1.00 EUR\n"
        );
    }

    #[test]
    fn comment_formatting_is_idempotent() {
        let source = "; leading comment\n\
                       2026-01-01 commodity EUR ; trailing comment\n\
                       2026-09-03 open Assets:Checking\n  \
                       description: \"desc\" ; note\n  \
                       ; standalone\n  \
                       opened_on: 2026-09-03\n";

        let once = format(source).unwrap();
        let twice = format(&once).unwrap();

        assert_eq!(once, twice);
    }

    #[test]
    fn format_transactions_round_trips_through_the_parser() {
        use chrono::NaiveDate;
        use pactole_core::{
            AccountName, Amount, CommodityName, Metadata, MetadataKey, Posting, Transaction,
            TransactionStatus,
        };
        use rust_decimal::Decimal;

        let mut meta = Metadata::new();
        meta.insert(
            MetadataKey::new("fitid").unwrap(),
            "2026090300001".to_string(),
        );

        let transaction = Transaction {
            date: NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(),
            effective_date: None,
            status: TransactionStatus::Cleared,
            payee: Some("Carrefour".to_string()),
            narration: Some("Courses".to_string()),
            postings: vec![
                Posting {
                    account: AccountName::new("Depenses:Alimentation").unwrap(),
                    amount: Some(Amount {
                        number: "45.30".parse::<Decimal>().unwrap(),
                        commodity: CommodityName::new("EUR").unwrap(),
                    }),
                    transaction_cost: None,
                    meta: Metadata::new(),
                },
                Posting {
                    account: AccountName::new("Actifs:Banque").unwrap(),
                    amount: Some(Amount {
                        number: "-45.30".parse::<Decimal>().unwrap(),
                        commodity: CommodityName::new("EUR").unwrap(),
                    }),
                    transaction_cost: None,
                    meta: Metadata::new(),
                },
            ],
            tags: Vec::new(),
            links: Vec::new(),
            reference: Some("CB-1".to_string()),
            meta,
        };

        let source = super::format_transactions(std::slice::from_ref(&transaction));
        let parsed = crate::analyze_file(&source);
        assert!(parsed.syntax_diagnostics().is_empty());
        assert!(parsed.lowering_diagnostics().is_empty());
        let entries = parsed.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].entry,
            pactole_core::Entry::Transaction(transaction)
        );
    }

    #[test]
    fn format_preserves_and_canonicalizes_transaction_cost_annotations() {
        let source = "2026-10-25 * \"Exchange\"\n  Assets:USD   -100 USD    @   0.93 EUR\n  Assets:EUR 93 EUR   @@ 93 EUR\n";

        let formatted = format(source).unwrap();

        assert!(formatted.contains("@ 0.93 EUR"));
        assert!(formatted.contains("@@ 93 EUR"));
        assert_eq!(format(&formatted).unwrap(), formatted);
    }

    #[test]
    fn transaction_serializer_round_trips_both_cost_annotation_forms() {
        use chrono::NaiveDate;
        use pactole_core::{
            AccountName, Amount, CommodityName, Metadata, Posting, Transaction, TransactionCost,
            TransactionCostAnnotation, TransactionStatus,
        };
        use rust_decimal::Decimal;

        let quote_commodity = CommodityName::new("EUR").unwrap();
        let transaction = Transaction {
            date: NaiveDate::from_ymd_opt(2026, 10, 25).unwrap(),
            effective_date: None,
            status: TransactionStatus::Cleared,
            payee: Some("Exchange".to_string()),
            narration: None,
            postings: vec![
                Posting {
                    account: AccountName::new("Assets:USD").unwrap(),
                    amount: Some(Amount {
                        number: Decimal::new(-100, 0),
                        commodity: CommodityName::new("USD").unwrap(),
                    }),
                    transaction_cost: Some(TransactionCost {
                        amount: Amount {
                            number: Decimal::new(93, 2),
                            commodity: quote_commodity.clone(),
                        },
                        annotation: TransactionCostAnnotation::Unit,
                    }),
                    meta: Metadata::new(),
                },
                Posting {
                    account: AccountName::new("Assets:EUR").unwrap(),
                    amount: Some(Amount {
                        number: Decimal::new(93, 0),
                        commodity: quote_commodity.clone(),
                    }),
                    transaction_cost: Some(TransactionCost {
                        amount: Amount {
                            number: Decimal::new(93, 0),
                            commodity: quote_commodity,
                        },
                        annotation: TransactionCostAnnotation::Total,
                    }),
                    meta: Metadata::new(),
                },
            ],
            tags: Vec::new(),
            links: Vec::new(),
            reference: None,
            meta: Metadata::new(),
        };

        let source = super::format_transactions(std::slice::from_ref(&transaction));
        assert!(source.contains("@ 0.93 EUR"));
        assert!(source.contains("@@ 93 EUR"));

        let parsed = crate::analyze_file(&source);
        assert!(parsed.syntax_diagnostics().is_empty());
        assert!(parsed.lowering_diagnostics().is_empty());
        assert_eq!(
            parsed.entries()[0].entry,
            pactole_core::Entry::Transaction(transaction)
        );
    }

    #[test]
    fn format_transactions_aligns_amounts_on_every_line() {
        use chrono::NaiveDate;
        use pactole_core::{
            AccountName, Amount, CommodityName, Metadata, Posting, Transaction, TransactionStatus,
        };
        use rust_decimal::Decimal;

        let transaction = |account: &str, number: &str| Transaction {
            date: NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(),
            effective_date: None,
            status: TransactionStatus::Cleared,
            payee: Some("Carrefour".to_string()),
            narration: Some("Courses".to_string()),
            postings: vec![Posting {
                account: AccountName::new(account).unwrap(),
                amount: Some(Amount {
                    number: number.parse::<Decimal>().unwrap(),
                    commodity: CommodityName::new("EUR").unwrap(),
                }),
                transaction_cost: None,
                meta: Metadata::new(),
            }],
            tags: Vec::new(),
            links: Vec::new(),
            reference: None,
            meta: Metadata::new(),
        };

        let transactions = vec![
            transaction("Depenses:Alimentation", "45.30"),
            transaction("Actifs:Banque", "-45.30"),
        ];

        let source = super::format_transactions(&transactions);

        // Every posting line is padded to the configured posting width, in
        // every transaction, not only in the first line of the output.
        for line in source.lines().filter(|line| line.starts_with("  ")) {
            assert_eq!(line.len(), 50, "unaligned posting: {line:?}");
        }
        // The serializer's output is already canonical.
        assert_eq!(format(&source).unwrap(), source);
    }

    #[test]
    fn render_metadata_value_distinguishes_dates_numbers_and_strings() {
        // Dates and grammar-sized numbers are emitted bare...
        assert_eq!(super::render_metadata_value("2026-09-03"), "2026-09-03");
        assert_eq!(
            super::render_metadata_value("2026090300001"),
            "2026090300001"
        );
        assert_eq!(super::render_metadata_value("-1,234.50"), "-1,234.50");

        // ... while anything else is quoted, including values that a naive
        // decimal parser would have accepted as scientific notation.
        assert_eq!(
            super::render_metadata_value("20260903e0001"),
            "\"20260903e0001\""
        );
        assert_eq!(super::render_metadata_value("SGML-1"), "\"SGML-1\"");
        assert_eq!(super::render_metadata_value(""), "\"\"");
    }

    #[test]
    fn format_transactions_quotes_non_numeric_metadata() {
        use chrono::NaiveDate;
        use pactole_core::{
            AccountName, Amount, CommodityName, Metadata, MetadataKey, Posting, Transaction,
            TransactionStatus,
        };
        use rust_decimal::Decimal;

        let mut meta = Metadata::new();
        meta.insert(MetadataKey::new("fitid").unwrap(), "SGML-1".to_string());

        let transaction = Transaction {
            date: NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(),
            effective_date: None,
            status: TransactionStatus::Cleared,
            payee: Some("Boulangerie".to_string()),
            narration: None,
            postings: vec![Posting {
                account: AccountName::new("Depenses:A-Classer").unwrap(),
                amount: Some(Amount {
                    number: "12.00".parse::<Decimal>().unwrap(),
                    commodity: CommodityName::new("EUR").unwrap(),
                }),
                transaction_cost: None,
                meta: Metadata::new(),
            }],
            tags: Vec::new(),
            links: Vec::new(),
            reference: None,
            meta,
        };

        let source = super::format_transactions(&[transaction]);

        assert!(source.contains("fitid: \"SGML-1\""));
        let parsed = crate::analyze_file(&source);
        assert!(parsed.syntax_diagnostics().is_empty());
        assert!(parsed.lowering_diagnostics().is_empty());
    }
}
