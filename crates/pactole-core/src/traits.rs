use crate::models::Entry;
use std::error::Error;

pub trait ReadableStorage {
    type Error: Error;

    fn get_all(&self) -> Result<Vec<Entry>, Self::Error>;
}
