use chrono::NaiveDate;
use pactole_core::{
    AccountClassification, AccountGrouping, CommodityName, DiffDirection, DiffMode,
    ExpenseGrouping, ExpenseOptions, IncomeGrouping, Period, ReferenceDate, RegisterFilter,
};

/// Market-price valuation requested by a report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValuationRequest {
    /// Commodity into which report amounts should be valued.
    pub target: CommodityName,
    /// Date at which market prices should be selected.
    pub at: NaiveDate,
}

/// Parameters for a balance-sheet query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BalanceQuery {
    /// Inclusive date at which balances are reported.
    pub as_of: ReferenceDate,
    /// Account grouping applied to the report.
    pub grouping: AccountGrouping,
    /// Account classification applied to the report.
    pub classification: AccountClassification,
    /// Optional market-price valuation.
    pub valuation: Option<ValuationRequest>,
}

/// Parameters for an income-statement query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomeQuery {
    /// Period covered by the report.
    pub period: Period,
    /// Grouping applied to income and expenses.
    pub grouping: IncomeGrouping,
    /// Account classification applied to the report.
    pub classification: AccountClassification,
    /// Optional market-price valuation.
    pub valuation: Option<ValuationRequest>,
}

/// Parameters for a comparison between two periods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparisonRequest {
    /// Current period to compare.
    pub period: Period,
    /// Direction used to compute differences.
    pub direction: DiffDirection,
    /// Comparison mode.
    pub mode: DiffMode,
}

/// Parameters for an expense-breakdown query.
#[derive(Debug, Clone)]
pub struct ExpensesQuery {
    /// Period covered by the report.
    pub period: Period,
    /// Grouping applied to expenses.
    pub grouping: ExpenseGrouping,
    /// Expense inclusion and filtering options.
    pub options: ExpenseOptions,
    /// Account classification applied to the report.
    pub classification: AccountClassification,
    /// Optional comparison request.
    pub comparison: Option<ComparisonRequest>,
    /// Optional market-price valuation.
    pub valuation: Option<ValuationRequest>,
}

/// Parameters for a register query.
#[derive(Debug, Clone, PartialEq)]
pub struct RegisterQuery {
    /// Register filtering criteria.
    pub filter: RegisterFilter,
    /// Optional market-price valuation.
    pub valuation: Option<ValuationRequest>,
}

/// Parameters for a diagnostics query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiagnosticsQuery {
    /// Whether warning diagnostics should be included.
    pub include_warnings: bool,
}
