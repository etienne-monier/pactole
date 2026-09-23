use crate::models::names::{AccountName, CommodityName, Metadata};
use chrono::NaiveDate;
use rust_decimal::Decimal;

// -------------------------------------------------
// Assertions
// -------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Balance {
    pub date: NaiveDate,
    pub account: AccountName,
    pub amount: Amount,
    pub tolerance: Option<Decimal>,
    pub meta: Metadata,
}

// -------------------------------------------------
// Misc
// -------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Amount {
    pub number: Decimal,
    pub commodity: CommodityName,
}
