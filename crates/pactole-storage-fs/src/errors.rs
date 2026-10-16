//! Errors returned by filesystem-backed parsing and formatting.

use thiserror::Error;

/// Errors raised while loading, parsing or querying a Pactole file storage.
#[derive(Error, Debug)]
pub enum PactoleFsStorageError {
    /// A syntax, lowering or project-analysis error.
    #[error("parsing error `{0}`")]
    ParseError(String),
    /// A core model value could not be built.
    #[error("invalid model: {0}")]
    Model(#[from] pactole_core::ModelError),
    /// Reading a source file failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The caller requested a journal before calling `parse`.
    #[error("journal has not been parsed yet")]
    NotParsed,
    /// An unspecified storage failure.
    #[error("unknown data store error")]
    Unknown,
}
