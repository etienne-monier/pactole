use crate::models::balance::Amount;
use crate::models::names::{AccountName, Metadata};
use chrono::NaiveDate;

// -------------------------------------------------
// Events
// -------------------------------------------------

/// A balanced financial transaction with one or more postings.
#[derive(Debug, Clone, PartialEq)]
pub struct Transaction {
    /// Posting date.
    pub date: NaiveDate,
    /// Optional effective date, distinct from the posting date.
    pub effective_date: Option<NaiveDate>,

    /// Reconciliation status of the transaction.
    pub status: TransactionStatus,
    /// Optional declared payee.
    pub payee: Option<String>,
    /// Optional human-readable narration.
    pub narration: Option<String>,

    /// Accounts and amounts making up the transaction.
    pub postings: Vec<Posting>,

    /// Tags attached to the transaction.
    pub tags: Vec<String>,
    /// Links attached to the transaction.
    pub links: Vec<String>,
    /// Optional external reference.
    pub reference: Option<String>,
    /// Metadata attached to the transaction.
    pub meta: Metadata,
}

/// One account-side movement within a transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct Posting {
    /// Account affected by the posting.
    pub account: AccountName,
    /// Amount and commodity. `None` may be inferred during validation when
    /// there is exactly one missing posting amount.
    pub amount: Option<Amount>,
    /// Metadata attached to the posting.
    pub meta: Metadata,
}

/// Reconciliation status of a transaction.
#[derive(Debug, Clone, PartialEq)]
pub enum TransactionStatus {
    /// The transaction has not been cleared.
    Uncleared,
    /// The transaction is pending.
    Pending,
    /// The transaction has cleared.
    Cleared,
}
