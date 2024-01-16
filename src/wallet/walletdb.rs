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

use std::{any::Any, path::PathBuf, sync::Arc};

use log::{debug, info};
use rusqlite::Connection;
use smol::lock::Mutex;

use crate::Result;

pub type WalletPtr = Arc<WalletDb>;

/// Types we want to allow to query from the SQL wallet
pub enum QueryType {
    /// Integer gets decoded into `u64`
    Integer = 0x00,
    /// Blob gets decoded into `Vec<u8>`
    Blob = 0x01,
    /// OptionInteger gets decoded into `Option<u64>`
    OptionInteger = 0x02,
    /// OptionBlob gets decoded into `Option<Vec<u8>>`
    OptionBlob = 0x03,
    /// Text gets decoded into `String`
    Text = 0x04,
    /// Last type, increment this when you add new types.
    Last = 0x05,
}

impl From<u8> for QueryType {
    fn from(x: u8) -> Self {
        match x {
            0x00 => Self::Integer,
            0x01 => Self::Blob,
            0x02 => Self::OptionInteger,
            0x03 => Self::OptionBlob,
            0x04 => Self::Text,
            _ => unimplemented!(),
        }
    }
}

#[derive(Debug)]
pub enum SqlType {
    Integer(i64),
    Text(String),
    Blob(Vec<u8>),
    Null,
}

impl SqlType {
    pub fn inner<T: 'static>(&self) -> Option<&T> {
        match self {
            SqlType::Integer(v) => (v as &dyn Any).downcast_ref::<T>(),
            SqlType::Text(v) => (v as &dyn Any).downcast_ref::<T>(),
            SqlType::Blob(v) => (v as &dyn Any).downcast_ref::<T>(),
            SqlType::Null => None,
        }
    }
}

/// Structure representing base wallet operations.
/// Additional operations can be implemented by trait extensions.
pub struct WalletDb {
    pub conn: Mutex<Connection>,
}

impl WalletDb {
    /// Create a new wallet. If `path` is `None`, create it in memory.
    pub fn new(path: Option<PathBuf>, password: Option<&str>) -> Result<WalletPtr> {
        let conn = match path.clone() {
            Some(p) => Connection::open(p)?,
            None => Connection::open_in_memory()?,
        };

        if let Some(password) = password {
            conn.pragma_update(None, "key", password)?;
        }
        conn.pragma_update(None, "foreign_keys", "ON")?;

        info!(target: "wallet::walletdb", "[WalletDb] Opened Sqlite connection at \"{:?}\"", path);
        Ok(Arc::new(Self { conn: Mutex::new(conn) }))
    }

    /// This function executes a given SQL query, but isn't able to return anything.
    /// Therefore it's best to use it for initializing a table or similar things.
    pub async fn exec_sql(&self, query: &str) -> Result<()> {
        info!(target: "wallet::walletdb", "[WalletDb] Executing SQL query");
        debug!(target: "wallet::walletdb", "[WalletDb] Query:\n{}", query);
        let _ = self.conn.lock().await.execute(query, ())?;
        Ok(())
    }

    pub async fn query_single(
        &self,
        table: &str,
        col_names: Vec<&str>,
        where_queries: Option<Vec<(&str, SqlType)>>,
    ) -> Result<Vec<SqlType>> {
        let mut query = format!("SELECT {} FROM {}", col_names.join(", "), table);

        if let Some(wq) = where_queries.as_ref() {
            let where_str: Vec<String> = wq.iter().map(|(k, _)| format!("{} = ?", k)).collect();
            query.push_str(&format!(" WHERE {}", where_str.join(" AND ")));
        }

        let params: Vec<rusqlite::types::ToSqlOutput> = where_queries.map_or(Vec::new(), |wq| {
            wq.into_iter()
                .map(|(_, v)| match v {
                    SqlType::Integer(i) => rusqlite::types::ToSqlOutput::from(i),
                    SqlType::Text(t) => rusqlite::types::ToSqlOutput::from(t),
                    SqlType::Blob(b) => rusqlite::types::ToSqlOutput::from(b),
                    SqlType::Null => rusqlite::types::ToSqlOutput::from(rusqlite::types::Null),
                })
                .collect::<Vec<_>>()
        });

        let wallet_conn = self.conn.lock().await;
        let mut stmt = wallet_conn.prepare(&query)?;
        let params_as_slice: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|x| x as &dyn rusqlite::ToSql).collect();
        let mut rows = stmt.query(params_as_slice.as_slice())?;

        let row = match rows.next()? {
            Some(row_result) => row_result,
            None => return Ok(vec![]),
        };

        let mut result = vec![];
        for (idx, _) in col_names.iter().enumerate() {
            let value: SqlType = match row.get_ref(idx)?.data_type() {
                rusqlite::types::Type::Integer => SqlType::Integer(row.get(idx)?),
                rusqlite::types::Type::Text => SqlType::Text(row.get(idx)?),
                rusqlite::types::Type::Blob => SqlType::Blob(row.get(idx)?),
                rusqlite::types::Type::Null => SqlType::Null,
                _ => unimplemented!(),
            };

            result.push(value);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mem_wallet() {
        smol::block_on(async {
            let wallet = WalletDb::new(None, Some("foobar")).unwrap();
            wallet.exec_sql("CREATE TABLE mista ( numba INTEGER );").await.unwrap();
            wallet.exec_sql("INSERT INTO mista ( numba ) VALUES ( 42 );").await.unwrap();

            let conn = wallet.conn.lock().await;
            let mut stmt = conn.prepare("SELECT numba FROM mista").unwrap();
            let numba: u64 = stmt.query_row((), |row| Ok(row.get("numba").unwrap())).unwrap();
            stmt.finalize().unwrap();
            assert!(numba == 42);
        });
    }

    #[test]
    fn test_query_single() {
        smol::block_on(async {
            let wallet = WalletDb::new(None, None).unwrap();
            wallet
                .exec_sql("CREATE TABLE mista ( why INTEGER, are TEXT, you INTEGER, gae BLOB );")
                .await
                .unwrap();

            let why = 42;
            let are = "are".to_string();
            let you = 69;
            let gae = vec![42u8; 32];

            let query_str = "INSERT INTO mista ( why, are, you, gae ) VALUES (?1, ?2, ?3, ?4);";

            let wallet_conn = wallet.conn.lock().await;
            let mut stmt = wallet_conn.prepare(query_str).unwrap();
            stmt.execute(rusqlite::params![why, are, you, gae]).unwrap();
            stmt.finalize().unwrap();
            drop(wallet_conn);

            let ret =
                wallet.query_single("mista", vec!["why", "are", "you", "gae"], None).await.unwrap();
            assert!(ret.len() == 4);

            assert!(ret[0].inner::<i64>().unwrap() == &why);
            assert!(ret[1].inner::<String>().unwrap() == &are);
            assert!(ret[2].inner::<i64>().unwrap() == &you);
            assert!(ret[3].inner::<Vec<u8>>().unwrap() == &gae);
        });
    }
}
