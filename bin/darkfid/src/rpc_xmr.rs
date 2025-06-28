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
use log::error;
use tinyjson::JsonValue;

use crate::DarkfiNode;

impl DarkfiNode {
    // RPCAPI:
    // Gets a unique ID that identifies this merge mined chain and
    // separates it from other chains.
    //
    // `chain_id`: A unique 32-byte hex-encoded value that identifies
    //             this merge mined chain.
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

        let resp_obj = HashMap::from([("chain_id".to_string(), genesis_hash.to_string().into())]);
        JsonResponse::new(resp_obj.into(), id).into()
    }
}
