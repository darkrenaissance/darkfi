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
use darkfi::rpc::{client::RpcClient, jsonrpc::JsonRequest};
use darkfi_sdk::crypto::{PublicKey, TokenId};
use serde_json::json;
use url::Url;

use super::Drk;

impl Drk {
    /// Request an airdrop of `amount` `token_id` tokens from a faucet.
    /// Returns a transaction ID on success.
    pub async fn request_airdrop(
        &self,
        faucet_endpoint: Url,
        amount: f64,
        token_id: TokenId,
        address: PublicKey,
    ) -> Result<String> {
        let rpc_client = RpcClient::new(faucet_endpoint).await?;
        let params = json!([format!("{}", address), amount, format!("{}", token_id),]);
        let req = JsonRequest::new("airdrop", params);
        let rep = rpc_client.oneshot_request(req).await?;

        let txid = serde_json::from_value(rep)?;

        Ok(txid)
    }
}
