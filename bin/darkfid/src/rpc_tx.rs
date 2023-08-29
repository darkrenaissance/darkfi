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

use darkfi_serial::deserialize;
use log::{error, warn};
use tinyjson::JsonValue;

use darkfi::{
    rpc::jsonrpc::{ErrorCode::InvalidParams, JsonError, JsonResponse, JsonResult},
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
    // --> {"jsonrpc": "2.0", "method": "tx.simulate", "params": ["base58encodedTX"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn tx_simulate(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        if !(*self.synced.lock().await) {
            error!("[RPC] tx.simulate: Blockchain is not synced");
            return server_error(RpcError::NotSynced, id, None)
        }

        // Try to deserialize the transaction
        let tx_enc = params[0].get::<String>().unwrap();
        let tx_bytes = match base64::decode(tx_enc.trim()) {
            Some(v) => v,
            None => {
                error!("[RPC] tx.simulate: Failed decoding base64 transaction");
                return server_error(RpcError::ParseError, id, None)
            }
        };

        let tx: Transaction = match deserialize(&tx_bytes) {
            Ok(v) => v,
            Err(e) => {
                error!("[RPC] tx.simulate: Failed deserializing bytes into Transaction: {}", e);
                return server_error(RpcError::ParseError, id, None)
            }
        };

        // Simulate state transition
        let lock = self.validator_state.read().await;
        let current_slot = lock.consensus.time_keeper.current_slot();
        match lock.verify_transactions(&[tx], current_slot, false).await {
            Ok(erroneous_txs) => {
                if !erroneous_txs.is_empty() {
                    error!("[RPC] tx.simulate: invalid transaction provided");
                    return server_error(RpcError::TxSimulationFail, id, None)
                }
            }
            Err(e) => {
                error!("[RPC] tx.simulate: Failed to validate state transition: {}", e);
                return server_error(RpcError::TxSimulationFail, id, None)
            }
        };

        JsonResponse::new(JsonValue::Boolean(true), id).into()
    }

    // RPCAPI:
    // Broadcast a given transaction to the P2P network.
    // The function will first simulate the state transition in order to see
    // if the transaction is actually valid, and in turn it will return an
    // error if this is the case. Otherwise, a transaction ID will be returned.
    //
    // --> {"jsonrpc": "2.0", "method": "tx.broadcast", "params": ["base58encodedTX"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "txID...", "id": 1}
    pub async fn tx_broadcast(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = params.get::<Vec<JsonValue>>().unwrap();
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        if !(*self.synced.lock().await) {
            error!("[RPC] tx.transfer: Blockchain is not synced");
            return server_error(RpcError::NotSynced, id, None)
        }

        // Try to deserialize the transaction
        let tx_enc = params[0].get::<String>().unwrap();
        let tx_bytes = match base64::decode(tx_enc.trim()) {
            Some(v) => v,
            None => {
                error!("[RPC] tx.broadcast: Failed decoding base64 transaction");
                return server_error(RpcError::ParseError, id, None)
            }
        };

        let tx: Transaction = match deserialize(&tx_bytes) {
            Ok(v) => v,
            Err(e) => {
                error!("[RPC] tx.broadcast: Failed deserializing bytes into Transaction: {}", e);
                return server_error(RpcError::ParseError, id, None)
            }
        };

        if self.consensus_p2p.is_some() {
            // Consider we're participating in consensus here?
            // The append_tx function performs a state transition check.
            if !self.validator_state.write().await.append_tx(tx.clone()).await {
                error!("[RPC] tx.broadcast: Failed to append transaction to mempool");
                return server_error(RpcError::TxBroadcastFail, id, None)
            }
        } else {
            // We'll perform the state transition check here.
            let lock = self.validator_state.read().await;
            let current_slot = lock.consensus.time_keeper.current_slot();
            match lock.verify_transactions(&[tx.clone()], current_slot, false).await {
                Ok(erroneous_txs) => {
                    if !erroneous_txs.is_empty() {
                        error!("[RPC] tx.broadcast: invalid transaction provided");
                        return server_error(RpcError::TxSimulationFail, id, None)
                    }
                }
                Err(e) => {
                    error!("[RPC] tx.broadcast: Failed to validate state transition: {}", e);
                    return server_error(RpcError::TxSimulationFail, id, None)
                }
            };
        }

        if let Some(sync_p2p) = &self.sync_p2p {
            sync_p2p.broadcast(&tx).await;
            if sync_p2p.channels().lock().await.is_empty() {
                error!("[RPC] tx.broadcast: Failed broadcasting tx, no connected channels");
                return server_error(RpcError::TxBroadcastFail, id, None)
            }
        } else {
            warn!("[RPC] tx.broadcast: No sync P2P network, not broadcasting transaction.");
            return server_error(RpcError::TxBroadcastFail, id, None)
        }

        let tx_hash = tx.hash().unwrap().to_string();
        JsonResponse::new(JsonValue::String(tx_hash), id).into()
    }
}
