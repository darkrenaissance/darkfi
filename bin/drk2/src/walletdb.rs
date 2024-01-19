/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{path::PathBuf, sync::Arc};

use log::{debug, error};
use rusqlite::{
    types::{ToSql, Value},
    Connection,
};
use smol::lock::Mutex;

use crate::error::{WalletDbError, WalletDbResult};

pub type WalletPtr = Arc<WalletDb>;

/// Structure representing base wallet database operations.
pub struct WalletDb {
    /// Connection to the SQLite database
    pub conn: Mutex<Connection>,
}

impl WalletDb {
    /// Create a new wallet database handler. If `path` is `None`, create it in memory.
    pub fn new(path: Option<PathBuf>, password: Option<&str>) -> WalletDbResult<WalletPtr> {
        let Ok(conn) = (match path.clone() {
            Some(p) => Connection::open(p),
            None => Connection::open_in_memory(),
        }) else {
            return Err(WalletDbError::ConnectionFailed);
        };

        if let Some(password) = password {
            if let Err(e) = conn.pragma_update(None, "key", password) {
                error!(target: "walletdb::new", "[WalletDb] Pragma update failed: {e}");
                return Err(WalletDbError::PragmaUpdateError);
            };
        }
        if let Err(e) = conn.pragma_update(None, "foreign_keys", "ON") {
            error!(target: "walletdb::new", "[WalletDb] Pragma update failed: {e}");
            return Err(WalletDbError::PragmaUpdateError);
        };

        debug!(target: "walletdb::new", "[WalletDb] Opened Sqlite connection at \"{path:?}\"");
        Ok(Arc::new(Self { conn: Mutex::new(conn) }))
    }

    /// This function executes a given SQL query that contains multiple SQL statements,
    /// that don't contain any parameters.
    pub async fn exec_batch_sql(&self, query: &str) -> WalletDbResult<()> {
        debug!(target: "walletdb::exec_batch_sql", "[WalletDb] Executing batch SQL query:\n{query}");
        // If no params are provided, execute directly
        if let Err(e) = self.conn.lock().await.execute_batch(query) {
            error!(target: "walletdb::exec_batch_sql", "[WalletDb] Query failed: {e}");
            return Err(WalletDbError::QueryExecutionFailed)
        };

        Ok(())
    }

    /// This function executes a given SQL query, but isn't able to return anything.
    /// Therefore it's best to use it for initializing a table or similar things.
    pub async fn exec_sql(&self, query: &str, params: &[&dyn ToSql]) -> WalletDbResult<()> {
        debug!(target: "walletdb::exec_sql", "[WalletDb] Executing SQL query:\n{query}");
        // If no params are provided, execute directly
        if params.is_empty() {
            if let Err(e) = self.conn.lock().await.execute(query, ()) {
                error!(target: "walletdb::exec_sql", "[WalletDb] Query failed: {e}");
                return Err(WalletDbError::QueryExecutionFailed)
            };
            return Ok(())
        }

        // First we prepare the query
        let conn = self.conn.lock().await;
        let Ok(mut stmt) = conn.prepare(query) else {
            eprintln!("Error: {:?}", conn.prepare(query));
            return Err(WalletDbError::QueryPreparationFailed)
        };

        // Execute the query using provided params
        if let Err(e) = stmt.execute(params) {
            error!(target: "walletdb::exec_sql", "[WalletDb] Query failed: {e}");
            return Err(WalletDbError::QueryExecutionFailed)
        };

        // Finalize query and drop connection lock
        if let Err(e) = stmt.finalize() {
            error!(target: "walletdb::exec_sql", "[WalletDb] Query finalization failed: {e}");
            return Err(WalletDbError::QueryFinalizationFailed)
        };
        drop(conn);

        Ok(())
    }

    /// Query provided table from selected column names and provided `WHERE` clauses.
    /// Named parameters are supported in the `WHERE` clauses, assuming they follow the
    /// normal formatting ":{column_name}"
    pub async fn query_single(
        &self,
        table: &str,
        col_names: Vec<&str>,
        params: &[(&str, &dyn ToSql)],
    ) -> WalletDbResult<Vec<Value>> {
        // Generate `SELECT` query
        let mut query = format!("SELECT {} FROM {}", col_names.join(", "), table);
        if !params.is_empty() {
            let mut where_str = Vec::with_capacity(params.len());
            for (k, _) in params {
                let col = &k[1..];
                where_str.push(format!("{col} = {k}"));
            }
            query.push_str(&format!(" WHERE {}", where_str.join(" AND ")));
        };
        debug!(target: "walletdb::query_single", "[WalletDb] Executing SQL query:\n{query}");

        // First we prepare the query
        let conn = self.conn.lock().await;
        let Ok(mut stmt) = conn.prepare(&query) else {
            return Err(WalletDbError::QueryPreparationFailed)
        };

        // Execute the query using provided params
        let Ok(mut rows) = stmt.query(params) else {
            return Err(WalletDbError::QueryExecutionFailed)
        };

        // Check if row exists
        let Ok(next) = rows.next() else { return Err(WalletDbError::QueryExecutionFailed) };
        let row = match next {
            Some(row_result) => row_result,
            None => return Err(WalletDbError::RowNotFound),
        };

        // Grab returned values
        let mut result = vec![];
        for col in col_names {
            let Ok(value) = row.get(col) else { return Err(WalletDbError::ParseColumnValueError) };
            result.push(value);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::types::Value;

    use crate::walletdb::WalletDb;

    #[test]
    fn test_mem_wallet() {
        smol::block_on(async {
            let wallet = WalletDb::new(None, Some("foobar")).unwrap();
            wallet.exec_sql("CREATE TABLE mista ( numba INTEGER );", &[]).await.unwrap();
            wallet.exec_sql("INSERT INTO mista ( numba ) VALUES ( 42 );", &[]).await.unwrap();

            let ret = wallet.query_single("mista", vec!["numba"], &[]).await.unwrap();
            assert_eq!(ret.len(), 1);
            let numba: i64 = if let Value::Integer(numba) = ret[0] { numba } else { -1 };
            assert_eq!(numba, 42);
        });
    }

    #[test]
    fn test_query_single() {
        smol::block_on(async {
            let wallet = WalletDb::new(None, None).unwrap();
            wallet
                .exec_sql(
                    "CREATE TABLE mista ( why INTEGER, are TEXT, you INTEGER, gae BLOB );",
                    &[],
                )
                .await
                .unwrap();

            let why = 42;
            let are = "are".to_string();
            let you = 69;
            let gae = vec![42u8; 32];

            wallet
                .exec_sql(
                    "INSERT INTO mista ( why, are, you, gae ) VALUES (?1, ?2, ?3, ?4);",
                    rusqlite::params![why, are, you, gae],
                )
                .await
                .unwrap();

            let ret =
                wallet.query_single("mista", vec!["why", "are", "you", "gae"], &[]).await.unwrap();
            assert_eq!(ret.len(), 4);
            assert_eq!(ret[0], Value::Integer(why));
            assert_eq!(ret[1], Value::Text(are.clone()));
            assert_eq!(ret[2], Value::Integer(you));
            assert_eq!(ret[3], Value::Blob(gae.clone()));

            let ret = wallet
                .query_single(
                    "mista",
                    vec!["gae"],
                    rusqlite::named_params! {":why" : why, ":are" : are, ":you" : you},
                )
                .await
                .unwrap();
            assert_eq!(ret.len(), 1);
            assert_eq!(ret[0], Value::Blob(gae));
        });
    }
}
