use chrono::NaiveDate;
use thiserror::Error;

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
}
