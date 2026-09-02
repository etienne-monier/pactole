use pactole_core::ReadableStorage;
use pactole_storage_fs::PactoleFileStorage;
use std::env;
use std::path::PathBuf;

fn main() {
    let path = env::args()
        .nth(1)
        .unwrap_or_else(|| "test.toml".to_string());

    let storage = PactoleFileStorage::from(PathBuf::from(path));
    let entries = storage.get_all();

    for entry in &entries {
        println!("{entry:#?}");
    }
}
