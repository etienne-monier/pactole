use chrono::NaiveDate;
use thiserror::Error;

use crate::models::CommodityName;

/// Errors raised when building reporting values.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ReportError {
    /// A period starts after it ends.
    #[error("invalid period: start {start} is after end {end}")]
    InvalidPeriod {
        /// First day of the period.
        start: NaiveDate,
        /// Last day of the period.
        end: NaiveDate,
    },
    /// A report amount could not be represented after market-price conversion.
    #[error("valuation overflow converting {from_commodity} to {target}")]
    ValuationOverflow {
        /// Original commodity.
        from_commodity: CommodityName,
        /// Requested report commodity.
        target: CommodityName,
    },
    /// A report already valued in a currency cannot be valued again.
    #[error("report has already been valued")]
    AlreadyValued,
}
