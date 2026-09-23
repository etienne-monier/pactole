use crate::errors::PactoleFsStorageError;
use chrono::NaiveDate;
use pactole_core::{
    AccountName, Amount, Balance, Close, Commodity, CommodityName, Entry, Journal, Metadata,
    MetadataKey, Open, Payee, Posting, Transaction, TransactionStatus,
};
use rust_decimal::Decimal;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tree_sitter::{Node, Parser};

/// Parse the source string and return the Journal object.
///
/// `base_dir` is the directory relative paths in `include` directives are
/// resolved against. It is typically the directory containing the file
/// `source` was read from; pass `None` when parsing a string that has no
/// associated file (relative includes will then fail to resolve).
pub fn parse(source: &str, base_dir: Option<&Path>) -> Result<Journal, PactoleFsStorageError> {
    let entries = parse_entries(source, base_dir)?;
    Ok(Journal { entries })
}

/// Parse the source string into a flat list of entries, recursively
/// inlining the entries of any `include`d file in place of the `include`
/// directive itself.
fn parse_entries(
    source: &str,
    base_dir: Option<&Path>,
) -> Result<Vec<Entry>, PactoleFsStorageError> {
    let mut parser = Parser::new();

    parser
        .set_language(&tree_sitter_pactole::LANGUAGE.into())
        .map_err(|e| PactoleFsStorageError::ParseError(e.to_string()))?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| PactoleFsStorageError::ParseError("failed to parse".to_string()))?;

    let root = tree.root_node();
    let builder = AstBuilder::new(source);

    builder.build_document(root, base_dir)
}

/// The AST builder
///
/// Holds a ref to the source.
struct AstBuilder<'src> {
    source: &'src str,
}

impl<'src> AstBuilder<'src> {
    /// Create a new AstBuilder from source reference.
    fn new(source: &str) -> AstBuilder<'_> {
        AstBuilder { source }
    }

    /// Return the source text spanned by the given node.
    fn text(&self, node: Node<'_>) -> &'src str {
        node.utf8_text(self.source.as_bytes())
            .expect("source is valid UTF-8")
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

    fn parse_date(&self, node: Node<'_>) -> Result<NaiveDate, PactoleFsStorageError> {
        let text = self.text(node);
        NaiveDate::parse_from_str(text, "%Y-%m-%d")
            .map_err(|e| self.error(format!("invalid date `{text}`: {e}")))
    }

    fn parse_number(&self, node: Node<'_>) -> Result<Decimal, PactoleFsStorageError> {
        let text = self.text(node);
        // Numbers may use `,` as a thousands separator (Beancount-like),
        // e.g. `1,234,567.89`; strip them before parsing the decimal.
        let normalized = text.replace(',', "");
        Decimal::from_str(&normalized)
            .map_err(|e| self.error(format!("invalid number `{text}`: {e}")))
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

    fn build_document(
        &self,
        node: Node<'_>,
        base_dir: Option<&Path>,
    ) -> Result<Vec<Entry>, PactoleFsStorageError> {
        let mut entries = Vec::new();

        let mut cursor = node.walk();

        for child in node.named_children(&mut cursor) {
            if child.kind() != "directive" {
                continue;
            }

            self.build_directive(child, base_dir, &mut entries)?;
        }

        Ok(entries)
    }

    fn build_directive(
        &self,
        node: Node<'_>,
        base_dir: Option<&Path>,
        entries: &mut Vec<Entry>,
    ) -> Result<(), PactoleFsStorageError> {
        let header = node
            .named_child(0)
            .ok_or_else(|| self.error("directive has no header"))?;

        match header.kind() {
            "open" => entries.push(Entry::Open(self.build_open(node, header)?)),
            "close" => entries.push(Entry::Close(self.build_close(node, header)?)),
            "commodity" => entries.push(Entry::Commodity(self.build_commodity(node, header)?)),
            "payee_declaration" => {
                entries.push(Entry::Payee(self.build_payee_declaration(node, header)?))
            }
            "transaction" => {
                entries.push(Entry::Transaction(self.build_transaction(node, header)?))
            }
            "balance" => entries.push(Entry::Balance(self.build_balance(node, header)?)),
            "include" => entries.extend(self.build_include(header, base_dir)?),
            other => return Err(self.error(format!("unexpected directive header `{other}`"))),
        }

        Ok(())
    }

    fn build_open(
        &self,
        directive: Node<'_>,
        open: Node<'_>,
    ) -> Result<Open, PactoleFsStorageError> {
        let date = self.parse_date(self.require_child(open, "date")?)?;
        let account = self.parse_account(self.require_child(open, "account")?)?;

        let mut meta = self.build_metadata(directive)?;
        let description = meta.remove("description");

        Ok(Open {
            date,
            account,
            description,
            meta,
        })
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

    /// Resolve an `include` directive by reading and parsing the included
    /// file, returning its entries so they can be inlined in place of the
    /// directive itself. The included path is resolved relative to
    /// `base_dir` (the directory of the file currently being parsed).
    fn build_include(
        &self,
        include: Node<'_>,
        base_dir: Option<&Path>,
    ) -> Result<Vec<Entry>, PactoleFsStorageError> {
        let path_node = self.require_child(include, "path")?;
        let raw_path = self.parse_quoted_text(path_node)?;
        let path = PathBuf::from(&raw_path);

        let resolved = if path.is_absolute() {
            path
        } else {
            let base_dir = base_dir.ok_or_else(|| {
                self.error(format!(
                    "cannot resolve relative include `{raw_path}` without a base directory"
                ))
            })?;
            base_dir.join(path)
        };

        let included_source = fs::read_to_string(&resolved).map_err(|e| {
            self.error(format!(
                "failed to read included file `{}`: {e}",
                resolved.display()
            ))
        })?;

        let included_base_dir = resolved.parent().map(Path::to_path_buf);

        parse_entries(&included_source, included_base_dir.as_deref())
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

        let status = self.parse_status(self.require_child(transaction, "status")?)?;

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
