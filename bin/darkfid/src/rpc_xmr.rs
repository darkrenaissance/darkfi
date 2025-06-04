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

use std::collections::HashMap;

use darkfi::rpc::jsonrpc::{ErrorCode, JsonError, JsonResponse, JsonResult};
use tinyjson::JsonValue;
use tracing::error;

use crate::DarkfiNode;

// https://github.com/SChernykh/p2pool/blob/master/docs/MERGE_MINING.MD

impl DarkfiNode {
    // RPCAPI:
    // Gets a unique ID that identifies this merge mined chain and
    // separates it from other chains.
    //
    // * `chain_id`: A unique 32-byte hex-encoded value that identifies
    //   this merge mined chain.
    //
    // darkfid will send the hash of the genesis block header.
    //
    // --> {"jsonrpc":"2.0", "method": "merge_mining_get_chain_id", "id": 1}
    // <-- {"jsonrpc":"2.0", "result": {"chain_id": "0f28c...7863"}, "id": 1}
    pub async fn xmr_merge_mining_get_chain_id(&self, id: u16, _params: JsonValue) -> JsonResult {
        let (_, genesis_hash) = match self.validator.blockchain.genesis() {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "darkfid::rpc::xmr_merge_mining_get_chain_id",
                    "[RPC] Error fetching genesis block hash: {e}"
                );
                return JsonError::new(ErrorCode::InternalError, None, id).into()
            }
        };

        let genesis_hex = genesis_hash.to_string();
        assert!(genesis_hex.len() == 32);

        let resp_obj = HashMap::from([("chain_id".to_string(), genesis_hex.into())]);
        JsonResponse::new(resp_obj.into(), id).into()
    }

    // RPCAPI:
    // Gets a blob of data (usually a new block for the merge mined chain)
    // and its hash to be used for merge mining.
    //
    // **Request:**
    // * `address` - A wallet address on the merge mined chain
    // * `aux_hash` - Merge mining job that is currently being used
    // * `height` - Monero height
    // * `prev_id` - Hash of the previous Monero block
    //
    // **Response:**
    // * `aux_blob` - A hex-encoded blob of data. Merge mined chain defines the
    //   contents of this blob. It's opaque to p2pool and will not be changed by it
    // * `aux_diff` - Mining difficulty (decimal number)
    // * `aux_hash` - A 32-byte hex-encoded hash of the `aux_blob`. Merge mined chain
    //   defines how exactly this hash is calculated. It's opaque to p2pool.
    //
    // --> {"jsonrpc":"2.0", "method": "merge_mining_get_aux_block", "params": {"address": "MERGE_MINED_CHAIN_ADDRESS", "aux_hash": "f6952d6eef555ddd87aca66e56b91530222d6e318414816f3ba7cf5bf694bf0f", "height": 3000000, "prev_id":"ad505b0be8a49b89273e307106fa42133cbd804456724c5e7635bd953215d92a"}, "id": 1}
    // <-- {"jsonrpc":"2.0", "result": {"aux_blob": "4c6f72656d20697073756d", "aux_diff": 123456, "aux_hash":"f6952d6eef555ddd87aca66e56b91530222d6e318414816f3ba7cf5bf694bf0f"}, "id": 1}
    pub async fn xmr_merge_mining_get_aux_block(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }

    // RPCAPI:
    // Submits a PoW solution for the merge mined chain's block. Note that
    // when merge mining with Monero, the PoW solution is always a Monero
    // block template with merge mining data included into it.
    //
    // **Request:**
    // * `aux_blob`: Blob of data returned by `merge_mining_get_aux_block`
    // * `aux_hash`: A 32-byte hex-encoded hash of the `aux_blob`, same as above.
    // * `blob`: Monero block template that has enough PoW to satisfy the difficulty
    //   returned by `merge_mining_get_aux_block`. It must also have a merge mining
    //   tag in `tx_extra` of the coinbase transaction.
    // * `merkle_proof`: A proof that `aux_hash` was included when calculating the
    //   Merkle root hash from the merge mining tag
    // * `path`: A path bitmap (32-bit unsigned integer) that complements `merkle_proof`
    // * `seed_hash`: A 32-byte hex-encoded key that is used to initialize the
    //   RandomX dataset
    //
    // **Response:**
    // * `status`: Block submit status
    //
    // --> {"jsonrpc":"2.0", "method": "merge_mining_submit_solution", "params": {"aux_blob": "4c6f72656d20697073756d", "aux_hash": "f6952d6eef555ddd87aca66e56b91530222d6e318414816f3ba7cf5bf694bf0f", "blob": "...", "merkle_proof": ["hash1", "hash2", "hash3"], "path": 3, "seed_hash": "22c3d47c595ae888b5d7fc304235f92f8854644d4fad38c5680a5d4a81009fcd"}, "id": 1}
    // <-- {"jsonrpc":"2.0", "result": {"status": "accepted"}, "id": 1}
    pub async fn xmr_merge_mining_submit_solution(&self, id: u16, params: JsonValue) -> JsonResult {
        todo!()
    }
}
