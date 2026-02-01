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

use darkfi_serial::deserialize_async;
use tinyjson::JsonValue;
use tracing::{error, warn};

use darkfi::{
    rpc::jsonrpc::{
        ErrorCode::{InternalError, InvalidParams},
        JsonError, JsonResponse, JsonResult,
    },
    tx::Transaction,
    util::encoding::base64,
};

use super::DarkfiNode;
use crate::{server_error, RpcError};

impl DarkfiNode {
    // RPCAPI:
    // Simulate a network state transition with the given transaction.
    // Returns `true` if the transaction is valid, otherwise, a corresponding
    // error.
    //
    // --> {"jsonrpc": "2.0", "method": "tx.simulate", "params": ["base64encodedTX"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn tx_simulate(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let mut validator = self.validator.write().await;
        if !validator.synced {
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
                error!(target: "darkfid::rpc::tx_simulate", "Failed deserializing bytes into Transaction: {e}");
                return server_error(RpcError::ParseError, id, None)
            }
        };

        // Simulate state transition
        if let Err(e) = validator.append_tx(&tx, false).await {
            error!(target: "darkfid::rpc::tx_simulate", "Failed to validate state transition: {e}");
            return server_error(RpcError::TxSimulationFail, id, None)
        };

        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }

    // RPCAPI:
    // Append a given transaction to the mempool and broadcast it to
    // the P2P network. The function will first simulate the state
    // transition in order to see if the transaction is actually valid,
    // and in turn it will return an error if this is the case.
    // Otherwise, a transaction ID will be returned.
    //
    // --> {"jsonrpc": "2.0", "method": "tx.broadcast", "params": ["base64encodedTX"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "txID...", "id": 1}
    pub async fn tx_broadcast(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let mut validator = self.validator.write().await;
        if !validator.synced {
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
                error!(target: "darkfid::rpc::tx_broadcast", "Failed deserializing bytes into Transaction: {e}");
                return server_error(RpcError::ParseError, id, None)
            }
        };

        // We'll perform the state transition check here.
        if let Err(e) = validator.append_tx(&tx, true).await {
            error!(target: "darkfid::rpc::tx_broadcast", "Failed to append transaction to mempool: {e}");
            return server_error(RpcError::TxSimulationFail, id, None)
        };

        self.p2p_handler.p2p.broadcast(&tx).await;
        if !self.p2p_handler.p2p.is_connected() {
            warn!(target: "darkfid::rpc::tx_broadcast", "No connected channels to broadcast tx");
        }

        let tx_hash = tx.hash().to_string();
        JsonResponse::new(JsonValue::String(tx_hash), id).into()
    }

    // RPCAPI:
    // Queries the node pending transactions store to retrieve all transactions.
    // Returns a vector of hex-encoded transaction hashes.
    //
    // --> {"jsonrpc": "2.0", "method": "tx.pending", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["TxHash" , "..."], "id": 1}
    pub async fn tx_pending(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let validator = self.validator.read().await;
        if !validator.synced {
            error!(target: "darkfid::rpc::tx_pending", "Blockchain is not synced");
            return server_error(RpcError::NotSynced, id, None)
        }

        let pending_txs = match validator.blockchain.get_pending_txs() {
            Ok(v) => v,
            Err(e) => {
                error!(target: "darkfid::rpc::tx_pending", "Failed fetching pending txs: {e}");
                return JsonError::new(InternalError, None, id).into()
            }
        };

        let pending_txs: Vec<JsonValue> =
            pending_txs.iter().map(|x| JsonValue::String(x.hash().to_string())).collect();

        JsonResponse::new(JsonValue::Array(pending_txs), id).into()
    }

    // RPCAPI:
    // Queries the node pending transactions store to reset all
    // transactions. Unproposed transactions are removed.
    // Returns `true` if the operation was successful, otherwise, a
    // corresponding error.
    //
    // --> {"jsonrpc": "2.0", "method": "tx.clean_pending", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn tx_clean_pending(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let mut validator = self.validator.write().await;
        if !validator.synced {
            error!(target: "darkfid::rpc::tx_clean_pending", "Blockchain is not synced");
            return server_error(RpcError::NotSynced, id, None)
        }

        // Grab node registry locks
        let submit_lock = self.registry.submit_lock.write().await;
        let block_templates = self.registry.block_templates.write().await;
        let jobs = self.registry.jobs.write().await;
        let mm_jobs = self.registry.mm_jobs.write().await;

        // Purge all unproposed pending transactions from the database
        let result = validator
            .consensus
            .purge_unproposed_pending_txs(self.registry.proposed_transactions(&block_templates))
            .await;

        // Release registry locks
        drop(block_templates);
        drop(jobs);
        drop(mm_jobs);
        drop(submit_lock);

        // Check result
        if let Err(e) = result {
            error!(target: "darkfid::rpc::tx_clean_pending", "Failed removing pending txs: {e}");
            return JsonError::new(InternalError, None, id).into()
        };

        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }

    // RPCAPI:
    // Compute provided transaction's total gas, against current best fork.
    // Returns the gas value if the transaction is valid, otherwise, a corresponding
    // error.
    //
    // --> {"jsonrpc": "2.0", "method": "tx.calculate_fee", "params": ["base64encodedTX", "include_fee"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn tx_calculate_fee(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if params.len() != 2 || !params[0].is_string() || !params[1].is_bool() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let validator = self.validator.read().await;
        if !validator.synced {
            error!(target: "darkfid::rpc::tx_calculate_fee", "Blockchain is not synced");
            return server_error(RpcError::NotSynced, id, None)
        }

        // Try to deserialize the transaction
        let tx_enc = params[0].get::<String>().unwrap().trim();
        let tx_bytes = match base64::decode(tx_enc) {
            Some(v) => v,
            None => {
                error!(target: "darkfid::rpc::tx_calculate_fee", "Failed decoding base64 transaction");
                return server_error(RpcError::ParseError, id, None)
            }
        };

        let tx: Transaction = match deserialize_async(&tx_bytes).await {
            Ok(v) => v,
            Err(e) => {
                error!(target: "darkfid::rpc::tx_calculate_fee", "Failed deserializing bytes into Transaction: {e}");
                return server_error(RpcError::ParseError, id, None)
            }
        };

        // Parse the include fee flag
        let include_fee = params[1].get::<bool>().unwrap();

        // Simulate state transition
        let result = validator.calculate_fee(&tx, *include_fee).await;
        if result.is_err() {
            error!(
                target: "darkfid::rpc::tx_calculate_fee", "Failed to validate state transition: {}",
                result.err().unwrap()
            );
            return server_error(RpcError::TxGasCalculationFail, id, None)
        };

        JsonResponse::new(JsonValue::Number(result.unwrap() as f64), id).into()
    }
}
