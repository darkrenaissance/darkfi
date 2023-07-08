/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use std::path::PathBuf;

use async_std::sync::{Arc, Mutex};
use log::{debug, info};
use rusqlite::Connection;

use crate::Result;

pub type WalletPtr = Arc<WalletDb>;

/// Types we want to allow to query from the SQL wallet
#[repr(u8)]
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

/// Structure representing base wallet operations.
/// Additional operations can be implemented by trait extensions.
pub struct WalletDb {
    pub conn: Mutex<Connection>,
}

impl WalletDb {
    /// Create a new wallet. If `path` is `None`, create it in memory.
    pub async fn new(path: Option<PathBuf>, _password: &str) -> Result<WalletPtr> {
        let conn = match path.clone() {
            Some(p) => Connection::open(p)?,
            None => Connection::open_in_memory()?,
        };

        conn.pragma_update(None, "foreign_keys", "ON")?;

        info!(target: "wallet::walletdb", "[WalletDb] Opened Sqlite connection at \"{:?}\"", path);
        Ok(Arc::new(Self { conn: Mutex::new(conn) }))
    }

    /// This function executes a given SQL query, but isn't able to return anything.
    /// Therefore it's best to use it for initializing a table or similar things.
    pub async fn exec_sql(&self, query: &str) -> Result<()> {
        info!(target: "wallet::walletdb", "[WalletDb] Executing SQL query");
        debug!(target: "wallet::walletdb", "\n{}", query);
        self.conn.lock().await.execute(query, ())?;
        Ok(())
    }
}
