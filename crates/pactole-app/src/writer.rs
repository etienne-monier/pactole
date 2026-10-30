use pactole_core::Transaction;

use crate::{Diagnostic, SourceId};

/// Identifies an entry without exposing a backend-specific representation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EntryId(pub String);

/// Identifies the version read before a write.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Revision(pub String);

/// Selects where a write should be persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteTarget {
    /// The backend's default writable target.
    Default,
    /// A specific source selected by the caller.
    Source(SourceId),
}

/// Controls the treatment of warnings during validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidationPolicy {
    /// Warnings do not prevent the write.
    #[default]
    AllowWarnings,
    /// Warnings prevent the write just like errors.
    RejectWarnings,
}

/// Controls how a stale revision is handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConflictPolicy {
    /// Refuse the operation when the backend revision changed.
    #[default]
    Reject,
}

/// Options governing a write operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WriteOptions {
    /// Policy applied to validation warnings.
    pub validation: ValidationPolicy,
    /// Policy applied when the backend revision is stale.
    pub conflict: ConflictPolicy,
}

/// Request to write one transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct WriteTransactionRequest {
    /// Transaction to persist.
    pub transaction: Transaction,
    /// Destination selected for the write.
    pub target: WriteTarget,
    /// Revision observed by the caller, when available.
    pub expected_revision: Option<Revision>,
    /// Options controlling validation and conflict handling.
    pub options: WriteOptions,
}

/// Write operations supported by the application boundary.
#[derive(Debug, Clone, PartialEq)]
pub enum WriteRequest {
    /// Persist one transaction.
    Transaction(WriteTransactionRequest),
}

/// Result of a successful write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteResult {
    /// Backend-assigned entry identity, when available.
    pub entry_id: Option<EntryId>,
    /// Revision after the write.
    pub revision: Revision,
    /// Target used by the backend.
    pub target: WriteTarget,
    /// Diagnostics returned by the write operation.
    pub diagnostics: Vec<Diagnostic>,
}

/// A future-facing writer port. No concrete Pactole backend implements this
/// port yet; read-only backends must continue to work without it.
pub trait LedgerWriter {
    /// Returns the capabilities advertised by the writer backend.
    fn capabilities(&self) -> crate::BackendCapabilities;

    /// Persists a write request.
    fn write(&mut self, request: WriteRequest) -> Result<WriteResult, crate::WriteError>;
}
