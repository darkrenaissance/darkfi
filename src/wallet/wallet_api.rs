use crate::serial::{deserialize, serialize, Decodable, Encodable};
use crate::Result;

use rusqlite::Connection;

use std::path::PathBuf;

pub trait WalletApi {
    fn get_password(&self) -> String;
    fn get_path(&self) -> PathBuf;

    fn get_value_serialized<T: Encodable>(&self, data: &T) -> Result<Vec<u8>> {
        let v = serialize(data);
        Ok(v)
    }

    fn get_value_deserialized<D: Decodable>(&self, key: Vec<u8>) -> Result<D> {
        let v: D = deserialize(&key)?;
        Ok(v)
    }

    fn get_tables_name(&self) -> Result<Vec<String>> {
        let conn = Connection::open(&self.get_path())?;
        conn.pragma_update(None, "key", &self.get_password())?;
        let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='table'")?;
        let table_iter = stmt.query_map::<String, _, _>([], |row| row.get(0))?;

        let mut tables = Vec::new();

        for table in table_iter {
            tables.push(table?);
        }

        Ok(tables)
    }

    fn destroy(&self) -> Result<()> {
        let conn = Connection::open(&self.get_path())?;
        conn.pragma_update(None, "key", &self.get_password())?;

        for table in self.get_tables_name()?.iter() {
            let drop_stmt = format!("DROP TABLE IF EXISTS {}", table);
            let drop_stmt = drop_stmt.as_str();
            conn.execute(drop_stmt, [])?;
        }

        Ok(())
    }
}
