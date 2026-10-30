use crate::{Capability, Diagnostic, EntryId, Revision};
use pactole_core::ReportError;
use thiserror::Error;

/// Operation that failed in a backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendOperation {
    /// Load the current ledger snapshot.
    LoadSnapshot,
}

/// Error raised while accessing a backend.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct BackendError {
    /// Backend operation that failed.
    pub operation: BackendOperation,
    /// Human-readable explanation of the failure.
    pub message: String,
}

/// Invalid semantic application query.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InvalidQuery {
    /// Comparison is not supported for monthly expense grouping.
    #[error("comparison is not supported when grouping expenses by month")]
    ComparisonByMonth,
    /// Comparison cannot be combined with the selected expense options.
    #[error("comparison cannot be combined with uncategorized or inactive expense options")]
    ComparisonWithExpenseOptions,
    /// The compared periods have different lengths.
    #[error("comparison periods do not have the same length")]
    ComparisonPeriodsHaveDifferentLengths,
    /// The requested valuation commodity is not declared.
    #[error("commodity {0} is not declared in the journal")]
    UndeclaredValuationCommodity(String),
}

/// Error raised while executing an application use case.
#[derive(Debug, Error)]
pub enum ApplicationError {
    /// Loading the backend snapshot failed.
    #[error(transparent)]
    Backend(#[from] BackendError),
    /// The backend lacks a capability required by the operation.
    #[error("backend does not support {capability:?}")]
    CapabilityUnavailable {
        /// Capability required by the operation.
        capability: Capability,
    },
    /// The query is semantically invalid.
    #[error(transparent)]
    InvalidQuery(#[from] InvalidQuery),
    /// The ledger cannot be used because it has error diagnostics.
    #[error("the ledger is unavailable because its diagnostics contain errors")]
    LedgerUnavailable {
        /// Diagnostics that make the ledger unavailable.
        diagnostics: Vec<Diagnostic>,
    },
    /// The operation rejected warning diagnostics.
    #[error("writing rejected because warnings are not allowed")]
    WarningsRejected {
        /// Warnings that caused the operation to be rejected.
        diagnostics: Vec<Diagnostic>,
    },
    /// A concrete writer rejected the operation.
    #[error(transparent)]
    Write(#[from] WriteError),
    /// Report computation failed.
    #[error(transparent)]
    Report(#[from] ReportError),
}

/// Error returned by a concrete writer.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WriteError {
    /// The expected revision differs from the backend revision.
    #[error("write conflict: expected revision {expected:?}, found {actual:?}")]
    Conflict {
        /// Revision supplied by the caller.
        expected: Revision,
        /// Revision currently held by the backend.
        actual: Revision,
    },
    /// The requested write target is unavailable.
    #[error("write target is unavailable")]
    TargetUnavailable,
    /// The requested target is read-only.
    #[error("write target is read-only")]
    TargetReadOnly,
    /// The entry identity already exists.
    #[error("entry identity `{0:?}` already exists")]
    IdentityConflict(EntryId),
    /// The candidate journal failed validation.
    #[error("validation failed")]
    ValidationFailed {
        /// Diagnostics describing the validation failure.
        diagnostics: Vec<Diagnostic>,
    },
    /// Formatting the candidate write failed.
    #[error("formatting failed: {0}")]
    FormattingFailed(String),
    /// Persisting the write atomically failed.
    #[error("atomic write failed: {0}")]
    AtomicCommitFailed(String),
}
