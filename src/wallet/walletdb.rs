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

use std::{path::Path, str::FromStr, time::Duration};

use async_std::{fs::create_dir_all, sync::Arc};
use log::{debug, error, info, LevelFilter};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
    ConnectOptions, SqlitePool,
};

use crate::{util::path::expand_path, Error, Result};

pub type WalletPtr = Arc<WalletDb>;

/// Helper function to initialize `WalletPtr`
pub async fn init_wallet(wallet_path: &str, wallet_pass: &str) -> Result<WalletPtr> {
    let expanded = expand_path(wallet_path)?;
    let wallet_path = format!("sqlite://{}", expanded.to_str().unwrap());
    let wallet = WalletDb::new(&wallet_path, wallet_pass).await?;
    Ok(wallet)
}

/// Types we want to allow to query from the SQL wallet
#[repr(u8)]
pub enum QueryType {
    /// Integer gets decoded into u64
    Integer = 0x00,
    /// Blob gets decoded into Vec<u8>
    Blob = 0x01,
    /// OptionInteger gets decoded into Option<u64>
    OptionInteger = 0x02,
    /// OptionBlob gets decoded into Option<Vec<u8>>
    OptionBlob = 0x03,
    /// Last type, increment this when you add new types.
    Last = 0x04,
}

impl From<u8> for QueryType {
    fn from(x: u8) -> Self {
        match x {
            0x00 => Self::Integer,
            0x01 => Self::Blob,
            0x02 => Self::OptionInteger,
            0x03 => Self::OptionBlob,
            _ => unimplemented!(),
        }
    }
}

/// Structure representing base wallet operations.
/// Additional operations can be implemented by trait extensions.
pub struct WalletDb {
    pub conn: SqlitePool,
}

impl WalletDb {
    pub async fn new(path: &str, password: &str) -> Result<WalletPtr> {
        if password.trim().is_empty() {
            error!(target: "wallet::walletdb", "Wallet password is empty. You must set a password to use the wallet.");
            return Err(Error::WalletEmptyPassword)
        }

        if path != "sqlite::memory:" {
            let p = Path::new(path.strip_prefix("sqlite://").unwrap());
            if let Some(dirname) = p.parent() {
                info!(target: "wallet::walletdb", "Creating path to wallet database: {}", dirname.display());
                create_dir_all(&dirname).await?;
            }
        }

        let mut connect_opts = SqliteConnectOptions::from_str(path)?
            //.pragma("key", password.to_string())
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Off);

        connect_opts.log_statements(LevelFilter::Trace);
        connect_opts.log_slow_statements(LevelFilter::Trace, Duration::from_micros(10));

        let conn = SqlitePool::connect_with(connect_opts).await?;

        info!(target: "wallet::walletdb", "Opened wallet Sqlite connection at path {}", path);
        Ok(Arc::new(WalletDb { conn }))
    }

    /// This function executes a given SQL query, but isn't able to return anything.
    /// Therefore it's best to use it for initializing a table or similar things.
    pub async fn exec_sql(&self, query: &str) -> Result<()> {
        info!(target: "wallet::walletdb", "walletdb: Executing SQL query");
        debug!(target: "wallet::walletdb", "\n{}", query);
        let mut conn = self.conn.acquire().await?;
        sqlx::query(query).execute(&mut conn).await?;
        Ok(())
    }
}
