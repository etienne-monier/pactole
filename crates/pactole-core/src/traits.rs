use crate::models::Journal;
use std::error::Error;

pub trait ReadableStorage {
    type Error: Error;

    /// Reads the full journal from the underlying storage.
    fn journal(&self) -> Result<Journal, Self::Error>;
}
