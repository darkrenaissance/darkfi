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

use std::{collections::HashMap, str::FromStr};

use darkfi::{
    rpc::{
        jsonrpc::{JsonRequest, JsonResponse},
        util::JsonValue,
    },
    Error, Result,
};
use log::{debug, error};
use monero::blockdata::transaction::{ExtraField, RawExtraField, SubField::MergeMining};

use super::MiningProxy;

impl MiningProxy {
    /// Perform a JSON-RPC GET request to monerod's endpoint with the given method
    async fn monero_get_request(&self, method: &str) -> Result<JsonValue> {
        let endpoint = format!("{}{}", self.monerod_rpc, method);
        debug!(target: "monerod::monero_get_request", "--> {}", endpoint);

        let mut rep = match surf::get(&endpoint).await {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "monerod::monero_get_request",
                    "Failed sending GET request to monerod: {}", e,
                );
                return Err(Error::Custom(format!("Failed sending GET request to monerod: {}", e)))
            }
        };

        let json_str: JsonValue = match rep.body_string().await {
            Ok(v) => match v.parse() {
                Ok(v) => v,
                Err(e) => {
                    error!(
                        target: "monerod::monero_get_request",
                        "Failed parsing JSON body string from monerod GET request response: {}", e,
                    );
                    return Err(Error::Custom(format!(
                        "Failed parsing JSON body string from monerod GET request response: {}",
                        e
                    )))
                }
            },
            Err(e) => {
                error!(
                   target: "monerod::monero_get_request",
                   "Failed parsing body string from monerod GET request response: {}", e,
                );
                return Err(Error::Custom(format!(
                    "Failed parsing body string from monerod GET request response: {}",
                    e
                )))
            }
        };

        Ok(json_str)
    }

    /// Perform a JSON-RPC POST request to monerod's endpoint with the given method
    /// and JSON-RPC request
    pub async fn monero_post_request(&self, req: JsonRequest) -> Result<JsonValue> {
        let endpoint = format!("{}json_rpc", self.monerod_rpc);
        debug!(target: "monerod::monero_post_request", "--> {}", endpoint);

        let client = surf::Client::new();

        let mut response = match client
            .get(endpoint)
            .header("Content-Type", "application/json")
            .body(req.stringify()?)
            .send()
            .await
        {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "monerod::monero_post_request",
                    "Failed sending monerod RPC POST request: {}", e,
                );
                return Err(Error::Custom(format!("Failed sending monerod RPC POST request: {}", e)))
            }
        };

        let response_bytes = match response.body_bytes().await {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "monerod::monero_post_request",
                    "Failed decoding monerod RPC POST response body: {}", e,
                );
                return Err(Error::Custom(format!(
                    "Failed decoding monerod RPC POST response body: {}",
                    e,
                )))
            }
        };

        let response_string = match String::from_utf8(response_bytes) {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "monerod::monero_post_request",
                    "Failed decoding UTF8 string from monerod RPC POST response body: {}", e,
                );
                return Err(Error::Custom(format!(
                    "Failed decoding UTF8 string from monerod RPC POST response body: {}",
                    e,
                )))
            }
        };

        let response_json: JsonValue = match response_string.parse() {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "monerod::monero_post_request",
                    "Failed parsing JSON string from monerod RPC POST response body: {}", e,
                );
                return Err(Error::Custom(format!(
                    "Failed parsing JSON string from monerod RPC POST response body: {}",
                    e
                )))
            }
        };

        Ok(response_json)
    }

    /// Proxy the `getheight` RPC request
    pub async fn monerod_get_height(&self) -> Result<JsonValue> {
        let rep = self.monero_get_request("getheight").await?;
        Ok(rep)
    }

    /// Proxy the `getinfo` RPC request
    pub async fn monerod_get_info(&self) -> Result<JsonValue> {
        let rep = self.monero_get_request("getinfo").await?;
        Ok(rep)
    }

    /// Proxy the `submitblock` RPC request
    pub async fn monerod_submit_block(&self, req: &JsonValue) -> Result<JsonValue> {
        let request = JsonRequest::try_from(req)?;
        let response = self.monero_post_request(request).await?;
        Ok(response)
    }

    /// Perform the `getblocktemplate` request and modify it with the necessary
    /// merge mining data.
    pub async fn monerod_getblocktemplate(&self, req: &JsonValue) -> Result<JsonValue> {
        let mut request = JsonRequest::try_from(req)?;

        if !request.params.is_object() {
            return Err(Error::Custom("Invalid request".to_string()))
        }

        let params: &mut HashMap<String, JsonValue> = request.params.get_mut().unwrap();
        if !params.contains_key("wallet_address") || !params.contains_key("reserve_size") {
            return Err(Error::Custom("Invalid request".to_string()))
        }

        let Some(wallet_address) = params["wallet_address"].get::<String>() else {
            return Err(Error::Custom("Invalid request".to_string()))
        };

        let Ok(wallet_address) = monero::Address::from_str(wallet_address) else {
            return Err(Error::Custom("Invalid request".to_string()))
        };

        if wallet_address.network != self.monerod_network {
            return Err(Error::Custom("Monero network address mismatch".to_string()))
        }

        if wallet_address.addr_type != monero::AddressType::Standard {
            return Err(Error::Custom("Non-standard Monero address".to_string()))
        }

        // Create the Merge Mining data
        let mm_tag = MergeMining(Some(monero::VarInt(32)), monero::Hash([0_u8; 32]));

        // Construct `tx_extra` from all the extra fields we have to add to
        // the coinbase transaction in the block we're mining.
        let tx_extra: RawExtraField = ExtraField(vec![mm_tag]).into();

        // Modify the params `reserve_size` to fit our Merge Mining data
        *params.get_mut("reserve_size").unwrap() = (tx_extra.0.len() as f64).into();

        // Perform the `getblocktemplate` call:
        let gbt_response = self.monero_post_request(request).await?;
        let mut gbt_response = JsonResponse::try_from(&gbt_response)?;
        let gbt_result: &mut HashMap<String, JsonValue> = gbt_response.result.get_mut().unwrap();

        // Now we have to modify the block template:
        let mut block_template = monero::consensus::deserialize::<monero::Block>(
            &hex::decode(gbt_result["blocktemplate_blob"].get::<String>().unwrap()).unwrap(),
        )
        .unwrap();

        // Update coinbase tx with our extra field
        block_template.miner_tx.prefix.extra = tx_extra;

        // Update `blocktemplate_blob` with the modified block:
        gbt_result.insert(
            "blocktemplate_blob".to_string(),
            hex::encode(monero::consensus::serialize(&block_template)).into(),
        );

        // Update `blockhashing_blob` in order to perform correct PoW:
        gbt_result.insert(
            "blockhashing_blob".to_string(),
            hex::encode(block_template.serialize_hashable()).into(),
        );

        // Return the modified JSON response
        Ok((&gbt_response).into())
    }
}
