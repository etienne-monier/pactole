use pactole_storage_fs::LedgerFileStorage;

const TEST_STRING: &str = "
# A test
2026-08-04 * Boulanger
    ; A metadata
    Expenses:Food      2
    Assets:Cash
";

fn main() {
    let storage = LedgerFileStorage::from(TEST_STRING.to_string());
    let transactions = storage.get_all();
    dbg!(transactions)
}
