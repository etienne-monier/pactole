use crate::errors::PactoleFsStorageError;
use chrono::NaiveDate;
use pactole_core::{
    AccountName, Amount, Balance, Close, Commodity, CommodityName, Entry, Metadata, MetadataKey,
    Open, Payee, Posting, Price, Transaction, TransactionStatus,
};
use rust_decimal::Decimal;
use std::str::FromStr;
use tree_sitter::Node;

/// Outcome of lowering a single top-level `directive` node without resolving
/// `include` directives (which are surfaced as
/// [`DirectiveOutcome::Include`] with their raw, unresolved path).
///
/// This is used by `analysis.rs` for positioned, directive-by-directive
/// analysis.
pub(crate) enum DirectiveOutcome {
    /// A directive that was successfully lowered into a domain entry.
    Entry(Entry),
    /// An `include` directive, with its raw (unresolved) path.
    Include(String),
}

/// The AST builder
///
/// Holds a ref to the source.
pub(crate) struct AstBuilder<'src> {
    source: &'src str,
}

impl<'src> AstBuilder<'src> {
    /// Create a new AstBuilder from source reference.
    pub(crate) fn new(source: &str) -> AstBuilder<'_> {
        AstBuilder { source }
    }

    /// Return the source text spanned by the given node.
    fn text(&self, node: Node<'_>) -> &'src str {
        match node.utf8_text(self.source.as_bytes()) {
            Ok(text) => text,
            Err(error) => {
                unreachable!("tree-sitter returned a non-UTF-8 source span: {error}")
            }
        }
    }

    /// Build a parser error carrying the given message.
    fn error(&self, message: impl Into<String>) -> PactoleFsStorageError {
        PactoleFsStorageError::ParseError(message.into())
    }

    /// Find the first named child of `node` with the given kind.
    fn find_child<'a>(&self, node: Node<'a>, kind: &str) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        node.named_children(&mut cursor).find(|c| c.kind() == kind)
    }

    /// Find the first named child of `node` with the given kind, or fail.
    fn require_child<'a>(
        &self,
        node: Node<'a>,
        kind: &str,
    ) -> Result<Node<'a>, PactoleFsStorageError> {
        self.find_child(node, kind)
            .ok_or_else(|| self.error(format!("missing `{kind}` node in `{}`", node.kind())))
    }

    fn require_field<'a>(
        &self,
        node: Node<'a>,
        field: &str,
    ) -> Result<Node<'a>, PactoleFsStorageError> {
        node.child_by_field_name(field)
            .ok_or_else(|| self.error(format!("missing `{field}` field in `{}`", node.kind())))
    }

    fn parse_date(&self, node: Node<'_>) -> Result<NaiveDate, PactoleFsStorageError> {
        let text = self.text(node);
        NaiveDate::parse_from_str(text, "%Y-%m-%d").map_err(|e| {
            self.error(format!(
                "invalid date `{text}`: {e} (expected a valid calendar date in YYYY-MM-DD format)"
            ))
        })
    }

    fn parse_number(&self, node: Node<'_>) -> Result<Decimal, PactoleFsStorageError> {
        let text = self.text(node);
        // Numbers may use `,` as a thousands separator (Beancount-like),
        // e.g. `1,234,567.89`; strip them before parsing the decimal.
        let normalized = text.replace(',', "");
        Decimal::from_str(&normalized)
            .map_err(|e| self.error(format!("invalid number `{text}`: {e} (expected an optionally signed decimal number, e.g. `1,234.56`)")))
    }

    fn parse_account(&self, node: Node<'_>) -> Result<AccountName, PactoleFsStorageError> {
        Ok(AccountName::new(self.text(node))?)
    }

    fn parse_commodity_name(&self, node: Node<'_>) -> Result<CommodityName, PactoleFsStorageError> {
        Ok(CommodityName::new(self.text(node))?)
    }

    /// Unquote a `string` node's text, e.g. `"foo"` -> `foo`.
    fn parse_string(&self, node: Node<'_>) -> String {
        self.text(node).trim_matches('"').to_string()
    }

    fn parse_status(&self, node: Node<'_>) -> Result<TransactionStatus, PactoleFsStorageError> {
        match self.text(node) {
            "*" => Ok(TransactionStatus::Cleared),
            "!" => Ok(TransactionStatus::Pending),
            "?" => Ok(TransactionStatus::Uncleared),
            other => Err(self.error(format!("unknown transaction status `{other}`"))),
        }
    }

    /// Extract the string content from a wrapper node holding a single
    /// `string` child, e.g. `payee`, `narration` or `path`.
    fn parse_quoted_text(&self, node: Node<'_>) -> Result<String, PactoleFsStorageError> {
        let string_node = self.require_child(node, "string")?;
        Ok(self.parse_string(string_node))
    }

    /// Parse a `value` node (string, date or number) into its raw metadata
    /// text representation.
    fn parse_metadata_value(&self, node: Node<'_>) -> Result<String, PactoleFsStorageError> {
        let child = node
            .named_child(0)
            .ok_or_else(|| self.error("empty `value` node"))?;

        Ok(match child.kind() {
            "string" => self.parse_string(child),
            _ => self.text(child).to_string(),
        })
    }

    /// Collect the metadata (`property` children) of a directive node into
    /// a `Metadata` map.
    fn build_metadata(&self, node: Node<'_>) -> Result<Metadata, PactoleFsStorageError> {
        let mut meta = Metadata::new();
        let mut cursor = node.walk();

        for child in node.named_children(&mut cursor) {
            if child.kind() != "property" {
                continue;
            }

            let key = MetadataKey::new(self.text(self.require_child(child, "key")?))?;
            let value = self.parse_metadata_value(self.require_child(child, "value")?)?;
            meta.insert(key, value);
        }

        Ok(meta)
    }

    /// Lower a single top-level `directive` node into a [`DirectiveOutcome`],
    /// without resolving `include` directives: their raw, unresolved path is
    /// returned instead of the entries of the included file.
    ///
    /// Shared by the positioned, directive-by-directive analysis, which
    /// reports includes without reading them.
    pub(crate) fn build_directive_outcome(
        &self,
        node: Node<'_>,
    ) -> Result<DirectiveOutcome, PactoleFsStorageError> {
        let header = node
            .named_child(0)
            .ok_or_else(|| self.error("directive has no header"))?;

        Ok(match header.kind() {
            "open" => DirectiveOutcome::Entry(Entry::Open(self.build_open(node, header)?)),
            "close" => DirectiveOutcome::Entry(Entry::Close(self.build_close(node, header)?)),
            "commodity" => {
                DirectiveOutcome::Entry(Entry::Commodity(self.build_commodity(node, header)?))
            }
            "price" => DirectiveOutcome::Entry(Entry::Price(self.build_price(node, header)?)),
            "payee_declaration" => {
                DirectiveOutcome::Entry(Entry::Payee(self.build_payee_declaration(node, header)?))
            }
            "transaction" => {
                DirectiveOutcome::Entry(Entry::Transaction(self.build_transaction(node, header)?))
            }
            "balance" => DirectiveOutcome::Entry(Entry::Balance(self.build_balance(node, header)?)),
            "include" => {
                let path_node = self.require_child(header, "path")?;
                DirectiveOutcome::Include(self.parse_quoted_text(path_node)?)
            }
            other => return Err(self.error(format!("unexpected directive header `{other}`"))),
        })
    }

    fn build_open(
        &self,
        directive: Node<'_>,
        open: Node<'_>,
    ) -> Result<Open, PactoleFsStorageError> {
        let date = self.parse_date(self.require_child(open, "date")?)?;
        let account = self.parse_account(self.require_child(open, "account")?)?;
        let commodities = self
            .find_child(open, "commodity_list")
            .map(|n| self.build_commodity_list(n))
            .transpose()?
            .unwrap_or_default();

        let mut meta = self.build_metadata(directive)?;
        let description = meta.remove("description");

        Ok(Open {
            date,
            account,
            commodities,
            description,
            meta,
        })
    }

    /// Collect the `commodity_name` children of a `commodity_list` node
    /// into an ordered list of commodities.
    fn build_commodity_list(
        &self,
        node: Node<'_>,
    ) -> Result<Vec<CommodityName>, PactoleFsStorageError> {
        let mut cursor = node.walk();
        node.named_children(&mut cursor)
            .map(|c| self.parse_commodity_name(c))
            .collect()
    }

    fn build_close(
        &self,
        directive: Node<'_>,
        close: Node<'_>,
    ) -> Result<Close, PactoleFsStorageError> {
        let date = self.parse_date(self.require_child(close, "date")?)?;
        let account = self.parse_account(self.require_child(close, "account")?)?;
        let meta = self.build_metadata(directive)?;

        Ok(Close {
            date,
            account,
            meta,
        })
    }

    fn build_commodity(
        &self,
        directive: Node<'_>,
        commodity: Node<'_>,
    ) -> Result<Commodity, PactoleFsStorageError> {
        let date = self.parse_date(self.require_child(commodity, "date")?)?;
        let name = self.parse_commodity_name(self.require_child(commodity, "commodity_name")?)?;
        let meta = self.build_metadata(directive)?;

        Ok(Commodity { date, name, meta })
    }

    fn build_price(
        &self,
        directive: Node<'_>,
        price: Node<'_>,
    ) -> Result<Price, PactoleFsStorageError> {
        let date = self.parse_date(self.require_field(price, "date")?)?;
        let base = self.parse_commodity_name(self.require_field(price, "base")?)?;
        let number = self.parse_number(self.require_field(price, "number")?)?;
        let commodity = self.parse_commodity_name(self.require_field(price, "quote")?)?;
        let meta = self.build_metadata(directive)?;

        Ok(Price {
            date,
            base,
            quote: Amount { number, commodity },
            meta,
        })
    }

    fn build_payee_declaration(
        &self,
        directive: Node<'_>,
        payee_declaration: Node<'_>,
    ) -> Result<Payee, PactoleFsStorageError> {
        let name = self.parse_string(self.require_child(payee_declaration, "string")?);
        let meta = self.build_metadata(directive)?;

        Ok(Payee { name, meta })
    }

    fn build_balance(
        &self,
        directive: Node<'_>,
        balance: Node<'_>,
    ) -> Result<Balance, PactoleFsStorageError> {
        let date = self.parse_date(self.require_child(balance, "date")?)?;
        let account = self.parse_account(self.require_child(balance, "account")?)?;
        let number = self.parse_number(self.require_child(balance, "number")?)?;
        let commodity =
            self.parse_commodity_name(self.require_child(balance, "commodity_name")?)?;
        let tolerance = self
            .find_child(balance, "tolerance")
            .map(|n| self.parse_number(n))
            .transpose()?;
        let meta = self.build_metadata(directive)?;

        Ok(Balance {
            date,
            account,
            amount: Amount { number, commodity },
            tolerance,
            meta,
        })
    }

    fn build_amount(&self, amount: Node<'_>) -> Result<Amount, PactoleFsStorageError> {
        let number = self.parse_number(self.require_child(amount, "number")?)?;
        let commodity = self.parse_commodity_name(self.require_child(amount, "commodity_name")?)?;

        Ok(Amount { number, commodity })
    }

    fn build_transaction(
        &self,
        directive: Node<'_>,
        transaction: Node<'_>,
    ) -> Result<Transaction, PactoleFsStorageError> {
        let date_node = transaction
            .child_by_field_name("date")
            .ok_or_else(|| self.error("transaction has no date"))?;
        let date = self.parse_date(date_node)?;

        let effective_date = transaction
            .child_by_field_name("effective_date")
            .map(|n| self.parse_date(n))
            .transpose()?;

        let status = self
            .find_child(transaction, "status")
            .map(|node| self.parse_status(node))
            .transpose()?
            .unwrap_or(TransactionStatus::Uncleared);

        let payee = self
            .find_child(transaction, "payee")
            .map(|n| self.parse_quoted_text(n))
            .transpose()?;

        let narration = self
            .find_child(transaction, "narration")
            .map(|n| self.parse_quoted_text(n))
            .transpose()?;

        let reference = self.find_child(transaction, "reference").map(|n| {
            self.text(n)
                .trim_start_matches('(')
                .trim_end_matches(')')
                .to_string()
        });

        let mut tags = Vec::new();
        let mut links = Vec::new();
        let mut cursor = transaction.walk();

        for child in transaction.named_children(&mut cursor) {
            match child.kind() {
                "tag" => tags.push(self.text(child).trim_start_matches('#').to_string()),
                "link" => links.push(self.text(child).trim_start_matches('^').to_string()),
                _ => continue,
            }
        }

        let meta = self.build_metadata(directive)?;

        let mut postings = Vec::new();
        let mut cursor = directive.walk();

        for child in directive.named_children(&mut cursor) {
            if child.kind() == "posting" {
                postings.push(self.build_posting(child)?);
            }
        }

        Ok(Transaction {
            date,
            effective_date,
            status,
            payee,
            narration,
            postings,
            tags,
            links,
            reference,
            meta,
        })
    }

    fn build_posting(&self, posting: Node<'_>) -> Result<Posting, PactoleFsStorageError> {
        let account = self.parse_account(self.require_child(posting, "account")?)?;
        let amount = self
            .find_child(posting, "amount")
            .map(|n| self.build_amount(n))
            .transpose()?;

        let mut meta = Metadata::new();
        let mut cursor = posting.walk();

        for child in posting.named_children(&mut cursor) {
            if child.kind() != "property" {
                continue;
            }

            let key = MetadataKey::new(self.text(self.require_child(child, "key")?))?;
            let value = self.parse_metadata_value(self.require_child(child, "value")?)?;
            meta.insert(key, value);
        }

        Ok(Posting {
            account,
            amount,
            meta,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::PactoleFsStorageError;
    use pactole_core::{CommodityName, Entry, Journal, Price, TransactionStatus};
    use std::path::Path;

    fn parse(source: &str, _base_dir: Option<&Path>) -> Result<Journal, PactoleFsStorageError> {
        let file = crate::analyze_file(source);
        if let Some(diagnostic) = file.syntax_diagnostics().first() {
            return Err(PactoleFsStorageError::ParseError(format!(
                "at {}: {}",
                diagnostic.span.start, diagnostic.message
            )));
        }
        if let Some(diagnostic) = file.lowering_diagnostics().first() {
            return Err(PactoleFsStorageError::ParseError(
                diagnostic.message.clone(),
            ));
        }
        Ok(Journal {
            entries: file
                .entries()
                .iter()
                .map(|entry| entry.entry.clone())
                .collect(),
        })
    }

    #[test]
    fn parses_open_with_a_single_restricted_commodity() {
        let source = "2026-09-03 open Assets:Checking EUR\n";
        let journal = parse(source, None).unwrap();

        match &journal.entries[0] {
            Entry::Open(open) => {
                assert_eq!(open.commodities, vec![CommodityName::new("EUR").unwrap()]);
            }
            other => panic!("expected an Open entry, got {other:?}"),
        }
    }

    #[test]
    fn parses_open_with_several_restricted_commodities() {
        let source = "2026-09-03 open Assets:Broker EUR,USD\n";
        let journal = parse(source, None).unwrap();

        match &journal.entries[0] {
            Entry::Open(open) => {
                assert_eq!(
                    open.commodities,
                    vec![
                        CommodityName::new("EUR").unwrap(),
                        CommodityName::new("USD").unwrap(),
                    ]
                );
            }
            other => panic!("expected an Open entry, got {other:?}"),
        }
    }

    #[test]
    fn parses_open_without_restricted_commodities() {
        let source = "2026-09-03 open Assets:Checking\n";
        let journal = parse(source, None).unwrap();

        match &journal.entries[0] {
            Entry::Open(open) => assert!(open.commodities.is_empty()),
            other => panic!("expected an Open entry, got {other:?}"),
        }
    }

    #[test]
    fn parses_a_dated_market_price() {
        let journal = parse(
            "2026-01-01 commodity EUR\n\
             2026-01-01 commodity USD\n\
             2026-01-02 price USD 0.92 EUR\n",
            None,
        )
        .unwrap();

        assert!(matches!(
            &journal.entries[2],
            Entry::Price(Price { base, quote, .. })
                if base.as_str() == "USD"
                    && quote.number == "0.92".parse().unwrap()
                    && quote.commodity.as_str() == "EUR"
        ));
    }

    #[test]
    fn reports_the_position_of_the_first_syntax_error() {
        let source = "2026-09-03 open\n";

        let error = parse(source, None).unwrap_err();

        let message = error.to_string();
        assert!(
            message.contains("1:"),
            "expected the error to mention the line of the first diagnostic, got: {message}"
        );
    }

    #[test]
    fn rejects_a_posting_under_a_non_transaction_directive() {
        let source = "2026-09-03 open Assets:Checking\n  Assets:Checking 10.00 EUR\n";

        assert!(parse(source, None).is_err());
    }

    #[test]
    fn rejects_a_posting_under_a_commodity_directive() {
        let source = "2026-01-01 commodity EUR\n  Assets:Checking 10.00 EUR\n";

        assert!(parse(source, None).is_err());
    }

    #[test]
    fn ignores_comments_at_every_supported_position() {
        let source = "; a leading comment\n\
                       2026-01-01 commodity EUR ; trailing on header\n\
                       2026-09-03 open Assets:Checking\n  \
                       description: \"desc\" ; trailing on property\n  \
                       ; standalone between properties\n  \
                       opened_on: 2026-09-03\n\
                       2026-09-03 * \"Carrefour\" \"Courses\" ; trailing on transaction\n  \
                       Expenses:Groceries 45.30 EUR ; trailing on posting\n  \
                       ; standalone between postings\n  \
                       Assets:Checking -45.30 EUR\n";

        let journal = parse(source, None).unwrap();

        assert_eq!(journal.entries.len(), 3);

        match &journal.entries[1] {
            Entry::Open(open) => {
                assert_eq!(open.description.as_deref(), Some("desc"));
                assert_eq!(open.meta.get("opened_on"), Some(&"2026-09-03".to_string()));
            }
            other => panic!("expected an Open entry, got {other:?}"),
        }

        match &journal.entries[2] {
            Entry::Transaction(transaction) => {
                assert_eq!(transaction.postings.len(), 2);
            }
            other => panic!("expected a Transaction entry, got {other:?}"),
        }
    }

    #[test]
    fn defaults_missing_transaction_status_to_uncleared() {
        let source = "2026-09-03 \"Carrefour\" \"Courses\"\n  \
                       Depenses:Alimentation 45.30 EUR\n  \
                       Actifs:Compte-Joint -45.30 EUR\n";

        let journal = parse(source, None).unwrap();

        match &journal.entries[0] {
            Entry::Transaction(transaction) => {
                assert_eq!(transaction.status, TransactionStatus::Uncleared);
            }
            other => panic!("expected a Transaction entry, got {other:?}"),
        }
    }
}
