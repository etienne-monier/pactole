use crate::Diagnostic;
use pactole_core::{ComparisonReport, RegisterEntry, Report, Valuation};

#[derive(Debug, Clone, PartialEq, Eq)]
/// Result of a balance-sheet query.
pub struct BalanceResult {
    /// Generated report.
    pub report: Report,
    /// Diagnostics produced while building the report.
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Result of an income-statement query.
pub struct IncomeResult {
    /// Generated report.
    pub report: Report,
    /// Diagnostics produced while building the report.
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Result of an expense-breakdown query.
pub struct ExpensesResult {
    /// Report for the requested period.
    pub current: Report,
    /// Optional comparison report.
    pub comparison: Option<ComparisonReport>,
    /// Diagnostics produced while building the reports.
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq)]
/// Result of a register query.
pub struct RegisterResult {
    /// Matching register entries.
    pub entries: Vec<RegisterEntry>,
    /// Optional valuation applied to the register.
    pub valuation: Option<Valuation>,
    /// Diagnostics produced while building the register.
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Result of a diagnostics query.
pub struct DiagnosticsResult {
    /// Diagnostics returned by the backend.
    pub diagnostics: Vec<Diagnostic>,
}

impl DiagnosticsResult {
    /// Returns whether at least one error is present.
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_error)
    }

    /// Returns whether at least one warning is present.
    pub fn has_warnings(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_warning)
    }
}
