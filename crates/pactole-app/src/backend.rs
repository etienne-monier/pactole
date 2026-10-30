use crate::{BackendCapabilities, BackendError, Diagnostic, Revision};
use pactole_core::Journal;

/// A complete, validated ledger or the diagnostics explaining why one is not
/// available.
#[derive(Debug, Clone, PartialEq)]
pub enum LedgerSnapshot {
    /// The journal is safe to use for application reports. Diagnostics here
    /// are warnings only.
    Ready {
        /// Validated journal contents.
        journal: Journal,
        /// Backend revision from which the journal was read.
        revision: Revision,
        /// Non-blocking diagnostics produced while loading the journal.
        diagnostics: Vec<Diagnostic>,
    },
    /// The backend could analyze enough content to provide diagnostics, but it
    /// could not provide a complete validated journal.
    Unavailable {
        /// Diagnostics explaining why the journal is unavailable.
        diagnostics: Vec<Diagnostic>,
    },
}

impl LedgerSnapshot {
    /// Returns all diagnostics associated with the snapshot.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        match self {
            Self::Ready { diagnostics, .. } | Self::Unavailable { diagnostics } => diagnostics,
        }
    }
}

/// Port implemented by concrete ledger readers.
pub trait LedgerReader {
    /// Returns the capabilities advertised by the backend.
    fn capabilities(&self) -> BackendCapabilities;

    /// Loads the current ledger snapshot.
    fn snapshot(&self) -> Result<LedgerSnapshot, BackendError>;
}
