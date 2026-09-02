use pactole_core::{Entry, Journal, ReadableStorage};
use std::fs;
use std::path::PathBuf;

pub struct PactoleFileStorage {
    filepath: Option<PathBuf>,
    journal: Journal,
}

impl PactoleFileStorage {
    fn parse_string(buffer: String) -> Self {
        let journal: Journal =
            toml::from_str(&buffer).expect("Should have been able to parse the journal");
        Self {
            filepath: None,
            journal,
        }
    }
}

impl From<PathBuf> for PactoleFileStorage {
    fn from(value: PathBuf) -> Self {
        let contents = fs::read_to_string(&value).expect("Should have been able to read the file");
        let mut storage = Self::parse_string(contents);
        storage.filepath = Some(value);
        storage
    }
}

impl From<String> for PactoleFileStorage {
    fn from(value: String) -> Self {
        Self::parse_string(value)
    }
}

impl ReadableStorage for PactoleFileStorage {
    fn get_all(&self) -> Vec<Entry> {
        self.journal.entries().to_vec()
    }
}
