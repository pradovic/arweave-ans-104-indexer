use arweave_ans_1040_indexer::db::DB;
use rand::Rng;

use std::path::Path;

pub struct DropDb {
    name: String,
}
impl Drop for DropDb {
    fn drop(&mut self) {
        std::fs::remove_file(&self.name).expect("failed to remove test database");
    }
}

pub fn create_test_db() -> (DropDb, DB) {
    let name = format!("test-{}.redb", rand::thread_rng().gen::<u32>());
    let db = DB::new(Path::new(&name)).expect("failed to create test database");
    (DropDb { name }, db)
}
