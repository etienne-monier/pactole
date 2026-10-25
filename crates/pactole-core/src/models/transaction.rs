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
    /// Transaction-specific conversion annotation, retained separately from
    /// the recorded amount and used to derive this posting's balancing weight.
    pub transaction_cost: Option<TransactionCost>,
    /// Metadata attached to the posting.
    pub meta: Metadata,
}

/// A posting's transaction cost in the representation written in the journal.
///
/// `amount` is the exact value written after `@` or `@@`; its meaning depends
/// on `annotation`. Keeping the written value avoids rounding total costs whose
/// equivalent unit price is not exactly representable. The posting amount is
/// needed to derive the other representation when required.
#[derive(Debug, Clone, PartialEq)]
pub struct TransactionCost {
    /// The exact unit or total cost written in the source.
    pub amount: Amount,
    /// Which equivalent representation was present in the source.
    pub annotation: TransactionCostAnnotation,
}

/// The syntax used to express a [`TransactionCost`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionCostAnnotation {
    /// A unit price introduced by `@`.
    Unit,
    /// A total price introduced by `@@`.
    Total,
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
