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

use darkfi::{rpc::client::RpcClient, system::ExecutorPtr, Result};
use url::Url;

/// damd JSON-RPC related methods
pub mod rpc;

/// CLI-util structure
pub struct DamCli {
    /// JSON-RPC client to execute requests to damd daemon
    pub rpc_client: RpcClient,
}

impl DamCli {
    pub async fn new(endpoint: &str, ex: &ExecutorPtr) -> Result<Self> {
        // Initialize rpc client
        let endpoint = Url::parse(endpoint)?;
        let rpc_client = RpcClient::new(endpoint, ex.clone()).await?;

        Ok(Self { rpc_client })
    }
}
