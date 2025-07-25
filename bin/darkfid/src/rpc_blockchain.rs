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

use std::str::FromStr;

use darkfi_sdk::{
    crypto::contract_id::{ContractId, SMART_CONTRACT_ZKAS_DB_NAME},
    tx::TransactionHash,
};
use darkfi_serial::{deserialize_async, serialize_async};
use log::{debug, error};
use tinyjson::JsonValue;

use darkfi::{
    rpc::jsonrpc::{
        ErrorCode::{InternalError, InvalidParams, ParseError},
        JsonError, JsonResponse, JsonResult,
    },
    util::encoding::base64,
};

use crate::{server_error, DarkfiNode, RpcError};

impl DarkfiNode {
    // RPCAPI:
    // Queries the blockchain database for a block in the given height.
    // Returns a readable block upon success.
    //
    // **Params:**
    // * `array[0]`: `u64` Block height (as string)
    //
    // **Returns:**
    // * [`BlockInfo`](https://darkrenaissance.github.io/darkfi/dev/darkfi/blockchain/block_store/struct.BlockInfo.html)
    //   struct serialized into base64.
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.get_block", "params": ["0"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn blockchain_get_block(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let block_height = match params[0].get::<String>().unwrap().parse::<u32>() {
            Ok(v) => v,
            Err(_) => return JsonError::new(ParseError, None, id).into(),
        };

        let blocks = match self.validator.blockchain.get_blocks_by_heights(&[block_height]) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "darkfid::rpc::blockchain_get_block", "Failed fetching block by height: {e}");
                return JsonError::new(InternalError, None, id).into()
            }
        };

        if blocks.is_empty() {
            return server_error(RpcError::UnknownBlockHeight, id, None)
        }

        let block = base64::encode(&serialize_async(&blocks[0]).await);
        JsonResponse::new(JsonValue::String(block), id).into()
    }

    // RPCAPI:
    // Queries the blockchain database for a given transaction.
    // Returns a serialized `Transaction` object.
    //
    // **Params:**
    // * `array[0]`: Hex-encoded transaction hash string
    //
    // **Returns:**
    // * Serialized [`Transaction`](https://darkrenaissance.github.io/darkfi/dev/darkfi/tx/struct.Transaction.html)
    //   object encoded with base64
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.get_tx", "params": ["TxHash"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "ABCD...", "id": 1}
    pub async fn blockchain_get_tx(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let tx_hash = params[0].get::<String>().unwrap();
        let tx_hash = match TransactionHash::from_str(tx_hash) {
            Ok(v) => v,
            Err(_) => return JsonError::new(ParseError, None, id).into(),
        };

        let txs = match self.validator.blockchain.transactions.get(&[tx_hash], true) {
            Ok(txs) => txs,
            Err(e) => {
                error!(target: "darkfid::rpc::blockchain_get_tx", "Failed fetching tx by hash: {e}");
                return JsonError::new(InternalError, None, id).into()
            }
        };
        // This would be an logic error somewhere
        assert_eq!(txs.len(), 1);
        // and strict was used during .get()
        let tx = txs[0].as_ref().unwrap();

        let tx_enc = base64::encode(&serialize_async(tx).await);
        JsonResponse::new(JsonValue::String(tx_enc), id).into()
    }

    // RPCAPI:
    // Queries the blockchain database to find the last confirmed block.
    //
    // **Params:**
    // * `None`
    //
    // **Returns:**
    // * `f64`   : Height of the last confirmed block
    // * `String`: Header hash of the last confirmed block
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.last_confirmed_block", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [1234, "HeaderHash"], "id": 1}
    pub async fn blockchain_last_confirmed_block(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let Ok((height, hash)) = self.validator.blockchain.last() else {
            return JsonError::new(InternalError, None, id).into()
        };

        JsonResponse::new(
            JsonValue::Array(vec![
                JsonValue::Number(height as f64),
                JsonValue::String(hash.to_string()),
            ]),
            id,
        )
        .into()
    }

    // RPCAPI:
    // Queries the validator to find the current best fork next block height.
    //
    // **Params:**
    // * `None`
    //
    // **Returns:**
    // * `f64`: Current best fork next block height
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.best_fork_next_block_height", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": 1234, "id": 1}
    pub async fn blockchain_best_fork_next_block_height(
        &self,
        id: u16,
        params: JsonValue,
    ) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let Ok(next_block_height) = self.validator.best_fork_next_block_height().await else {
            return JsonError::new(InternalError, None, id).into()
        };

        JsonResponse::new(JsonValue::Number(next_block_height as f64), id).into()
    }

    // RPCAPI:
    // Queries the validator to get the currently configured block target time.
    //
    // **Params:**
    // * `None`
    //
    // **Returns:**
    // * `f64`: Current block target time
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.block_target", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": 1234, "id": 1}
    pub async fn blockchain_block_target(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let block_target = self.validator.consensus.module.read().await.target;

        JsonResponse::new(JsonValue::Number(block_target as f64), id).into()
    }

    // RPCAPI:
    // Initializes a subscription to new incoming blocks.
    // Once a subscription is established, `darkfid` will send JSON-RPC notifications of
    // new incoming blocks to the subscriber.
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.subscribe_blocks", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "method": "blockchain.subscribe_blocks", "params": [`blockinfo`]}
    pub async fn blockchain_subscribe_blocks(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        self.subscribers.get("blocks").unwrap().clone().into()
    }

    // RPCAPI:
    // Initializes a subscription to new incoming transactions.
    // Once a subscription is established, `darkfid` will send JSON-RPC notifications of
    // new incoming transactions to the subscriber.
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.subscribe_txs", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "method": "blockchain.subscribe_txs", "params": [`tx_hash`]}
    pub async fn blockchain_subscribe_txs(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        self.subscribers.get("txs").unwrap().clone().into()
    }

    // RPCAPI:
    // Initializes a subscription to new incoming proposals. Once a subscription is established,
    // `darkfid` will send JSON-RPC notifications of new incoming proposals to the subscriber.
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.subscribe_proposals", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "method": "blockchain.subscribe_proposals", "params": [`blockinfo`]}
    pub async fn blockchain_subscribe_proposals(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        self.subscribers.get("proposals").unwrap().clone().into()
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
    //   [`ZkBinary`](https://darkrenaissance.github.io/darkfi/dev/darkfi/zkas/decoder/struct.ZkBinary.html)
    //   object
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.lookup_zkas", "params": ["BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [["Foo", "ABCD..."], ["Bar", "EFGH..."]], "id": 1}
    pub async fn blockchain_lookup_zkas(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let contract_id = params[0].get::<String>().unwrap();
        let contract_id = match ContractId::from_str(contract_id) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "darkfid::rpc::blockchain_lookup_zkas", "Error decoding string to ContractId: {e}");
                return JsonError::new(InvalidParams, None, id).into()
            }
        };

        let Ok(zkas_db) = self.validator.blockchain.contracts.lookup(
            &self.validator.blockchain.sled_db,
            &contract_id,
            SMART_CONTRACT_ZKAS_DB_NAME,
        ) else {
            error!(target: "darkfid::rpc::blockchain_lookup_zkas", "Did not find zkas db for ContractId: {contract_id}");
            return server_error(RpcError::ContractZkasDbNotFound, id, None)
        };

        let mut ret = vec![];

        for i in zkas_db.iter() {
            debug!(target: "darkfid::rpc::blockchain_lookup_zkas", "Iterating over zkas db");
            let Ok((zkas_ns, zkas_bytes)) = i else {
                error!(target: "darkfid::rpc::blockchain_lookup_zkas", "Internal sled error iterating db");
                return JsonError::new(InternalError, None, id).into()
            };

            let Ok(zkas_ns) = deserialize_async(&zkas_ns).await else {
                return JsonError::new(InternalError, None, id).into()
            };

            let (zkbin, _): (Vec<u8>, Vec<u8>) = match deserialize_async(&zkas_bytes).await {
                Ok(pair) => pair,
                Err(_) => return JsonError::new(InternalError, None, id).into(),
            };

            let zkas_bincode = base64::encode(&zkbin);
            ret.push(JsonValue::Array(vec![
                JsonValue::String(zkas_ns),
                JsonValue::String(zkas_bincode),
            ]));
        }

        JsonResponse::new(JsonValue::Array(ret), id).into()
    }

    // RPCAPI:
    // Queries the blockchain database for a given contract state records.
    // Returns the records value raw bytes as a `BTreeMap`.
    //
    // **Params:**
    // * `array[0]`: base58-encoded contract ID string
    // * `array[1]`: Contract tree name string
    //
    // **Returns:**
    // * Records serialized `BTreeMap` encoded with base64
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.get_contract_state", "params": ["BZHK...", "tree"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "ABCD...", "id": 1}
    pub async fn blockchain_get_contract_state(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 2 || !params[0].is_string() || !params[1].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let contract_id = params[0].get::<String>().unwrap();
        let contract_id = match ContractId::from_str(contract_id) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "darkfid::rpc::blockchain_get_contract_state", "Error decoding string to ContractId: {e}");
                return JsonError::new(InvalidParams, None, id).into()
            }
        };

        let tree_name = params[1].get::<String>().unwrap();

        match self.validator.blockchain.contracts.get_state_tree_records(
            &self.validator.blockchain.sled_db,
            &contract_id,
            tree_name,
        ) {
            Ok(records) => JsonResponse::new(
                JsonValue::String(base64::encode(&serialize_async(&records).await)),
                id,
            )
            .into(),
            Err(e) => {
                error!(target: "darkfid::rpc::blockchain_get_contract_state", "Failed fetching contract state records: {e}");
                server_error(RpcError::ContractStateNotFound, id, None)
            }
        }
    }

    // RPCAPI:
    // Queries the blockchain database for a given contract state key raw bytes.
    // Returns the record value raw bytes.
    //
    // **Params:**
    // * `array[0]`: base58-encoded contract ID string
    // * `array[1]`: Contract tree name string
    // * `array[2]`: Key raw bytes, encoded with base64
    //
    // **Returns:**
    // * Record value raw bytes encoded with base64
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.get_contract_state_key", "params": ["BZHK...", "tree", "ABCD..."], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "ABCD...", "id": 1}
    pub async fn blockchain_get_contract_state_key(
        &self,
        id: u16,
        params: JsonValue,
    ) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 3 ||
            !params[0].is_string() ||
            !params[1].is_string() ||
            !params[2].is_string()
        {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let contract_id = params[0].get::<String>().unwrap();
        let contract_id = match ContractId::from_str(contract_id) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "darkfid::rpc::blockchain_get_contract_state_key", "Error decoding string to ContractId: {e}");
                return JsonError::new(InvalidParams, None, id).into()
            }
        };

        let tree_name = params[1].get::<String>().unwrap();

        let key_enc = params[2].get::<String>().unwrap().trim();
        let Some(key) = base64::decode(key_enc) else {
            error!(target: "darkfid::rpc::blockchain_get_contract_state_key", "Failed decoding base64 key");
            return server_error(RpcError::ParseError, id, None)
        };

        match self.validator.blockchain.contracts.get_state_tree_value(
            &self.validator.blockchain.sled_db,
            &contract_id,
            tree_name,
            &key,
        ) {
            Ok(value) => JsonResponse::new(JsonValue::String(base64::encode(&value)), id).into(),
            Err(e) => {
                error!(target: "darkfid::rpc::blockchain_get_contract_state_key", "Failed fetching contract state key value: {e}");
                server_error(RpcError::ContractStateKeyNotFound, id, None)
            }
        }
    }
}
