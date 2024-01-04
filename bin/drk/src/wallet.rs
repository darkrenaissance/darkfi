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

use anyhow::Result;
use darkfi::rpc::jsonrpc::JsonRequest;
use serde_json::json;

use super::Drk;

pub const BALANCE_BASE10_DECIMALS: usize = 8;

impl Drk {
    /// Initialize wallet with tables for drk
    pub async fn initialize_wallet(&self) -> Result<()> {
        let wallet_schema = include_str!("../wallet.sql");

        // We perform a request to darkfid with the schema to initialize
        // the necessary tables in the wallet.
        let req = JsonRequest::new("wallet.exec_sql", json!([wallet_schema]));
        let rep = self.rpc_client.request(req).await?;

        if rep == true {
            eprintln!("Successfully initialized wallet schema for drk");
        } else {
            eprintln!("[initialize_wallet] Got unexpected reply from darkfid: {}", rep);
        }

        Ok(())
    }
}
