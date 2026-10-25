//! Domain entities used by Pactole journals.

mod balance;
mod declarations;
mod names;
mod transaction;

pub use balance::{Amount, Balance};
pub use declarations::{Close, Commodity, Open, Payee, Price};
pub use names::{AccountName, CommodityName, Metadata, MetadataKey};
pub use transaction::{
    Posting, Transaction, TransactionCost, TransactionCostAnnotation, TransactionStatus,
};

// -------------------------------------------------
// Journal structure
// -------------------------------------------------

/// A collection of top-level Pactole directives.
#[derive(Debug, Clone, PartialEq)]
pub struct Journal {
    /// Directives in their current order. Validation returns a chronologically
    /// sorted journal while preserving source order before validation.
    pub entries: Vec<Entry>,
}

impl Journal {
    /// Returns the directives in this journal in their current order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
}

/// A top-level directive in a Pactole journal.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    /// Opens an account.
    Open(Open),
    /// Closes an account.
    Close(Close),
    /// Declares a commodity.
    Commodity(Commodity),
    /// Records a dated market price between two commodities.
    Price(Price),
    /// Declares a known payee.
    Payee(Payee),
    /// Records a transaction and its postings.
    Transaction(Transaction),
    /// Asserts an account balance.
    Balance(Balance),
}

impl Entry {
    /// Returns the date this entry is recorded at, used to sort a journal
    /// chronologically before validating it.
    ///
    /// Returns `None` for a [`Payee`] declaration, which carries no date
    /// (see its doc comment): entries with no date sort before every
    /// dated entry (a stable sort keeps their relative order), so a
    /// payee is always known by the time any transaction is validated,
    /// regardless of where in the file it was declared.
    pub fn date(&self) -> Option<chrono::NaiveDate> {
        match self {
            Entry::Open(open) => Some(open.date),
            Entry::Close(close) => Some(close.date),
            Entry::Commodity(commodity) => Some(commodity.date),
            Entry::Price(price) => Some(price.date),
            Entry::Payee(_) => None,
            Entry::Transaction(transaction) => Some(transaction.date),
            Entry::Balance(balance) => Some(balance.date),
        }
    }
}
