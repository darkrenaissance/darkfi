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

use darkfi_sdk::{crypto::ContractId, db::SMART_CONTRACT_ZKAS_DB_NAME};
use darkfi_serial::{deserialize, serialize};
use log::{debug, error};
use serde_json::{json, Value};

use darkfi::rpc::jsonrpc::{
    ErrorCode::{InternalError, InvalidParams},
    JsonError, JsonResponse, JsonResult, JsonSubscriber,
};

use super::Darkfid;
use crate::{server_error, RpcError};

impl Darkfid {
    // RPCAPI:
    // Queries the blockchain database for a block in the given slot.
    // Returns a readable block upon success.
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.get_slot", "params": [0], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn blockchain_get_slot(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_u64() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let slot = params[0].as_u64().unwrap();
        let validator_state = self.validator_state.read().await;

        let blocks = match validator_state.blockchain.get_blocks_by_slot(&[slot]) {
            Ok(v) => {
                drop(validator_state);
                v
            }
            Err(e) => {
                error!("[RPC] blockchain.get_slot: Failed fetching block by slot: {}", e);
                return JsonError::new(InternalError, None, id).into()
            }
        };

        if blocks.is_empty() {
            return server_error(RpcError::UnknownSlot, id, None)
        }

        JsonResponse::new(json!(serialize(&blocks[0])), id).into()
    }

    // RPCAPI:
    // Queries the blockchain database to find the last known slot
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.last_known_slot", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": 1234, "id": 1}
    pub async fn blockchain_last_known_slot(&self, id: Value, params: &[Value]) -> JsonResult {
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let blockchain = { self.validator_state.read().await.blockchain.clone() };
        let Ok(last_slot) = blockchain.last() else {
                return JsonError::new(InternalError, None, id).into()
        };

        JsonResponse::new(json!(last_slot.0), id).into()
    }

    // RPCAPI:
    // Initializes a subscription to new incoming blocks.
    // Once a subscription is established, `darkfid` will send JSON-RPC notifications of
    // new incoming blocks to the subscriber.
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.subscribe_blocks", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "method": "blockchain.subscribe_blocks", "params": [`blockinfo`]}
    pub async fn blockchain_subscribe_blocks(&self, id: Value, params: &[Value]) -> JsonResult {
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let blocks_subscriber =
            self.validator_state.read().await.subscribers.get("blocks").unwrap().clone();

        JsonSubscriber::new(blocks_subscriber).into()
    }

    // RPCAPI:
    // Performs a lookup of zkas bincodes for a given contract ID and returns all of
    // them, including their namespace.
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.lookup_zkas", "params": ["6Ef42L1KLZXBoxBuCDto7coi9DA2D2SRtegNqNU4sd74"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [["Foo", [...]], ["Bar", [...]]], "id": 1}
    pub async fn blockchain_lookup_zkas(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let contract_id = match ContractId::try_from(params[0].as_str().unwrap()) {
            Ok(v) => v,
            Err(e) => {
                error!("[RPC] blockchain.lookup_zkas: Error decoding string to ContractId: {}", e);
                return JsonError::new(InvalidParams, None, id).into()
            }
        };

        let blockchain = { self.validator_state.read().await.blockchain.clone() };

        let Ok(zkas_db) = blockchain.contracts.lookup(&blockchain.sled_db, &contract_id, SMART_CONTRACT_ZKAS_DB_NAME) else {
            error!("[RPC] blockchain.lookup_zkas: Did not find zkas db for ContractId: {}", contract_id);
            return server_error(RpcError::ContractZkasDbNotFound, id, None)
        };

        let mut ret: Vec<(String, Vec<u8>)> = vec![];

        for i in zkas_db.iter() {
            debug!("Iterating over zkas db");
            let Ok((zkas_ns, zkas_bincode)) = i else {
                error!("Internal sled error iterating db");
                return JsonError::new(InternalError, None, id).into()
            };

            let Ok(zkas_ns) = deserialize(&zkas_ns) else {
                return JsonError::new(InternalError, None, id).into()
            };

            ret.push((zkas_ns, zkas_bincode.to_vec()));
        }

        JsonResponse::new(json!(ret), id).into()
    }

    // RPCAPI:
    // Queries the blockchain database to check if the provided transaction hash exists
    // in the erroneous transactions set.
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.is_erroneous_tx", "params": [[tx_hash bytes]], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": bool, "id": 1}
    pub async fn blockchain_is_erroneous_tx(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_array() {
            return JsonError::new(InvalidParams, None, id).into()
        }
        let hash_bytes: [u8; 32] = serde_json::from_value(params[0].clone()).unwrap();
        let tx_hash = blake3::Hash::try_from(hash_bytes).unwrap();
        let blockchain = { self.validator_state.read().await.blockchain.clone() };
        let Ok(result) = blockchain.is_erroneous_tx(&tx_hash) else {
                return JsonError::new(InternalError, None, id).into()
        };

        JsonResponse::new(json!(result), id).into()
    }
}
