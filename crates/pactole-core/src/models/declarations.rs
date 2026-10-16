use crate::models::names::{AccountName, CommodityName, Metadata};
use chrono::NaiveDate;

// -------------------------------------------------
// Declarations
// -------------------------------------------------

/// Opens an account on a given date.
#[derive(Debug, Clone, PartialEq)]
pub struct Open {
    /// Date on which the account becomes available.
    pub date: NaiveDate,
    /// Account being opened.
    pub account: AccountName,
    /// Beancount-style optional list of commodities the account is
    /// restricted to, e.g. `EUR` or `EUR,USD`. Empty when the `open`
    /// directive doesn't list any commodity, meaning the account is
    /// unrestricted. When non-empty, [`crate::validation::validate_journal`]
    /// rejects any posting on this account (including auto-balanced ones)
    /// using a commodity outside this list.
    pub commodities: Vec<CommodityName>,
    /// Optional description of the account.
    pub description: Option<String>,
    /// Metadata attached to the directive.
    pub meta: Metadata,
}

/// Closes an account on a given date.
#[derive(Debug, Clone, PartialEq)]
pub struct Close {
    /// Date on which the account becomes unavailable.
    pub date: NaiveDate,
    /// Account being closed.
    pub account: AccountName,
    /// Metadata attached to the directive.
    pub meta: Metadata,
}

/// Declares a commodity on a given date.
#[derive(Debug, Clone, PartialEq)]
pub struct Commodity {
    /// Date of the declaration.
    pub date: NaiveDate,
    /// Declared commodity name.
    pub name: CommodityName,
    /// Metadata attached to the directive.
    pub meta: Metadata,
}

/// Declares a payee as "known" (see `ValidationError::PayeeNotDeclared`).
///
/// Unlike `Open`/`Commodity`, this carries no date: a payee has no
/// temporal life cycle, so only its presence in the journal matters, not
/// where it appears relative to the transactions using it.
#[derive(Debug, Clone, PartialEq)]
pub struct Payee {
    /// Name used by transactions referring to this payee.
    pub name: String,
    /// Metadata attached to the directive.
    pub meta: Metadata,
}
