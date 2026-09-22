use thiserror::Error;

#[derive(Error, Debug)]
pub enum PactoleFsStorageError {
    #[error("parsing error `{0}`")]
    ParseError(String),
    #[error("invalid model: {0}")]
    Model(#[from] pactole_core::ModelError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("journal has not been parsed yet")]
    NotParsed,
    #[error("unknown data store error")]
    Unknown,
}
