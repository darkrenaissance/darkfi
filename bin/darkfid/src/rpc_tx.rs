/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use darkfi_serial::deserialize_async;
use log::error;
use tinyjson::JsonValue;

use darkfi::{
    rpc::jsonrpc::{
        ErrorCode::{InternalError, InvalidParams},
        JsonError, JsonResponse, JsonResult,
    },
    tx::Transaction,
    util::encoding::base64,
};

use super::Darkfid;
use crate::{server_error, RpcError};

impl Darkfid {
    // RPCAPI:
    // Simulate a network state transition with the given transaction.
    // Returns `true` if the transaction is valid, otherwise, a corresponding
    // error.
    //
    // --> {"jsonrpc": "2.0", "method": "tx.simulate", "params": ["base64encodedTX"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn tx_simulate(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        if !*self.validator.synced.read().await {
            error!(target: "darkfid::rpc::tx_simulate", "Blockchain is not synced");
            return server_error(RpcError::NotSynced, id, None)
        }

        // Try to deserialize the transaction
        let tx_enc = params[0].get::<String>().unwrap().trim();
        let tx_bytes = match base64::decode(tx_enc) {
            Some(v) => v,
            None => {
                error!(target: "darkfid::rpc::tx_simulate", "Failed decoding base64 transaction");
                return server_error(RpcError::ParseError, id, None)
            }
        };

        let tx: Transaction = match deserialize_async(&tx_bytes).await {
            Ok(v) => v,
            Err(e) => {
                error!(target: "darkfid::rpc::tx_simulate", "Failed deserializing bytes into Transaction: {}", e);
                return server_error(RpcError::ParseError, id, None)
            }
        };

        // Simulate state transition
        let result = self.validator.append_tx(&tx, false).await;
        if result.is_err() {
            error!(
                target: "darkfid::rpc::tx_simulate", "Failed to validate state transition: {}",
                result.err().unwrap()
            );
            return server_error(RpcError::TxSimulationFail, id, None)
        };

        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }

    // RPCAPI:
    // Broadcast a given transaction to the P2P network.
    // The function will first simulate the state transition in order to see
    // if the transaction is actually valid, and in turn it will return an
    // error if this is the case. Otherwise, a transaction ID will be returned.
    //
    // --> {"jsonrpc": "2.0", "method": "tx.broadcast", "params": ["base64encodedTX"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "txID...", "id": 1}
    pub async fn tx_broadcast(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        if !*self.validator.synced.read().await {
            error!(target: "darkfid::rpc::tx_broadcast", "Blockchain is not synced");
            return server_error(RpcError::NotSynced, id, None)
        }

        // Try to deserialize the transaction
        let tx_enc = params[0].get::<String>().unwrap().trim();
        let tx_bytes = match base64::decode(tx_enc) {
            Some(v) => v,
            None => {
                error!(target: "darkfid::rpc::tx_broadcast", "Failed decoding base64 transaction");
                return server_error(RpcError::ParseError, id, None)
            }
        };

        let tx: Transaction = match deserialize_async(&tx_bytes).await {
            Ok(v) => v,
            Err(e) => {
                error!(target: "darkfid::rpc::tx_broadcast", "Failed deserializing bytes into Transaction: {}", e);
                return server_error(RpcError::ParseError, id, None)
            }
        };

        // Block production participants can directly perform
        // the state transition check and append to their
        // pending transactions store.
        let error_message = if self.miner {
            "Failed to append transaction to mempool"
        } else {
            "Failed to validate state transition"
        };
        // We'll perform the state transition check here.
        if let Err(e) = self.validator.append_tx(&tx, self.miner).await {
            error!(target: "darkfid::rpc::tx_broadcast", "{}: {}", error_message, e);
            return server_error(RpcError::TxSimulationFail, id, None)
        };

        self.p2p.broadcast(&tx).await;
        if self.p2p.hosts().channels().await.is_empty() {
            error!(target: "darkfid::rpc::tx_broadcast", "Failed broadcasting tx, no connected channels");
            return server_error(RpcError::TxBroadcastFail, id, None)
        }

        let tx_hash = tx.hash().to_string();
        JsonResponse::new(JsonValue::String(tx_hash), id).into()
    }

    // RPCAPI:
    // Queries the node pending transactions store to retrieve all transactions.
    // Returns a vector of hex-encoded transaction hashes.
    //
    // --> {"jsonrpc": "2.0", "method": "tx.pending", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "[TxHash,...]", "id": 1}
    pub async fn tx_pending(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        if !*self.validator.synced.read().await {
            error!(target: "darkfid::rpc::tx_pending", "Blockchain is not synced");
            return server_error(RpcError::NotSynced, id, None)
        }

        let pending_txs = match self.validator.blockchain.get_pending_txs() {
            Ok(v) => v,
            Err(e) => {
                error!(target: "darkfid::rpc::tx_pending", "Failed fetching pending txs: {}", e);
                return JsonError::new(InternalError, None, id).into()
            }
        };

        let pending_txs: Vec<JsonValue> =
            pending_txs.iter().map(|x| JsonValue::String(x.hash().to_string())).collect();

        JsonResponse::new(JsonValue::Array(pending_txs), id).into()
    }

    // RPCAPI:
    // Queries the node pending transactions store to remove all transactions.
    // Returns a vector of hex-encoded transaction hashes.
    //
    // --> {"jsonrpc": "2.0", "method": "tx.clean_pending", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "[TxHash,...]", "id": 1}
    pub async fn tx_clean_pending(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        if !*self.validator.synced.read().await {
            error!(target: "darkfid::rpc::tx_clean_pending", "Blockchain is not synced");
            return server_error(RpcError::NotSynced, id, None)
        }

        let pending_txs = match self.validator.blockchain.get_pending_txs() {
            Ok(v) => v,
            Err(e) => {
                error!(target: "darkfid::rpc::tx_clean_pending", "Failed fetching pending txs: {}", e);
                return JsonError::new(InternalError, None, id).into()
            }
        };

        if let Err(e) = self.validator.blockchain.remove_pending_txs(&pending_txs) {
            error!(target: "darkfid::rpc::tx_clean_pending", "Failed fetching pending txs: {}", e);
            return JsonError::new(InternalError, None, id).into()
        };

        let pending_txs: Vec<JsonValue> =
            pending_txs.iter().map(|x| JsonValue::String(x.hash().to_string())).collect();

        JsonResponse::new(JsonValue::Array(pending_txs), id).into()
    }
}
