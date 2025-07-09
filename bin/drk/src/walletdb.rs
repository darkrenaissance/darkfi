/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use rusqlite::{
    types::{ToSql, Value},
    Connection,
};
use tracing::{debug, error};

use crate::error::{WalletDbError, WalletDbResult};

pub type WalletPtr = Arc<WalletDb>;

/// Structure representing base wallet database operations.
pub struct WalletDb {
    /// Connection to the SQLite database.
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
    pub fn exec_batch_sql(&self, query: &str) -> WalletDbResult<()> {
        debug!(target: "walletdb::exec_batch_sql", "[WalletDb] Executing batch SQL query:\n{query}");
        let Ok(conn) = self.conn.lock() else { return Err(WalletDbError::FailedToAquireLock) };
        if let Err(e) = conn.execute_batch(query) {
            error!(target: "walletdb::exec_batch_sql", "[WalletDb] Query failed: {e}");
            return Err(WalletDbError::QueryExecutionFailed)
        };

        Ok(())
    }

    /// This function executes a given SQL query, but isn't able to return anything.
    /// Therefore it's best to use it for initializing a table or similar things.
    pub fn exec_sql(&self, query: &str, params: &[&dyn ToSql]) -> WalletDbResult<()> {
        debug!(target: "walletdb::exec_sql", "[WalletDb] Executing SQL query:\n{query}");
        let Ok(conn) = self.conn.lock() else { return Err(WalletDbError::FailedToAquireLock) };

        // If no params are provided, execute directly
        if params.is_empty() {
            if let Err(e) = conn.execute(query, ()) {
                error!(target: "walletdb::exec_sql", "[WalletDb] Query failed: {e}");
                return Err(WalletDbError::QueryExecutionFailed)
            };
            return Ok(())
        }

        // First we prepare the query
        let Ok(mut stmt) = conn.prepare(query) else {
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

    /// Generate a new statement for provided query and bind the provided params,
    /// returning the raw SQL query as a string.
    pub fn create_prepared_statement(
        &self,
        query: &str,
        params: &[&dyn ToSql],
    ) -> WalletDbResult<String> {
        debug!(target: "walletdb::create_prepared_statement", "[WalletDb] Preparing statement for SQL query:\n{query}");
        let Ok(conn) = self.conn.lock() else { return Err(WalletDbError::FailedToAquireLock) };

        // First we prepare the query
        let Ok(mut stmt) = conn.prepare(query) else {
            return Err(WalletDbError::QueryPreparationFailed)
        };

        // Bind all provided params
        for (index, param) in params.iter().enumerate() {
            if stmt.raw_bind_parameter(index + 1, param).is_err() {
                return Err(WalletDbError::QueryPreparationFailed)
            };
        }

        // Grab the raw SQL
        let query = stmt.expanded_sql().unwrap();

        // Drop statement and the connection lock
        drop(stmt);
        drop(conn);

        Ok(query)
    }

    /// Generate a `SELECT` query for provided table from selected column names and
    /// provided `WHERE` clauses. Named parameters are supported in the `WHERE` clauses,
    /// assuming they follow the normal formatting ":{column_name}".
    fn generate_select_query(
        &self,
        table: &str,
        col_names: &[&str],
        params: &[(&str, &dyn ToSql)],
    ) -> String {
        let mut query = if col_names.is_empty() {
            format!("SELECT * FROM {table}")
        } else {
            format!("SELECT {} FROM {table}", col_names.join(", "))
        };
        if params.is_empty() {
            return query
        }

        let mut where_str = Vec::with_capacity(params.len());
        for (k, _) in params {
            let col = &k[1..];
            where_str.push(format!("{col} = {k}"));
        }
        query.push_str(&format!(" WHERE {}", where_str.join(" AND ")));

        query
    }

    /// Query provided table from selected column names and provided `WHERE` clauses,
    /// for a single row.
    pub fn query_single(
        &self,
        table: &str,
        col_names: &[&str],
        params: &[(&str, &dyn ToSql)],
    ) -> WalletDbResult<Vec<Value>> {
        // Generate `SELECT` query
        let query = self.generate_select_query(table, col_names, params);
        debug!(target: "walletdb::query_single", "[WalletDb] Executing SQL query:\n{query}");

        // First we prepare the query
        let Ok(conn) = self.conn.lock() else { return Err(WalletDbError::FailedToAquireLock) };

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
        if col_names.is_empty() {
            let mut idx = 0;
            loop {
                let Ok(value) = row.get(idx) else { break };
                result.push(value);
                idx += 1;
            }
        } else {
            for col in col_names {
                let Ok(value) = row.get(*col) else {
                    return Err(WalletDbError::ParseColumnValueError)
                };
                result.push(value);
            }
        }

        Ok(result)
    }

    /// Query provided table from selected column names and provided `WHERE` clauses,
    /// for multiple rows.
    pub fn query_multiple(
        &self,
        table: &str,
        col_names: &[&str],
        params: &[(&str, &dyn ToSql)],
    ) -> WalletDbResult<Vec<Vec<Value>>> {
        // Generate `SELECT` query
        let query = self.generate_select_query(table, col_names, params);
        debug!(target: "walletdb::query_multiple", "[WalletDb] Executing SQL query:\n{query}");

        // First we prepare the query
        let Ok(conn) = self.conn.lock() else { return Err(WalletDbError::FailedToAquireLock) };
        let Ok(mut stmt) = conn.prepare(&query) else {
            return Err(WalletDbError::QueryPreparationFailed)
        };

        // Execute the query using provided converted params
        let Ok(mut rows) = stmt.query(params) else {
            return Err(WalletDbError::QueryExecutionFailed)
        };

        // Loop over returned rows and parse them
        let mut result = vec![];
        loop {
            // Check if an error occured
            let row = match rows.next() {
                Ok(r) => r,
                Err(_) => return Err(WalletDbError::QueryExecutionFailed),
            };

            // Check if no row was returned
            let row = match row {
                Some(r) => r,
                None => break,
            };

            // Grab row returned values
            let mut row_values = vec![];
            if col_names.is_empty() {
                let mut idx = 0;
                loop {
                    let Ok(value) = row.get(idx) else { break };
                    row_values.push(value);
                    idx += 1;
                }
            } else {
                for col in col_names {
                    let Ok(value) = row.get(*col) else {
                        return Err(WalletDbError::ParseColumnValueError)
                    };
                    row_values.push(value);
                }
            }
            result.push(row_values);
        }

        Ok(result)
    }

    /// Query provided table using provided query for multiple rows.
    pub fn query_custom(
        &self,
        query: &str,
        params: &[&dyn ToSql],
    ) -> WalletDbResult<Vec<Vec<Value>>> {
        debug!(target: "walletdb::query_custom", "[WalletDb] Executing SQL query:\n{query}");

        // First we prepare the query
        let Ok(conn) = self.conn.lock() else { return Err(WalletDbError::FailedToAquireLock) };
        let Ok(mut stmt) = conn.prepare(query) else {
            return Err(WalletDbError::QueryPreparationFailed)
        };

        // Execute the query using provided converted params
        let Ok(mut rows) = stmt.query(params) else {
            return Err(WalletDbError::QueryExecutionFailed)
        };

        // Loop over returned rows and parse them
        let mut result = vec![];
        loop {
            // Check if an error occured
            let row = match rows.next() {
                Ok(r) => r,
                Err(_) => return Err(WalletDbError::QueryExecutionFailed),
            };

            // Check if no row was returned
            let row = match row {
                Some(r) => r,
                None => break,
            };

            // Grab row returned values
            let mut row_values = vec![];
            let mut idx = 0;
            loop {
                let Ok(value) = row.get(idx) else { break };
                row_values.push(value);
                idx += 1;
            }
            result.push(row_values);
        }

        Ok(result)
    }
}

/// Custom implementation of rusqlite::named_params! to use `expr` instead of `literal` as `$param_name`,
/// and append the ":" named parameters prefix.
#[macro_export]
macro_rules! convert_named_params {
    () => {
        &[] as &[(&str, &dyn rusqlite::types::ToSql)]
    };
    ($(($param_name:expr, $param_val:expr)),+ $(,)?) => {
        &[$((format!(":{}", $param_name).as_str(), &$param_val as &dyn rusqlite::types::ToSql)),+] as &[(&str, &dyn rusqlite::types::ToSql)]
    };
}

#[cfg(test)]
mod tests {
    use rusqlite::types::Value;

    use crate::walletdb::WalletDb;

    #[test]
    fn test_mem_wallet() {
        let wallet = WalletDb::new(None, Some("foobar")).unwrap();
        wallet
            .exec_batch_sql(
                "CREATE TABLE mista ( numba INTEGER ); INSERT INTO mista ( numba ) VALUES ( 42 );",
            )
            .unwrap();

        let ret = wallet.query_single("mista", &["numba"], &[]).unwrap();
        assert_eq!(ret.len(), 1);
        let numba: i64 = if let Value::Integer(numba) = ret[0] { numba } else { -1 };
        assert_eq!(numba, 42);

        let ret = wallet.query_custom("SELECT numba FROM mista;", &[]).unwrap();
        assert_eq!(ret.len(), 1);
        assert_eq!(ret[0].len(), 1);
        let numba: i64 = if let Value::Integer(numba) = ret[0][0] { numba } else { -1 };
        assert_eq!(numba, 42);
    }

    #[test]
    fn test_query_single() {
        let wallet = WalletDb::new(None, None).unwrap();
        wallet
            .exec_batch_sql("CREATE TABLE mista ( why INTEGER, are TEXT, you INTEGER, gae BLOB );")
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
            .unwrap();

        let ret = wallet.query_single("mista", &["why", "are", "you", "gae"], &[]).unwrap();
        assert_eq!(ret.len(), 4);
        assert_eq!(ret[0], Value::Integer(why));
        assert_eq!(ret[1], Value::Text(are.clone()));
        assert_eq!(ret[2], Value::Integer(you));
        assert_eq!(ret[3], Value::Blob(gae.clone()));
        let ret = wallet.query_custom("SELECT why, are, you, gae FROM mista;", &[]).unwrap();
        assert_eq!(ret.len(), 1);
        assert_eq!(ret[0].len(), 4);
        assert_eq!(ret[0][0], Value::Integer(why));
        assert_eq!(ret[0][1], Value::Text(are.clone()));
        assert_eq!(ret[0][2], Value::Integer(you));
        assert_eq!(ret[0][3], Value::Blob(gae.clone()));

        let ret = wallet
            .query_single(
                "mista",
                &["gae"],
                rusqlite::named_params! {":why": why, ":are": are, ":you": you},
            )
            .unwrap();
        assert_eq!(ret.len(), 1);
        assert_eq!(ret[0], Value::Blob(gae.clone()));
        let ret = wallet
            .query_custom(
                "SELECT gae FROM mista WHERE why = ?1 AND are = ?2 AND you = ?3;",
                rusqlite::params![why, are, you],
            )
            .unwrap();
        assert_eq!(ret.len(), 1);
        assert_eq!(ret[0].len(), 1);
        assert_eq!(ret[0][0], Value::Blob(gae));
    }

    #[test]
    fn test_query_multi() {
        let wallet = WalletDb::new(None, None).unwrap();
        wallet
            .exec_batch_sql("CREATE TABLE mista ( why INTEGER, are TEXT, you INTEGER, gae BLOB );")
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
            .unwrap();
        wallet
            .exec_sql(
                "INSERT INTO mista ( why, are, you, gae ) VALUES (?1, ?2, ?3, ?4);",
                rusqlite::params![why, are, you, gae],
            )
            .unwrap();

        let ret = wallet.query_multiple("mista", &[], &[]).unwrap();
        assert_eq!(ret.len(), 2);
        for row in ret {
            assert_eq!(row.len(), 4);
            assert_eq!(row[0], Value::Integer(why));
            assert_eq!(row[1], Value::Text(are.clone()));
            assert_eq!(row[2], Value::Integer(you));
            assert_eq!(row[3], Value::Blob(gae.clone()));
        }
        let ret = wallet.query_custom("SELECT * FROM mista;", &[]).unwrap();
        assert_eq!(ret.len(), 2);
        for row in ret {
            assert_eq!(row.len(), 4);
            assert_eq!(row[0], Value::Integer(why));
            assert_eq!(row[1], Value::Text(are.clone()));
            assert_eq!(row[2], Value::Integer(you));
            assert_eq!(row[3], Value::Blob(gae.clone()));
        }

        let ret = wallet
            .query_multiple(
                "mista",
                &["gae"],
                convert_named_params! {("why", why), ("are", are), ("you", you)},
            )
            .unwrap();
        assert_eq!(ret.len(), 2);
        for row in ret {
            assert_eq!(row.len(), 1);
            assert_eq!(row[0], Value::Blob(gae.clone()));
        }
        let ret = wallet
            .query_custom(
                "SELECT gae FROM mista WHERE why = ?1 AND are = ?2 AND you = ?3;",
                rusqlite::params![why, are, you],
            )
            .unwrap();
        assert_eq!(ret.len(), 2);
        for row in ret {
            assert_eq!(row.len(), 1);
            assert_eq!(row[0], Value::Blob(gae.clone()));
        }
    }
}
