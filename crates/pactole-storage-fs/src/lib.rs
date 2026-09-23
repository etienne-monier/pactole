use pactole_core::{Journal, ReadableStorage};
use std::fs;
use std::path::PathBuf;
pub mod errors;
mod parser;
mod printer;

pub use crate::errors::PactoleFsStorageError;
pub use crate::printer::format;

pub struct PactoleFileStorage {
    pub filepath: Option<PathBuf>,
    pub content: String,
    pub journal: Option<Journal>,
}

impl PactoleFileStorage {
    pub fn parse(self: &mut Self) -> Result<(), errors::PactoleFsStorageError> {
        let base_dir = self.filepath.as_deref().and_then(std::path::Path::parent);
        self.journal = Some(parser::parse(&self.content, base_dir)?);
        Ok(())
    }
}

impl TryFrom<PathBuf> for PactoleFileStorage {
    type Error = errors::PactoleFsStorageError;

    fn try_from(value: PathBuf) -> Result<Self, Self::Error> {
        let contents = fs::read_to_string(&value)?;
        Ok(Self {
            filepath: Some(value),
            content: contents,
            journal: None,
        })
    }
}

impl ReadableStorage for PactoleFileStorage {
    type Error = PactoleFsStorageError;

    fn journal(&self) -> Result<Journal, Self::Error> {
        self.journal
            .clone()
            .ok_or(PactoleFsStorageError::NotParsed)
    }
}
