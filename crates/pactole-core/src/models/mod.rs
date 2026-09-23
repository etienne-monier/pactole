mod balance;
mod declarations;
mod names;
mod transaction;

pub use balance::{Amount, Balance};
pub use declarations::{Close, Commodity, Open, Payee};
pub use names::{AccountName, CommodityName, Metadata, MetadataKey};
pub use transaction::{Posting, Transaction, TransactionStatus};

// -------------------------------------------------
// Journal structure
// -------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Journal {
    pub entries: Vec<Entry>,
}

impl Journal {
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Open(Open),
    Close(Close),
    Commodity(Commodity),
    Payee(Payee),
    Transaction(Transaction),
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
            Entry::Payee(_) => None,
            Entry::Transaction(transaction) => Some(transaction.date),
            Entry::Balance(balance) => Some(balance.date),
        }
    }
}
