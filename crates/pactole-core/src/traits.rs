use crate::models::Journal;
use std::error::Error;

/// Reads a complete journal from a storage backend.
pub trait ReadableStorage {
    /// Error returned by the storage backend.
    type Error: Error;

    /// Reads the full journal from the underlying storage.
    fn journal(&self) -> Result<Journal, Self::Error>;
}
