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

use anyhow::{anyhow, Result};
use darkfi::rpc::{client::RpcClient, jsonrpc::JsonRequest};
use darkfi_sdk::{
    crypto::{mimc_vdf, PublicKey},
    num_bigint::BigUint,
    num_traits::Num,
};
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
        address: PublicKey,
    ) -> Result<String> {
        let rpc_client = RpcClient::new(faucet_endpoint, None).await?;

        // First we request a VDF challenge from the faucet
        let params = json!([format!("{}", address)]);
        let req = JsonRequest::new("challenge", params);
        let rep = rpc_client.request(req).await?;

        let Some(rep) = rep.as_array() else {
            return Err(anyhow!("Invalid challenge response from faucet: {:?}", rep))
        };
        if rep.len() != 2 || !rep[0].is_string() || !rep[1].is_u64() {
            return Err(anyhow!("Invalid challenge response from faucet: {:?}", rep))
        }

        // Retrieve VDF challenge
        let challenge = BigUint::from_str_radix(rep[0].as_str().unwrap(), 16)?;
        let n_steps = rep[1].as_u64().unwrap();

        // Then evaluate the VDF
        eprintln!("Evaluating VDF with n_steps={} ... (this could take about a minute)", n_steps);
        let witness = mimc_vdf::eval(&challenge, n_steps);
        eprintln!("Done! Sending airdrop request...");

        // And finally request airdrop with the VDF evaluation witness
        let params = json!([format!("{}", address), amount, witness.to_str_radix(16)]);
        let req = JsonRequest::new("airdrop", params);
        let rep = rpc_client.oneshot_request(req).await?;

        let txid = serde_json::from_value(rep)?;

        Ok(txid)
    }
}
