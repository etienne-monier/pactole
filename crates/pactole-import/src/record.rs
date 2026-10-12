use chrono::NaiveDate;
use rust_decimal::Decimal;

/// A format-neutral transaction record extracted from an external source
/// (currently OFX).
///
/// This is the intermediate representation between a concrete input format
/// and the Pactole domain: format-specific parsers produce [`ImportRecord`]s,
/// and the shared rules engine (internal to this crate) turns them into
/// [`pactole_core::Transaction`]s. Keeping this type free of any
/// format-specific dependency is what lets future importers (CSV, QIF, ...)
/// reuse the same rule evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRecord {
    /// Date the transaction was posted.
    pub date: NaiveDate,

    /// Signed amount, relative to the imported (bank/credit card) account: a
    /// negative amount is money leaving that account.
    pub amount: Decimal,

    /// Raw currency code from the source (e.g. `EUR`), if any.
    pub currency: Option<String>,

    /// Raw payee / merchant name from the source, if any.
    pub payee: Option<String>,

    /// Raw memo / narration from the source, if any.
    pub memo: Option<String>,

    /// Raw source transaction type (e.g. `DEBIT`, `POS`), if any. Kept as a
    /// plain string so the rules engine does not depend on any format
    /// parser's types.
    pub transaction_type: Option<String>,

    /// Raw check number, if any.
    pub check_number: Option<String>,

    /// Raw reference number, if any.
    pub reference: Option<String>,

    /// Source-provided unique transaction identifier, used for idempotent
    /// re-imports. For OFX this is the `FITID`.
    pub fitid: Option<String>,
}
