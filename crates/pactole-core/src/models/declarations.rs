use crate::models::names::{AccountName, CommodityName, Metadata};
use chrono::NaiveDate;

// -------------------------------------------------
// Declarations
// -------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Open {
    pub date: NaiveDate,
    pub account: AccountName,
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
