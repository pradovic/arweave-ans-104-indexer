use crate::BundleEntry;
use redb::{Database, TableDefinition};
use std::path::Path;

const TABLE: TableDefinition<i128, String> = TableDefinition::new("entries_table");

pub struct DB {
    db: Database,
    last_index: i128,
}

impl DB {
    pub fn new(name: &Path) -> Result<Self, String> {
        let db = Database::create(name).map_err(|e| format!("Failed to open database: {}", e))?;

        Ok(Self { db, last_index: -1 })
    }

    pub fn add_entry(&self, key: i128, value: BundleEntry) -> Result<(), String> {
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| format!("Failed to begin write transaction: {}", e))?;

        {
            let mut table = write_txn
                .open_table(TABLE)
                .map_err(|e| format!("Failed to open table: {}", e))?;

            let serialized_value = serde_json::to_string(&value)
                .map_err(|e| format!("Failed to serialize value: {}", e))?;

            table
                .insert(key, &serialized_value)
                .map_err(|e| format!("Failed to insert entry: {}", e))?;
        }

        write_txn
            .commit()
            .map_err(|e| format!("Failed to commit write transaction: {}", e))?;

        Ok(())
    }

    pub fn read_entry(&self, key: i128) -> Result<Option<BundleEntry>, String> {
        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| format!("Failed to begin read transaction: {}", e))?;

        let table = read_txn
            .open_table(TABLE)
            .map_err(|e| format!("Failed to open table: {}", e))?;

        let serialized_value = table
            .get(key)
            .map_err(|e| format!("Failed to get entry: {}", e))?;

        let value = match serialized_value {
            Some(serialized_value) => serde_json::from_str(&serialized_value.value())
                .map_err(|e| format!("Failed to deserialize value: {}", e))?,
            None => return Ok(None),
        };

        Ok(Some(value))
    }

    pub fn pop_first_entry(&self) -> Result<Option<BundleEntry>, String> {
        let val;

        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| format!("Failed to begin write transaction: {}", e))?;

        {
            let mut table = write_txn
                .open_table(TABLE)
                .map_err(|e| format!("Failed to open table: {}", e))?;

            val = match table
                .pop_first()
                .map_err(|e| format!("Failed to pop first entry: {}", e))?
            {
                Some((_, serialized_value)) => {
                    let value = serde_json::from_str(&serialized_value.value())
                        .map_err(|e| format!("Failed to deserialize value: {}", e))?;
                    Some(value)
                }
                None => None,
            };
        }

        write_txn
            .commit()
            .map_err(|e| format!("Failed to commit write transaction: {}", e))?;

        Ok(val)
    }

    pub fn push_last_entry(&mut self, value: BundleEntry) -> Result<(), String> {
        self.last_index += 1;
        self.add_entry(self.last_index, value)
    }
}
