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

use std::sync::Arc;

use darkfi::{rpc::client::RpcClient, Result};
use smol::Executor;
use url::Url;

/// rlnd JSON-RPC related methods
pub mod rpc;

/// CLI-util structure
pub struct RlndCli {
    /// JSON-RPC client to execute requests to darkfid daemon
    pub rpc_client: RpcClient,
}

impl RlndCli {
    pub async fn new(endpoint: &str, ex: Arc<Executor<'static>>) -> Result<Self> {
        // Initialize rpc client
        let endpoint = Url::parse(endpoint)?;
        let rpc_client = RpcClient::new(endpoint, ex).await?;

        Ok(Self { rpc_client })
    }
}
