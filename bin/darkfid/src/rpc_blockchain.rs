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

use darkfi_sdk::crypto::MerkleNode;
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

        // TODO: Return block as JSON
        debug!("{:#?}", blocks[0]);
        JsonResponse::new(json!(true), id).into()
    }

    // RPCAPI:
    // Queries the blockchain database for all available merkle roots.
    //
    // --> {"jsonrpc": "2.0", "method": "blockchain.merkle_roots", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [..., ..., ...], "id": 1}
    pub async fn blockchain_merkle_roots(&self, id: Value, params: &[Value]) -> JsonResult {
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let validator_state = self.validator_state.read().await;

        let roots: Vec<MerkleNode> = match validator_state.blockchain.merkle_roots.get_all() {
            Ok(v) => {
                drop(validator_state);
                v
            }
            Err(e) => {
                error!("[RPC] blockchain.merkle_roots: Failed fetching merkle roots from rootstore: {}", e);
                return JsonError::new(InternalError, None, id).into()
            }
        };

        let roots: Vec<String> = roots.iter().map(|x| x.to_string()).collect();

        JsonResponse::new(json!(roots), id).into()
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
}
