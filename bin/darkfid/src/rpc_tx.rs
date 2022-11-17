/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use darkfi_sdk::crypto::{Address, PublicKey, TokenId};
use darkfi_serial::{deserialize, serialize};
use log::{error, warn};
use serde_json::{json, Value};

use darkfi::{
    rpc::jsonrpc::{ErrorCode::InvalidParams, JsonError, JsonResponse, JsonResult},
    tx::Transaction,
};

use super::Darkfid;
use crate::{server_error, RpcError};

impl Darkfid {
    // RPCAPI:
    // Transfer a given amount of some token to the given address.
    // Returns a transaction ID upon success.
    //
    // * `dest_addr` -> Recipient's DarkFi address
    // * `token_id` -> ID of the token to send
    // * `12345` -> Amount in `u64` of the funds to send
    //
    // --> {"jsonrpc": "2.0", "method": "tx.transfer", "params": ["dest_addr", "token_id", 12345], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "txID...", "id": 1}
    pub async fn tx_transfer(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 3 ||
            !params[0].is_string() ||
            !params[1].is_string() ||
            !params[2].is_u64()
        {
            return JsonError::new(InvalidParams, None, id).into()
        }

        if !(*self.synced.lock().await) {
            error!("[RPC] tx.transfer: Blockchain is not synced");
            return server_error(RpcError::NotSynced, id, None)
        }

        let address = params[0].as_str().unwrap();
        let token = params[1].as_str().unwrap();
        let amount = params[2].as_u64().unwrap();

        let address = match Address::from_str(address) {
            Ok(v) => v,
            Err(e) => {
                error!("[RPC] tx.transfer: Failed parsing address from string: {}", e);
                return server_error(RpcError::InvalidAddressParam, id, None)
            }
        };

        let pubkey = match PublicKey::try_from(address) {
            Ok(v) => v,
            Err(e) => {
                error!("[RPC] tx.transfer: Failed parsing PublicKey from Address: {}", e);
                return server_error(RpcError::ParseError, id, None)
            }
        };

        let token_id = match TokenId::try_from(token) {
            Ok(v) => v,
            Err(e) => {
                error!("[RPC] tx.transfer: Failed parsing Token ID from string: {}", e);
                return server_error(RpcError::ParseError, id, None)
            }
        };

        let tx = match self
            .client
            .build_transaction(
                pubkey,
                amount,
                token_id,
                false,
                self.validator_state.read().await.state_machine.clone(),
            )
            .await
        {
            Ok(v) => v,
            Err(e) => {
                error!("tx.transfer: Failed building transaction: {}", e);
                return server_error(RpcError::TxBuildFail, id, None)
            }
        };

        if let Some(sync_p2p) = &self.sync_p2p {
            if let Err(e) = sync_p2p.broadcast(tx.clone()).await {
                error!("[RPC] tx.transfer: Failed broadcasting transaction: {}", e);
                return server_error(RpcError::TxBroadcastFail, id, None)
            }
        } else {
            warn!("[RPC] tx.transfer: No sync P2P network, not broadcasting transaction.");
            return server_error(RpcError::TxBroadcastFail, id, None)
        }

        let tx_hash = blake3::hash(&serialize(&tx)).to_hex().as_str().to_string();
        JsonResponse::new(json!(tx_hash), id).into()
    }

    // RPCAPI:
    // Simulate a network state transition with the given transaction.
    // Returns `true` if the transaction is valid, otherwise, a corresponding
    // error.
    //
    // --> {"jsonrpc": "2.0", "method": "tx.simulate", "params": ["base58encodedTX"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn tx_simulate(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        if !(*self.synced.lock().await) {
            error!("[RPC] tx.simulate: Blockchain is not synced");
            return server_error(RpcError::NotSynced, id, None)
        }

        // Try to deserialize the transaction
        let tx_bytes = match bs58::decode(params[0].as_str().unwrap().trim()).into_vec() {
            Ok(v) => v,
            Err(e) => {
                error!("[RPC] tx.simulate: Failed decoding base58 transaction: {}", e);
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
        if let Err(e) = self.simulate_transaction(&tx).await {
            error!("[RPC] tx.broadcast: Failed to validate state transition: {}", e);
            return server_error(RpcError::TxSimulationFail, id, None)
        }

        JsonResponse::new(json!(true), id).into()
    }

    // RPCAPI:
    // Broadcast a given transaction to the P2P network.
    // The function will first simulate the state transition in order to see
    // if the transaction is actually valid, and in turn it will return an
    // error if this is the case. Otherwise, a transaction ID will be returned.
    //
    // --> {"jsonrpc": "2.0", "method": "tx.broadcast", "params": ["base58encodedTX"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "txID...", "id": 1}
    pub async fn tx_broadcast(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        if !(*self.synced.lock().await) {
            error!("[RPC] tx.transfer: Blockchain is not synced");
            return server_error(RpcError::NotSynced, id, None)
        }

        // Try to deserialize the transaction
        let tx_bytes = match bs58::decode(params[0].as_str().unwrap().trim()).into_vec() {
            Ok(v) => v,
            Err(e) => {
                error!("[RPC] tx.broadcast: Failed decoding base58 transaction: {}", e);
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

        // Simulate state transition
        if let Err(e) = self.simulate_transaction(&tx).await {
            error!("[RPC] tx.broadcast: Failed to validate state transition: {}", e);
            return server_error(RpcError::TxSimulationFail, id, None)
        }

        // TODO: Should we apply the state transition locally before broadcasting it?
        if let Some(sync_p2p) = &self.sync_p2p {
            if let Err(e) = sync_p2p.broadcast(tx.clone()).await {
                error!("[RPC] tx.broadcast: Failed broadcasting transaction: {}", e);
                return server_error(RpcError::TxBroadcastFail, id, None)
            }

            // TODO: Mark coin as spent in the wallet
        } else {
            warn!("[RPC] tx.broadcast: No sync P2P network, not broadcasting transaction.");
            return server_error(RpcError::TxBroadcastFail, id, None)
        }

        let tx_hash = blake3::hash(&serialize(&tx)).to_hex().as_str().to_string();
        JsonResponse::new(json!(tx_hash), id).into()
    }
}
