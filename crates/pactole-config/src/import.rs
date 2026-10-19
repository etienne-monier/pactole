//! Configuration types for importing external statements (currently OFX).
//!
//! These types only describe the mapping; they perform no I/O and know
//! nothing about where the configuration comes from. The TOML shape of the
//! `[import]` section of `pactole.toml` is defined in [`crate::raw`] and
//! merged into these types by [`crate::Config`]'s resolution.

use std::collections::BTreeMap;

/// Configuration for importing an external statement (currently OFX) into
/// Pactole transactions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportConfig {
    /// The Pactole account corresponding to the statement being imported
    /// (e.g. `Assets:Bank`). Money movements are recorded against this
    /// account with the signed amount from the source.
    ///
    /// Used as the fallback for transactions whose source account is not
    /// covered by [`ImportConfig::accounts`] (nor by a CLI override).
    pub bank_account: String,

    /// Mapping of source account identifiers (for OFX: the statement's
    /// `ACCTID`) to Pactole account names, for statements covering several
    /// physical accounts. A transaction whose source account is absent from
    /// this table falls back to [`ImportConfig::bank_account`] (with a
    /// warning).
    pub accounts: BTreeMap<String, String>,

    /// Fallback commodity (e.g. `EUR`) used when the source transaction
    /// carries no currency of its own.
    pub currency: Option<String>,

    /// Counter account used for transactions that no [`Rule`] matches (e.g.
    /// `Expenses:To-Be-Classified`). This posting is the "unknown category"
    /// side of the transaction, to be categorized later.
    pub default_counter_account: String,

    /// Rules evaluated in order, first match wins, used to determine the
    /// counter account and optionally the canonical payee of each
    /// transaction.
    pub rules: Vec<Rule>,
}

/// A single import rule: a set of optional, conjunctive match criteria over
/// an imported record, plus the counter account (and optional canonical
/// payee) to use when it matches.
///
/// A rule with no criteria matches every transaction (useful as a
/// catch-all placed after more specific rules).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// Case-insensitive substring matched against the source payee/name.
    pub payee_contains: Option<String>,

    /// Case-insensitive substring matched against the source memo.
    pub memo_contains: Option<String>,

    /// Source transaction type to match exactly, case-insensitively (e.g.
    /// `DEBIT`, `POS`, `XFER`).
    pub trntype: Option<String>,

    /// Direction of the money movement to match.
    pub direction: Option<Direction>,

    /// Pactole counter account to post the opposite side of a matching
    /// transaction to (e.g. `Depenses:Alimentation`).
    pub counter_account: String,

    /// Optional canonical payee to use instead of the raw source payee.
    pub payee: Option<String>,
}

/// The direction of a money movement, as seen from the imported account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Money leaving the imported account (negative amount).
    Debit,
    /// Money entering the imported account (positive amount).
    Credit,
}
