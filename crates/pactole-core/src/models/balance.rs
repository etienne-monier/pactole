use crate::models::names::{AccountName, CommodityName, Metadata};
use chrono::NaiveDate;
use rust_decimal::Decimal;

// -------------------------------------------------
// Assertions
// -------------------------------------------------

/// Expected balance of an account on a date.
#[derive(Debug, Clone, PartialEq)]
pub struct Balance {
    /// Date of the assertion.
    pub date: NaiveDate,
    /// Account whose balance is asserted.
    pub account: AccountName,
    /// Expected amount.
    pub amount: Amount,
    /// Optional allowed difference. Without it, the assertion must match
    /// exactly.
    pub tolerance: Option<Decimal>,
    /// Metadata attached to the directive.
    pub meta: Metadata,
}

// -------------------------------------------------
// Misc
// -------------------------------------------------

/// A decimal quantity paired with its commodity.
#[derive(Debug, Clone, PartialEq)]
pub struct Amount {
    /// Numeric quantity, represented exactly as a decimal.
    pub number: Decimal,
    /// Commodity in which the quantity is expressed.
    pub commodity: CommodityName,
}
