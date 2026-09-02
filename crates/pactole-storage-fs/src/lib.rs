use pactole_core::Transaction;
use std::fs;
use std::path::PathBuf;

pub struct LedgerFileStorage {
    filepath: PathBuf,
}

impl LedgerFileStorage {
    fn parse_string(buffer: String) -> Self {
        todo!()
    }
}

impl From<PathBuf> for LedgerFileStorage {
    fn from(value: PathBuf) -> Self {
        let contents = fs::read_to_string(value).expect("Should have been able to read the file");
        Self::from(contents)
    }
}

impl From<String> for LedgerFileStorage {
    fn from(value: String) -> Self {
        Self::parse_string(value)
    }
}
