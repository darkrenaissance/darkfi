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

use async_trait::async_trait;
use darkfi_sdk::crypto::{Keypair, PublicKey, SecretKey};
use darkfi_serial::deserialize;
use log::debug;

use crate::{wallet::WalletDb, Result};

const CONSENSUS_KEYS_TABLE: &str = "consensus_keys";
const CONSENSUS_KEYS_COLUMN_IS_DEFAULT: &str = "is_default";

#[async_trait]
pub trait ConsensusWallet {
    async fn get_default_keypair(&self) -> Result<Keypair>;
}

#[async_trait]
impl ConsensusWallet for WalletDb {
    async fn get_default_keypair(&self) -> Result<Keypair> {
        debug!(target: "consensus::wallet", "Returning default keypair");

        let wallet_conn = self.conn.lock().await;
        let mut stmt = wallet_conn.prepare(&format!(
            "SELECT * FROM {} WHERE {} = 1",
            CONSENSUS_KEYS_TABLE, CONSENSUS_KEYS_COLUMN_IS_DEFAULT
        ))?;

        let (public, secret): (PublicKey, SecretKey) = stmt.query_row((), |row| {
            let p_bytes: Vec<u8> = row.get("public")?;
            let s_bytes: Vec<u8> = row.get("secret")?;
            let public = deserialize(&p_bytes).unwrap();
            let secret = deserialize(&s_bytes).unwrap();
            Ok((public, secret))
        })?;
        stmt.finalize()?;

        Ok(Keypair { secret, public })
    }
}
