use crate::models::{AccountName, CommodityName};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use thiserror::Error;

/// Errors raised when building core model values: purely grammatical /
/// structural checks (e.g. the shape of a name), with no knowledge of the
/// rest of the journal.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ModelError {
    #[error(
        "invalid account name `{0}`: expected colon-separated segments, the first one starting \
         with an uppercase letter, each made of letters, digits, `_` or `-`"
    )]
    InvalidAccountName(String),
    #[error(
        "invalid commodity name `{0}`: expected 2 to 24 characters, starting with an uppercase \
         letter, ending with an uppercase letter or digit, made of uppercase letters, digits, \
         `'`, `.`, `_` or `-` in between"
    )]
    InvalidCommodityName(String),
    #[error(
        "invalid metadata key `{0}`: expected only lowercase letters, `_` or `-`"
    )]
    InvalidMetadataKey(String),
}

/// Errors raised when validating a [`crate::Journal`] as a whole: checks
/// that require knowing about other entries of the journal, as opposed to
/// the purely grammatical checks reported as [`ModelError`].
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    #[error("account `{account}` is not open on {date}")]
    AccountNotOpen { account: AccountName, date: NaiveDate },
    #[error("commodity `{commodity}` is not declared on {date}")]
    CommodityNotDeclared {
        commodity: CommodityName,
        date: NaiveDate,
    },
    #[error("payee `{payee}` is not declared (transaction on {date})")]
    PayeeNotDeclared { payee: String, date: NaiveDate },
    #[error(
        "transaction has {0} postings without an amount, but at most one posting can be left \
         without an amount for auto-balancing"
    )]
    TooManyPostingsWithoutAmount(usize),
    #[error(
        "cannot auto-balance transaction: unable to determine a single commodity for the \
         missing amount from the other postings (commodities found: {0:?})"
    )]
    AmbiguousAutoBalanceCommodity(Vec<String>),
    #[error(
        "transaction is not balanced: postings for commodity `{0}` sum to `{1}` instead of zero"
    )]
    TransactionNotBalanced(String, Decimal),
}
