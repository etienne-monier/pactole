#![warn(missing_docs)]
#![warn(clippy::missing_docs_in_private_items)]

//! Pure domain models, validation and reporting for Pactole journals.
//!
//! This crate performs no I/O and has no dependency on the parser or storage
//! layers. A typical consumer builds a [`Journal`], validates it with
//! [`validate_journal`] or [`validate_journal_all`], then produces a register
//! report with [`register`] or a render-agnostic [`Report`]. Monetary values
//! use [`rust_decimal::Decimal`].

/// Stable diagnostic codes shared by frontends.
mod diagnostics;
/// Domain and validation error types.
mod errors;
/// Journal and accounting domain models.
mod models;
/// Register report computation.
mod register;
/// Render-agnostic financial reports.
mod report;
/// Storage traits independent of concrete backends.
mod traits;
/// Journal invariant validation.
mod validation;

pub use crate::diagnostics::DiagnosticCode;
pub use crate::errors::{ModelError, ValidationError};
pub use crate::models::{
    AccountName, Amount, Balance, Close, Commodity, CommodityName, Entry, Journal, Metadata,
    MetadataKey, Open, Payee, Posting, Price, Transaction, TransactionCost,
    TransactionCostAnnotation, TransactionStatus,
};
pub use crate::register::{RegisterEntry, RegisterFilter, register};
pub use crate::report::{
    AccountBalances, AccountClassification, AccountGrouping, Amounts, ComparisonLine,
    ComparisonReport, ComparisonValuation, DiffDirection, DiffMode, ExpenseGrouping,
    ExpenseOptions, IncomeGrouping, Period, ReferenceDate, Report, ReportError, ReportFormat,
    ReportLine, ReportScope, ReportSection, Valuation, ValuationPrice, Valuator, balance_sheet,
    compare_reports, expense_breakdown, expense_breakdown_with_options, income_statement,
    periods_have_same_length, previous_period,
};
pub use crate::traits::ReadableStorage;
pub use crate::validation::{
    MissingData, ValidationIssue, ValidationReport, unused_declarations, unused_declarations_with,
    validate_journal, validate_journal_all, validate_journal_all_with,
};
