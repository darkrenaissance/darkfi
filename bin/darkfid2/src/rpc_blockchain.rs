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

use std::str::FromStr;

use darkfi_sdk::crypto::ContractId;
use darkfi_serial::{deserialize, serialize};
use log::{debug, error};
use serde_json::{json, Value};

use darkfi::{
    rpc::jsonrpc::{
        ErrorCode::{InternalError, InvalidParams, ParseError},
        JsonError, JsonResponse, JsonResult,
    },
    runtime::vm_runtime::SMART_CONTRACT_ZKAS_DB_NAME,
};

use crate::{server_error, Darkfid, RpcError};

impl Darkfid {
    // RPCAPI:
    // Queries the blockchain database for a block in the given slot.
    // Returns a readable block upon success.
    //
    // **Params:**
    // * `array[0]`: `u64` slot ID
    //
    // **Returns:**
    // * [`BlockInfo`](https://darkrenaissance.github.io/darkfi/development/darkfi/consensus/block/struct.BlockInfo.html)
    //   struct as a JSON object
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.get_slot", "params": [0], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn blockchain_get_slot(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_u64() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let slot = params[0].as_u64().unwrap();
        let validator = self.validator.read().await;

        let blocks = match validator.blockchain.get_blocks_by_slot(&[slot]) {
            Ok(v) => {
                drop(validator);
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
    // Queries the blockchain database for a given transaction.
    // Returns a serialized `Transaction` object.
    //
    // **Params:**
    // * `array[0]`: Hex-encoded transaction hash string
    //
    // **Returns:**
    // * Serialized [`Transaction`](https://darkrenaissance.github.io/darkfi/development/darkfi/tx/struct.Transaction.html)
    //   object
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.get_tx", "params": ["TxHash"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn blockchain_get_tx(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let tx_hash_str = if let Some(tx_hash_str) = params[0].as_str() {
            tx_hash_str
        } else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        let tx_hash = if let Ok(tx_hash) = blake3::Hash::from_hex(tx_hash_str) {
            tx_hash
        } else {
            return JsonError::new(ParseError, None, id).into()
        };

        let validator = self.validator.read().await;

        let txs = match validator.blockchain.transactions.get(&[tx_hash], true) {
            Ok(txs) => {
                drop(validator);
                txs
            }
            Err(e) => {
                error!("[RPC] blockchain.get_tx: Failed fetching tx by hash: {}", e);
                return JsonError::new(InternalError, None, id).into()
            }
        };
        // This would be an logic error somewhere
        assert_eq!(txs.len(), 1);
        // and strict was used during .get()
        let tx = txs[0].as_ref().unwrap();

        JsonResponse::new(json!(serialize(tx)), id).into()
    }

    // RPCAPI:
    // Queries the blockchain database to find the last known slot
    //
    // **Params:**
    // * `None`
    //
    // **Returns:**
    // * `u64` ID of the last known slot
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.last_known_slot", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": 1234, "id": 1}
    pub async fn blockchain_last_known_slot(&self, id: Value, params: &[Value]) -> JsonResult {
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let blockchain = { self.validator.read().await.blockchain.clone() };
        let Ok(last_slot) = blockchain.last() else {
            return JsonError::new(InternalError, None, id).into()
        };

        JsonResponse::new(json!(last_slot.0), id).into()
    }

    // RPCAPI:
    // Performs a lookup of zkas bincodes for a given contract ID and returns all of
    // them, including their namespace.
    //
    // **Params:**
    // * `array[0]`: base58-encoded contract ID string
    //
    // **Returns:**
    // * `array[n]`: Pairs of: `zkas_namespace` string, serialized
    //   [`ZkBinary`](https://darkrenaissance.github.io/darkfi/development/darkfi/zkas/decoder/struct.ZkBinary.html)
    //   object
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.lookup_zkas", "params": ["6Ef42L1KLZXBoxBuCDto7coi9DA2D2SRtegNqNU4sd74"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [["Foo", [...]], ["Bar", [...]]], "id": 1}
    pub async fn blockchain_lookup_zkas(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let contract_id = match ContractId::from_str(params[0].as_str().unwrap()) {
            Ok(v) => v,
            Err(e) => {
                error!("[RPC] blockchain.lookup_zkas: Error decoding string to ContractId: {}", e);
                return JsonError::new(InvalidParams, None, id).into()
            }
        };

        let blockchain = { self.validator.read().await.blockchain.clone() };

        let Ok(zkas_db) = blockchain.contracts.lookup(
            &blockchain.sled_db,
            &contract_id,
            SMART_CONTRACT_ZKAS_DB_NAME,
        ) else {
            error!(
                "[RPC] blockchain.lookup_zkas: Did not find zkas db for ContractId: {}",
                contract_id
            );
            return server_error(RpcError::ContractZkasDbNotFound, id, None)
        };

        let mut ret: Vec<(String, Vec<u8>)> = vec![];

        for i in zkas_db.iter() {
            debug!("Iterating over zkas db");
            let Ok((zkas_ns, zkas_bytes)) = i else {
                error!("Internal sled error iterating db");
                return JsonError::new(InternalError, None, id).into()
            };

            let Ok(zkas_ns) = deserialize(&zkas_ns) else {
                return JsonError::new(InternalError, None, id).into()
            };

            let Ok((zkas_bincode, _)): Result<(Vec<u8>, Vec<u8>), std::io::Error> =
                deserialize(&zkas_bytes)
            else {
                return JsonError::new(InternalError, None, id).into()
            };

            ret.push((zkas_ns, zkas_bincode.to_vec()));
        }

        JsonResponse::new(json!(ret), id).into()
    }
}
