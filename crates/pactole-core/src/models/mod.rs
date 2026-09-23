mod balance;
mod declarations;
mod names;
mod transaction;

pub use balance::{Amount, Balance};
pub use declarations::{Close, Commodity, Open};
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
    Transaction(Transaction),
    Balance(Balance),
}
