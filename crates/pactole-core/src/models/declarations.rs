use crate::models::names::{AccountName, CommodityName, Metadata};
use chrono::NaiveDate;

// -------------------------------------------------
// Declarations
// -------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Open {
    pub date: NaiveDate,
    pub account: AccountName,
    /// Beancount-style optional list of commodities the account is
    /// restricted to, e.g. `EUR` or `EUR,USD`. Empty when the `open`
    /// directive doesn't list any commodity, meaning the account is
    /// unrestricted. When non-empty, [`crate::validation::validate_journal`]
    /// rejects any posting on this account (including auto-balanced ones)
    /// using a commodity outside this list.
    pub commodities: Vec<CommodityName>,
    pub description: Option<String>,
    pub meta: Metadata,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Close {
    pub date: NaiveDate,
    pub account: AccountName,
    pub meta: Metadata,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Commodity {
    pub date: NaiveDate,
    pub name: CommodityName,
    pub meta: Metadata,
}

/// Declares a payee as "known" (see `ValidationError::PayeeNotDeclared`).
///
/// Unlike `Open`/`Commodity`, this carries no date: a payee has no
/// temporal life cycle, so only its presence in the journal matters, not
/// where it appears relative to the transactions using it.
#[derive(Debug, Clone, PartialEq)]
pub struct Payee {
    pub name: String,
    pub meta: Metadata,
}
