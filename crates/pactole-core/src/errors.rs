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
    /// A market price has a zero or negative quote.
    #[error("price for `{base}` must be positive on {date}")]
    PriceNotPositive {
        /// Commodity being priced.
        base: CommodityName,
        /// Date of the invalid quote.
        date: NaiveDate,
    },
    /// Different quotes are declared for the same pair on the same date.
    #[error("conflicting prices for `{base}` in `{quote}` on {date}: `{first}` and `{second}`")]
    PriceConflict {
        /// Commodity being priced.
        base: CommodityName,
        /// Commodity in which the price is quoted.
        quote: CommodityName,
        /// Date of the conflicting quotes.
        date: NaiveDate,
        /// First quote encountered on this date.
        first: Decimal,
        /// Conflicting quote.
        second: Decimal,
    },
    /// A transaction cost annotation has a zero or negative price.
    #[error("transaction cost on account `{account}` must be positive on {date}")]
    TransactionCostNotPositive {
        /// Account carrying the invalid transaction cost.
        account: AccountName,
        /// Date of the transaction.
        date: NaiveDate,
    },
    /// A transaction-cost annotation has no explicit non-zero posting amount,
    /// or a unit cost cannot be converted to an exactly representable total.
    #[error(
        "transaction cost on account `{account}` is inconsistent with its posting amount on {date}"
    )]
    TransactionCostInconsistent {
        /// Account carrying the inconsistent transaction cost.
        account: AccountName,
        /// Date of the transaction.
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
    /// Transaction posting weights do not balance in one commodity.
    #[error(
        "transaction on {date} is not balanced: posting weights for commodity `{commodity}` sum \
         to `{sum}` instead of zero"
    )]
    TransactionNotBalanced {
        /// Commodity whose posting weights do not sum to zero.
        commodity: String,
        /// Non-zero sum of the posting weights.
        sum: Decimal,
        /// Date of the transaction.
        date: NaiveDate,
    },
    /// A transaction has non-zero converted weights in more than one commodity.
    #[error(
        "transaction on {date} is not balanced across commodities: {formatted_imbalances}",
        formatted_imbalances = format_imbalances(.imbalances)
    )]
    MultiCommodityTransactionNotBalanced {
        /// Non-zero weight sums, ordered by commodity.
        imbalances: Vec<(CommodityName, Decimal)>,
        /// Date of the transaction.
        date: NaiveDate,
    },
    /// A posting weight or a sum of transaction weights cannot be represented exactly.
    #[error("transaction weight for `{commodity}` cannot be represented exactly on {date}")]
    TransactionWeightUnrepresentable {
        /// Commodity whose weight cannot be represented exactly.
        commodity: CommodityName,
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
    /// A commodity is declared but never used by an amount, a price directive
    /// or an account restriction.
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

fn format_imbalances(imbalances: &[(CommodityName, Decimal)]) -> String {
    imbalances
        .iter()
        .map(|(commodity, sum)| format!("{commodity} {sum}"))
        .collect::<Vec<_>>()
        .join(", ")
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
            Self::PriceNotPositive { .. } => DiagnosticCode::PriceNotPositive,
            Self::PriceConflict { .. } => DiagnosticCode::PriceConflict,
            Self::TransactionCostNotPositive { .. } => DiagnosticCode::TransactionCostNotPositive,
            Self::TransactionCostInconsistent { .. } => DiagnosticCode::TransactionCostInconsistent,
            Self::PayeeAlreadyDeclared { .. } => DiagnosticCode::PayeeAlreadyDeclared,
            Self::PayeeNotDeclared { .. } => DiagnosticCode::PayeeNotDeclared,
            Self::TooManyPostingsWithoutAmount { .. } => {
                DiagnosticCode::TooManyPostingsWithoutAmount
            }
            Self::AmbiguousAutoBalanceCommodity { .. } => {
                DiagnosticCode::AmbiguousAutoBalanceCommodity
            }
            Self::TransactionNotBalanced { .. } => DiagnosticCode::TransactionNotBalanced,
            Self::MultiCommodityTransactionNotBalanced { .. } => {
                DiagnosticCode::MultiCommodityTransactionNotBalanced
            }
            Self::TransactionWeightUnrepresentable { .. } => {
                DiagnosticCode::TransactionWeightUnrepresentable
            }
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
            ValidationError::PriceNotPositive {
                base: commodity.clone(),
                date,
            },
            ValidationError::PriceConflict {
                base: commodity.clone(),
                quote: CommodityName::new("USD").unwrap(),
                date,
                first: Decimal::ONE,
                second: Decimal::TWO,
            },
            ValidationError::TransactionCostNotPositive {
                account: account.clone(),
                date,
            },
            ValidationError::TransactionCostInconsistent {
                account: account.clone(),
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
            ValidationError::MultiCommodityTransactionNotBalanced {
                imbalances: vec![(commodity.clone(), Decimal::ONE)],
                date,
            },
            ValidationError::TransactionWeightUnrepresentable {
                commodity: commodity.clone(),
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

    #[test]
    fn multi_commodity_imbalance_message_formats_each_weight_clearly() {
        let error = ValidationError::MultiCommodityTransactionNotBalanced {
            imbalances: vec![
                (CommodityName::new("EUR").unwrap(), Decimal::new(92, 0)),
                (CommodityName::new("USD").unwrap(), Decimal::new(-100, 0)),
            ],
            date: NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
        };

        assert_eq!(
            error.to_string(),
            "transaction on 2026-01-02 is not balanced across commodities: EUR 92, USD -100"
        );
    }
}
