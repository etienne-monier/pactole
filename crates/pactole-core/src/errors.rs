use thiserror::Error;

/// Errors raised when building or validating core model values.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ModelError {
    #[error(
        "invalid account name `{0}`: expected colon-separated segments, the first one starting \
         with an uppercase letter, each made of letters, digits, `_` or `-`"
    )]
    InvalidAccountName(String),
    #[error(
        "invalid commodity name `{0}`: expected an uppercase letter followed by uppercase \
         letters, digits, `_`, `.` or `-`"
    )]
    InvalidCommodityName(String),
    #[error(
        "invalid metadata key `{0}`: expected only lowercase letters, `_` or `-`"
    )]
    InvalidMetadataKey(String),
}
