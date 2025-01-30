/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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
use log::{debug, error, info};
use monero::blockdata::transaction::{ExtraField, RawExtraField, SubField::MergeMining};

use super::MiningProxy;

/// Types of requests that can be sent to monerod
pub(crate) enum MonerodRequest {
    Get(String),
    Post(JsonRequest),
}

impl MiningProxy {
    /// Perform a JSON-RPC request to monerod's endpoint with the given method
    pub(crate) async fn monero_request(&self, req: MonerodRequest) -> Result<JsonValue> {
        let mut rep = match req {
            MonerodRequest::Get(method) => {
                let endpoint = format!("{}{}", self.monero_rpc, method);

                match surf::get(&endpoint).await {
                    Ok(v) => v,
                    Err(e) => {
                        let e = format!("Failed sending monerod GET request: {}", e);
                        error!(target: "monerod::monero_request", "{}", e);
                        return Err(Error::Custom(e))
                    }
                }
            }
            MonerodRequest::Post(data) => {
                let endpoint = format!("{}json_rpc", self.monero_rpc);
                let client = surf::Client::new();

                match client
                    .get(endpoint)
                    .header("Content-Type", "application/json")
                    .body(data.stringify().unwrap())
                    .send()
                    .await
                {
                    Ok(v) => v,
                    Err(e) => {
                        let e = format!("Failed sending monerod POST request: {}", e);
                        error!(target: "monerod::monero_request", "{}", e);
                        return Err(Error::Custom(e))
                    }
                }
            }
        };

        let json_rep: JsonValue = match rep.body_string().await {
            Ok(v) => match v.parse() {
                Ok(v) => v,
                Err(e) => {
                    let e = format!("Failed parsing JSON string from monerod response: {}", e);
                    error!(target: "monerod::monero_request", "{}", e);
                    return Err(Error::Custom(e))
                }
            },
            Err(e) => {
                let e = format!("Failed parsing body string from monerod response:  {}", e);
                error!(target: "monerod::monero_request", "{}", e);
                return Err(Error::Custom(e))
            }
        };

        Ok(json_rep)
    }

    /// Proxy the `/getheight` RPC request
    pub async fn monerod_get_height(&self) -> Result<JsonValue> {
        info!(target: "monerod::getheight", "Proxying /getheight request");
        let rep = self.monero_request(MonerodRequest::Get("getheight".to_string())).await?;
        Ok(rep)
    }

    /// Proxy the `/getinfo` RPC request
    pub async fn monerod_get_info(&self) -> Result<JsonValue> {
        info!(target: "monerod::getinfo", "Proxying /getinfo request");
        let rep = self.monero_request(MonerodRequest::Get("getinfo".to_string())).await?;
        Ok(rep)
    }

    /// Proxy the `submitblock` RPC request
    pub async fn monerod_submit_block(&self, req: &JsonValue) -> Result<JsonValue> {
        info!(target: "monerod::submitblock", "Proxying submitblock request");
        let request = JsonRequest::try_from(req)?;

        if !request.params.is_array() {
            return Err(Error::Custom("Invalid request".to_string()))
        }

        for block in request.params.get::<Vec<JsonValue>>().unwrap() {
            let Some(block) = block.get::<String>() else {
                return Err(Error::Custom("Invalid request".to_string()))
            };

            debug!(
                target: "monerod::submitblock", "{:#?}",
                monero::consensus::deserialize::<monero::Block>(&hex::decode(block).unwrap()).unwrap(),
            );
        }

        let response = self.monero_request(MonerodRequest::Post(request)).await?;
        Ok(response)
    }

    /// Perform the `getblocktemplate` request and modify it with the necessary
    /// merge mining data.
    pub async fn monerod_getblocktemplate(&self, req: &JsonValue) -> Result<JsonValue> {
        info!(target: "monerod::getblocktemplate", "Proxying getblocktemplate request");
        let mut request = JsonRequest::try_from(req)?;

        if !request.params.is_object() {
            return Err(Error::Custom("Invalid request".to_string()))
        }

        let params: &mut HashMap<String, JsonValue> = request.params.get_mut().unwrap();
        if !params.contains_key("wallet_address") {
            return Err(Error::Custom("Invalid request".to_string()))
        }

        let Some(wallet_address) = params["wallet_address"].get::<String>() else {
            return Err(Error::Custom("Invalid request".to_string()))
        };

        let Ok(wallet_address) = monero::Address::from_str(wallet_address) else {
            return Err(Error::Custom("Invalid request".to_string()))
        };

        if wallet_address.network != self.monero_network {
            return Err(Error::Custom("Monero network address mismatch".to_string()))
        }

        if wallet_address.addr_type != monero::AddressType::Standard {
            return Err(Error::Custom("Non-standard Monero address".to_string()))
        }

        // Create the Merge Mining data
        // TODO: This is where we're gonna include the necessary DarkFi data
        // that has to end up in Monero blocks.
        let mm_tag = MergeMining(monero::VarInt(32), monero::Hash([0_u8; 32]));

        // Construct `tx_extra` from all the extra fields we have to add to
        // the coinbase transaction in the block we're mining.
        let tx_extra: RawExtraField = ExtraField(vec![mm_tag]).into();

        // Modify the params `reserve_size` to fit our Merge Mining data
        debug!(target: "monerod::getblocktemplate", "Inserting \"reserve_size\":{}", tx_extra.0.len());
        params.insert("reserve_size".to_string(), (tx_extra.0.len() as f64).into());

        // Remove `extra_nonce` from the request, XMRig tends to send this in daemon-mode
        params.remove("extra_nonce");

        // Perform the `getblocktemplate` call:
        let gbt_response = self.monero_request(MonerodRequest::Post(request)).await?;
        debug!(target: "monerod::getblocktemplate", "Got {}", gbt_response.stringify()?);
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
