use crate::models::Transaction;

pub trait ReadableStorage {
    fn get_all() -> Vec<Transaction>;
}
