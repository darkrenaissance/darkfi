/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::{convert::From, path::PathBuf, sync::Arc};

use smol::lock::Mutex as AsyncMutex;
use tracing::{debug, error};
pub use turso::{Builder, Connection, EncryptionOpts, Value};

use crate::error::{WalletDbError, WalletDbResult};

pub type WalletPtr = Arc<WalletDb>;

const ENCRYPTION_ALGO: &str = "aegis256";

/// Structure representing base wallet database operations.
pub struct WalletDb {
    /// Connection to the turso database.
    pub conn: AsyncMutex<Connection>,
}

impl WalletDb {
    /// Create a new wallet database handler. If `path` is `None`, create it in memory.
    pub async fn new(path: Option<PathBuf>, password: Option<&str>) -> WalletDbResult<WalletPtr> {
        // Parse database path
        let path = match path {
            Some(p) => {
                let Some(p) = p.to_str() else {
                    return Err(WalletDbError::ConnectionFailed);
                };
                String::from(p)
            }
            None => String::from(":memory:"),
        };

        // Set encryption. We have to manually devire the key since
        // turso doesn't support it yet.
        let builder = match password {
            Some(password) => {
                let opts = EncryptionOpts {
                    cipher: String::from(ENCRYPTION_ALGO),
                    hexkey: blake3::hash(password.as_bytes()).to_hex().to_string(),
                };
                Builder::new_local(&path).experimental_encryption(true).with_encryption(opts)
            }
            None => Builder::new_local(&path),
        };

        // Initialize connection builder
        let Ok(builder) = builder.build().await else {
            return Err(WalletDbError::ConnectionFailed);
        };

        // Connect to database
        let Ok(conn) = builder.connect() else {
            return Err(WalletDbError::ConnectionFailed);
        };

        // Set foreign keys pragma
        if let Err(e) = conn.pragma_update("foreign_keys", "ON").await {
            error!(target: "walletdb::new", "[WalletDb] Foreign keys pragma update failed: {e}");
            return Err(WalletDbError::PragmaUpdateError);
        };

        debug!(target: "walletdb::new", "[WalletDb] Opened Sqlite connection at \"{path:?}\"");
        Ok(Arc::new(Self { conn: AsyncMutex::new(conn) }))
    }

    /// This function executes a given SQL query that contains multiple SQL statements,
    /// that don't contain any parameters.
    pub async fn exec_batch_sql(&self, query: &str) -> WalletDbResult<()> {
        debug!(target: "walletdb::exec_batch_sql", "[WalletDb] Executing batch SQL query:\n{query}");
        if let Err(e) = self.conn.lock().await.execute_batch(query).await {
            error!(target: "walletdb::exec_batch_sql", "[WalletDb] Query failed: {e}");
            return Err(WalletDbError::QueryExecutionFailed)
        };

        Ok(())
    }

    /// This function executes a given SQL query, but isn't able to return anything.
    /// Therefore it's best to use it for initializing a table or similar things.
    pub async fn exec_sql(&self, query: &str, params: Vec<Value>) -> WalletDbResult<()> {
        debug!(target: "walletdb::exec_sql", "[WalletDb] Executing SQL query:\n{query}");
        let conn = self.conn.lock().await;

        // If no params are provided, execute directly
        if params.is_empty() {
            if let Err(e) = conn.execute(query, ()).await {
                error!(target: "walletdb::exec_sql", "[WalletDb] Query failed: {e}");
                return Err(WalletDbError::QueryExecutionFailed)
            };
            return Ok(())
        }

        // First we prepare the query
        let Ok(mut stmt) = conn.prepare(query).await else {
            return Err(WalletDbError::QueryPreparationFailed)
        };

        // Execute the query using provided params
        if let Err(e) = stmt.execute(params).await {
            error!(target: "walletdb::exec_sql", "[WalletDb] Query failed: {e}");
            return Err(WalletDbError::QueryExecutionFailed)
        };
        drop(conn);

        Ok(())
    }

    /// Generate a `SELECT` query for provided table from selected column names and
    /// provided `WHERE` clauses. Named parameters are supported in the `WHERE` clauses,
    /// assuming they follow the normal formatting ":{column_name}".
    fn generate_select_query(
        &self,
        table: &str,
        col_names: &[&str],
        params: &[(String, Value)],
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
    pub async fn query_single(
        &self,
        table: &str,
        col_names: &[&str],
        params: Vec<(String, Value)>,
    ) -> WalletDbResult<Vec<Value>> {
        // Generate `SELECT` query
        let query = self.generate_select_query(table, col_names, &params);
        debug!(target: "walletdb::query_single", "[WalletDb] Executing SQL query:\n{query}");

        // First we prepare the query
        let conn = self.conn.lock().await;

        let Ok(mut stmt) = conn.prepare(&query).await else {
            return Err(WalletDbError::QueryPreparationFailed)
        };

        // Execute the query using provided params
        let Ok(mut rows) = stmt.query(params).await else {
            return Err(WalletDbError::QueryExecutionFailed)
        };

        // Check if row exists
        let Ok(next) = rows.next().await else { return Err(WalletDbError::QueryExecutionFailed) };
        let row = match next {
            Some(row_result) => row_result,
            None => return Err(WalletDbError::RowNotFound),
        };

        // Grab returned values
        let mut result = vec![];
        if col_names.is_empty() {
            let mut idx = 0;
            loop {
                let Ok(value) = row.get_value(idx) else { break };
                result.push(value);
                idx += 1;
            }
        } else {
            for col in col_names {
                let Ok(idx) = rows.column_index(col) else {
                    return Err(WalletDbError::ParseColumnValueError)
                };
                let Ok(value) = row.get_value(idx) else {
                    return Err(WalletDbError::ParseColumnValueError)
                };
                result.push(value);
            }
        }

        Ok(result)
    }

    /// Query provided table from selected column names and provided `WHERE` clauses,
    /// for multiple rows.
    pub async fn query_multiple(
        &self,
        table: &str,
        col_names: &[&str],
        params: Vec<(String, Value)>,
    ) -> WalletDbResult<Vec<Vec<Value>>> {
        // Generate `SELECT` query
        let query = self.generate_select_query(table, col_names, &params);
        debug!(target: "walletdb::query_multiple", "[WalletDb] Executing SQL query:\n{query}");

        // First we prepare the query
        let conn = self.conn.lock().await;
        let Ok(mut stmt) = conn.prepare(&query).await else {
            return Err(WalletDbError::QueryPreparationFailed)
        };

        // Execute the query using provided converted params
        let Ok(mut rows) = stmt.query(params).await else {
            return Err(WalletDbError::QueryExecutionFailed)
        };

        // Loop over returned rows and parse them
        let mut result = vec![];
        loop {
            // Check if an error occured
            let row = match rows.next().await {
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
                    let Ok(value) = row.get_value(idx) else { break };
                    row_values.push(value);
                    idx += 1;
                }
            } else {
                for col in col_names {
                    let Ok(idx) = rows.column_index(col) else {
                        return Err(WalletDbError::ParseColumnValueError)
                    };
                    let Ok(value) = row.get_value(idx) else {
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
    pub async fn query_custom(
        &self,
        query: &str,
        params: Vec<Value>,
    ) -> WalletDbResult<Vec<Vec<Value>>> {
        debug!(target: "walletdb::query_custom", "[WalletDb] Executing SQL query:\n{query}");

        // First we prepare the query
        let conn = self.conn.lock().await;
        let Ok(mut stmt) = conn.prepare(query).await else {
            return Err(WalletDbError::QueryPreparationFailed)
        };

        // Execute the query using provided converted params
        let Ok(mut rows) = stmt.query(params).await else {
            return Err(WalletDbError::QueryExecutionFailed)
        };

        // Loop over returned rows and parse them
        let mut result = vec![];
        loop {
            // Check if an error occured
            let row = match rows.next().await {
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
                let Ok(value) = row.get_value(idx) else { break };
                row_values.push(value);
                idx += 1;
            }
            result.push(row_values);
        }

        Ok(result)
    }
}

/// Custom implementation of `turso::params!` to construct positional
/// params from a heterogeneous set of params types as a vec.
#[macro_export]
macro_rules! params {
    () => {
       ()
    };
    ($($value:expr),* $(,)?) => {
        [$(turso::Value::from($value)),*].to_vec()

    };
}

/// Custom implementation of `turso::named_params!` to construct named
/// params from a heterogeneous set of params types as a vec.
#[macro_export]
macro_rules! named_params {
    () => {
        ()
    };
    ($($param_name:literal: $value:expr),* $(,)?) => {
        [$((String::from($param_name), turso::Value::from($value))),*].to_vec()
    };
}

/// Custom implementation of `turso::named_params!` to use `expr`
/// instead of `literal` as `$param_name`, and append the ":" named
/// parameters prefix.
#[macro_export]
macro_rules! convert_named_params {
    () => {
        ()
    };
    ($(($param_name:expr, $value:expr)),* $(,)?) => {
        [$((format!(":{}", $param_name), turso::Value::from($value))),*].to_vec()
    };
}

#[cfg(test)]
mod tests {
    use crate::walletdb::{Value, WalletDb};

    #[test]
    fn test_mem_wallet() {
        smol::block_on(async {
            let wallet = WalletDb::new(None, Some("foobar")).await.unwrap();
            wallet
                .exec_batch_sql(
                    "CREATE TABLE mista ( numba INTEGER ); INSERT INTO mista ( numba ) VALUES ( 42 );",
                ).await
                .unwrap();

            let ret = wallet.query_single("mista", &["numba"], vec![]).await.unwrap();
            assert_eq!(ret.len(), 1);
            let numba: i64 = if let Value::Integer(numba) = ret[0] { numba } else { -1 };
            assert_eq!(numba, 42);

            let ret = wallet.query_custom("SELECT numba FROM mista;", vec![]).await.unwrap();
            assert_eq!(ret.len(), 1);
            assert_eq!(ret[0].len(), 1);
            let numba: i64 = if let Value::Integer(numba) = ret[0][0] { numba } else { -1 };
            assert_eq!(numba, 42);
        })
    }

    #[test]
    fn test_query_single() {
        smol::block_on(async {
            let wallet = WalletDb::new(None, None).await.unwrap();
            wallet
                .exec_batch_sql(
                    "CREATE TABLE mista ( why INTEGER, are TEXT, you INTEGER, gae BLOB );",
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
                    params![why, are.clone(), you, gae.clone()],
                )
                .await
                .unwrap();

            let ret =
                wallet.query_single("mista", &["why", "are", "you", "gae"], vec![]).await.unwrap();
            assert_eq!(ret.len(), 4);
            assert_eq!(ret[0], Value::Integer(why));
            assert_eq!(ret[1], Value::Text(are.clone()));
            assert_eq!(ret[2], Value::Integer(you));
            assert_eq!(ret[3], Value::Blob(gae.clone()));
            let ret =
                wallet.query_custom("SELECT why, are, you, gae FROM mista;", vec![]).await.unwrap();
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
                    named_params! {":why": why, ":are": are.clone(), ":you": you},
                )
                .await
                .unwrap();
            assert_eq!(ret.len(), 1);
            assert_eq!(ret[0], Value::Blob(gae.clone()));
            let ret = wallet
                .query_custom(
                    "SELECT gae FROM mista WHERE why = ?1 AND are = ?2 AND you = ?3;",
                    params![why, are, you],
                )
                .await
                .unwrap();
            assert_eq!(ret.len(), 1);
            assert_eq!(ret[0].len(), 1);
            assert_eq!(ret[0][0], Value::Blob(gae));
        })
    }

    #[test]
    fn test_query_multi() {
        smol::block_on(async {
            let wallet = WalletDb::new(None, None).await.unwrap();
            wallet
                .exec_batch_sql(
                    "CREATE TABLE mista ( why INTEGER, are TEXT, you INTEGER, gae BLOB );",
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
                    params![why, are.clone(), you, gae.clone()],
                )
                .await
                .unwrap();
            wallet
                .exec_sql(
                    "INSERT INTO mista ( why, are, you, gae ) VALUES (?1, ?2, ?3, ?4);",
                    params![why, are.clone(), you, gae.clone()],
                )
                .await
                .unwrap();

            let ret = wallet.query_multiple("mista", &[], vec![]).await.unwrap();
            assert_eq!(ret.len(), 2);
            for row in ret {
                assert_eq!(row.len(), 4);
                assert_eq!(row[0], Value::Integer(why));
                assert_eq!(row[1], Value::Text(are.clone()));
                assert_eq!(row[2], Value::Integer(you));
                assert_eq!(row[3], Value::Blob(gae.clone()));
            }
            let ret = wallet.query_custom("SELECT * FROM mista;", vec![]).await.unwrap();
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
                    convert_named_params! {("why", why), ("are", are.clone()), ("you", you)},
                )
                .await
                .unwrap();
            assert_eq!(ret.len(), 2);
            for row in ret {
                assert_eq!(row.len(), 1);
                assert_eq!(row[0], Value::Blob(gae.clone()));
            }
            let ret = wallet
                .query_custom(
                    "SELECT gae FROM mista WHERE why = ?1 AND are = ?2 AND you = ?3;",
                    params![why, are, you],
                )
                .await
                .unwrap();
            assert_eq!(ret.len(), 2);
            for row in ret {
                assert_eq!(row.len(), 1);
                assert_eq!(row[0], Value::Blob(gae.clone()));
            }
        })
    }
}
