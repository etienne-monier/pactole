use crate::diagnostics::DiagnosticCode;
use crate::models::{AccountName, CommodityName};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use thiserror::Error;

/// Errors raised when building core model values: purely grammatical /
/// structural checks (e.g. the shape of a name), with no knowledge of the
/// rest of the journal.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ModelError {
    /// An account name does not satisfy the Pactole account grammar.
    #[error(
        "invalid account name `{0}`: expected colon-separated segments, the first one starting \
         with an uppercase letter, each made of letters, digits, `_` or `-`"
    )]
    InvalidAccountName(String),
    /// A commodity name does not satisfy the Pactole commodity grammar.
    #[error(
        "invalid commodity name `{0}`: expected 2 to 24 characters, starting with an uppercase \
         letter, ending with an uppercase letter or digit, made of uppercase letters, digits, \
         `'`, `.`, `_` or `-` in between"
    )]
    InvalidCommodityName(String),
    /// A metadata key does not satisfy the lowercase-key grammar.
    #[error("invalid metadata key `{0}`: expected only lowercase letters, `_` or `-`")]
    InvalidMetadataKey(String),
}

/// Errors raised when validating a [`crate::Journal`] as a whole: checks
/// that require knowing about other entries of the journal, as opposed to
/// the purely grammatical checks reported as [`ModelError`].
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// An account is opened while it is already open.
    #[error("account `{account}` is already open on {date}")]
    AccountAlreadyOpen {
        /// Account name involved in the duplicate opening.
        account: AccountName,
        /// Date of the duplicate opening.
        date: NaiveDate,
    },
    /// An account is used or closed while it is not open.
    #[error("account `{account}` is not open on {date}")]
    AccountNotOpen {
        /// Account name that was used while closed or unopened.
        account: AccountName,
        /// Date of the invalid use.
        date: NaiveDate,
    },
    /// A commodity is declared more than once.
    #[error("commodity `{commodity}` is already declared on {date}")]
    CommodityAlreadyDeclared {
        /// Commodity declared more than once.
        commodity: CommodityName,
        /// Date of the duplicate declaration.
        date: NaiveDate,
    },
    /// A commodity is used before being declared.
    #[error("commodity `{commodity}` is not declared on {date}")]
    CommodityNotDeclared {
        /// Commodity used before its declaration.
        commodity: CommodityName,
        /// Date of the invalid use.
        date: NaiveDate,
    },
    /// A payee is declared more than once.
    #[error("payee `{payee}` is already declared")]
    PayeeAlreadyDeclared {
        /// Payee declared more than once.
        payee: String,
    },
    /// A transaction uses a payee that has not been declared.
    #[error("payee `{payee}` is not declared (transaction on {date})")]
    PayeeNotDeclared {
        /// Payee used by the transaction.
        payee: String,
        /// Date of the transaction.
        date: NaiveDate,
    },
    /// More than one transaction posting has no amount.
    #[error(
        "transaction on {date} has {count} postings without an amount, but at most one posting \
         can be left without an amount for auto-balancing"
    )]
    TooManyPostingsWithoutAmount {
        /// Number of postings without an amount.
        count: usize,
        /// Date of the transaction.
        date: NaiveDate,
    },
    /// The commodity of an inferred amount cannot be determined uniquely.
    #[error(
        "cannot auto-balance transaction: unable to determine a single commodity for the \
         missing amount from the other postings on {date} (commodities found: {commodities:?})"
    )]
    AmbiguousAutoBalanceCommodity {
        /// Commodities found among the explicitly valued postings.
        commodities: Vec<String>,
        /// Date of the transaction.
        date: NaiveDate,
    },
    /// Explicit transaction postings do not balance.
    #[error(
        "transaction on {date} is not balanced: postings for commodity `{commodity}` sum to \
         `{sum}` instead of zero"
    )]
    TransactionNotBalanced {
        /// Commodity whose postings do not sum to zero.
        commodity: String,
        /// Non-zero sum of the postings.
        sum: Decimal,
        /// Date of the transaction.
        date: NaiveDate,
    },
    /// A posting uses a commodity excluded by the account's opening.
    #[error(
        "commodity `{commodity}` is not allowed on account `{account}` (posting on {date}): the \
         account was opened with a restricted list of commodities that does not include it"
    )]
    CommodityNotAllowed {
        /// Account carrying the restricted commodity.
        account: AccountName,
        /// Commodity excluded by the account opening.
        commodity: CommodityName,
        /// Date of the posting.
        date: NaiveDate,
    },
    /// A balance assertion differs from the calculated balance beyond tolerance.
    #[error(
        "balance assertion failed on {date}: account `{account}` has a balance of `{actual}` \
         {commodity}, expected `{expected}` {commodity} (tolerance ~ {tolerance})"
    )]
    BalanceAssertionFailed {
        /// Account whose balance was asserted.
        account: AccountName,
        /// Commodity of the assertion.
        commodity: CommodityName,
        /// Date of the assertion.
        date: NaiveDate,
        /// Expected balance.
        expected: Decimal,
        /// Calculated balance.
        actual: Decimal,
        /// Allowed difference.
        tolerance: Decimal,
    },
    /// A payee declaration is never used by any transaction.
    ///
    /// This is a cleanup hint rather than a correctness problem: front-ends
    /// decide its severity (the `unused` key of `[validation]`), so it is
    /// reported as a warning by default.
    #[error("payee `{payee}` is declared but never used by any transaction")]
    PayeeUnused {
        /// Declared payee that no transaction uses.
        payee: String,
    },
    /// An account is opened but never used by a transaction or a balance
    /// assertion (merely closing it does not count as a use).
    ///
    /// Like [`Self::PayeeUnused`], this is a cleanup hint whose severity is
    /// decided by front-ends.
    #[error("account `{account}` is opened on {date} but never used")]
    AccountUnused {
        /// Opened account that no transaction nor balance assertion uses.
        account: AccountName,
        /// Date of the opening.
        date: NaiveDate,
    },
    /// A commodity is declared but never used by any amount, nor restricted
    /// to an account by an `open` directive.
    ///
    /// Like [`Self::PayeeUnused`], this is a cleanup hint whose severity is
    /// decided by front-ends.
    #[error("commodity `{commodity}` is declared on {date} but never used")]
    CommodityUnused {
        /// Declared commodity that no amount uses.
        commodity: CommodityName,
        /// Date of the declaration.
        date: NaiveDate,
    },
}

impl ValidationError {
    /// Returns the stable diagnostic code identifying this kind of error.
    ///
    /// The code is independent of the message and of the severity a front-end
    /// gives it (the `[validation]` levels), so it is safe to expose to
    /// scripts and editors.
    pub fn code(&self) -> DiagnosticCode {
        match self {
            Self::AccountAlreadyOpen { .. } => DiagnosticCode::AccountAlreadyOpen,
            Self::AccountNotOpen { .. } => DiagnosticCode::AccountNotOpen,
            Self::CommodityAlreadyDeclared { .. } => DiagnosticCode::CommodityAlreadyDeclared,
            Self::CommodityNotDeclared { .. } => DiagnosticCode::CommodityNotDeclared,
            Self::PayeeAlreadyDeclared { .. } => DiagnosticCode::PayeeAlreadyDeclared,
            Self::PayeeNotDeclared { .. } => DiagnosticCode::PayeeNotDeclared,
            Self::TooManyPostingsWithoutAmount { .. } => {
                DiagnosticCode::TooManyPostingsWithoutAmount
            }
            Self::AmbiguousAutoBalanceCommodity { .. } => {
                DiagnosticCode::AmbiguousAutoBalanceCommodity
            }
            Self::TransactionNotBalanced { .. } => DiagnosticCode::TransactionNotBalanced,
            Self::CommodityNotAllowed { .. } => DiagnosticCode::CommodityNotAllowed,
            Self::BalanceAssertionFailed { .. } => DiagnosticCode::BalanceAssertionFailed,
            Self::AccountUnused { .. } => DiagnosticCode::AccountUnused,
            Self::CommodityUnused { .. } => DiagnosticCode::CommodityUnused,
            Self::PayeeUnused { .. } => DiagnosticCode::PayeeUnused,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DiagnosticCode, ValidationError};
    use crate::models::{AccountName, CommodityName};
    use chrono::NaiveDate;
    use rust_decimal::Decimal;
    use std::collections::HashSet;

    /// One error of every business variant.
    fn every_validation_error() -> Vec<ValidationError> {
        let account = AccountName::new("Assets:Checking").unwrap();
        let commodity = CommodityName::new("EUR").unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let payee = "Shop".to_string();

        vec![
            ValidationError::AccountAlreadyOpen {
                account: account.clone(),
                date,
            },
            ValidationError::AccountNotOpen {
                account: account.clone(),
                date,
            },
            ValidationError::CommodityAlreadyDeclared {
                commodity: commodity.clone(),
                date,
            },
            ValidationError::CommodityNotDeclared {
                commodity: commodity.clone(),
                date,
            },
            ValidationError::PayeeAlreadyDeclared {
                payee: payee.clone(),
            },
            ValidationError::PayeeNotDeclared {
                payee: payee.clone(),
                date,
            },
            ValidationError::TooManyPostingsWithoutAmount { count: 2, date },
            ValidationError::AmbiguousAutoBalanceCommodity {
                commodities: vec!["EUR".to_string(), "USD".to_string()],
                date,
            },
            ValidationError::TransactionNotBalanced {
                commodity: "EUR".to_string(),
                sum: Decimal::ONE,
                date,
            },
            ValidationError::CommodityNotAllowed {
                account: account.clone(),
                commodity: commodity.clone(),
                date,
            },
            ValidationError::BalanceAssertionFailed {
                account: account.clone(),
                commodity: commodity.clone(),
                date,
                expected: Decimal::ZERO,
                actual: Decimal::ONE,
                tolerance: Decimal::ZERO,
            },
            ValidationError::AccountUnused { account, date },
            ValidationError::CommodityUnused { commodity, date },
            ValidationError::PayeeUnused { payee },
        ]
    }

    #[test]
    fn every_business_error_has_a_distinct_business_code() {
        let codes: Vec<DiagnosticCode> = every_validation_error()
            .iter()
            .map(ValidationError::code)
            .collect();

        let unique: HashSet<DiagnosticCode> = codes.iter().copied().collect();
        assert_eq!(unique.len(), codes.len(), "two errors share a code");

        // Every code is a business one, never a syntax, lowering or include
        // code.
        assert!(codes.iter().all(|code| {
            !matches!(
                code,
                DiagnosticCode::SyntaxError
                    | DiagnosticCode::LoweringError
                    | DiagnosticCode::IncludeNotFound
                    | DiagnosticCode::IncludeLoadFailed
                    | DiagnosticCode::IncludeNoBaseDirectory
                    | DiagnosticCode::IncludeDepthExceeded
                    | DiagnosticCode::IncludeCycle
            )
        }));
    }
}
