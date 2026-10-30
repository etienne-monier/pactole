#![warn(missing_docs)]
#![warn(clippy::missing_docs_in_private_items)]

//! Transport-independent application use cases for Pactole.
//!
//! This crate orchestrates validated domain journals and the reporting
//! functions from [`pactole_core`]. It does not parse source files, render
//! output, or depend on a concrete storage backend.

/// Backend reader port and validated ledger snapshots.
mod backend;
/// Backend capability declarations and invariants.
mod capabilities;
/// Transport-independent diagnostics.
mod diagnostics;
/// Application-layer errors.
mod errors;
/// Typed report query parameters.
mod queries;
/// Typed report results.
mod results;
/// Application use-case orchestration.
mod service;
/// Future write-side application ports.
mod writer;

pub use backend::{LedgerReader, LedgerSnapshot};
pub use capabilities::{BackendCapabilities, Capability, CapabilityError};
pub use diagnostics::{
    Diagnostic, DiagnosticOrigin, DiagnosticSeverity, SourceId, SourcePoint, SourceSpan,
};
pub use errors::{ApplicationError, BackendError, BackendOperation, InvalidQuery, WriteError};
pub use queries::{
    BalanceQuery, ComparisonRequest, DiagnosticsQuery, ExpensesQuery, IncomeQuery, RegisterQuery,
    ValuationRequest,
};
pub use results::{BalanceResult, DiagnosticsResult, ExpensesResult, IncomeResult, RegisterResult};
pub use service::Application;
pub use service::WritableApplication;
pub use writer::{
    ConflictPolicy, EntryId, LedgerWriter, Revision, ValidationPolicy, WriteOptions, WriteRequest,
    WriteResult, WriteTarget, WriteTransactionRequest,
};
