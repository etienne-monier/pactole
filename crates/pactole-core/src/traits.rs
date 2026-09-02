use crate::models::Entry;

pub trait ReadableStorage {
    fn get_all(&self) -> Vec<Entry>;
}
