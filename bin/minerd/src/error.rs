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

use darkfi::rpc::jsonrpc::{ErrorCode::ServerError, JsonError, JsonResult};

/// Custom RPC errors available for minerd.
/// Please sort them sensefully.
pub enum RpcError {
    // Parsing errors
    TargetParseError = -32101,
    BlockParseError = -32102,

    // Miner errors
    MiningFailed = -32201,
    StopFailed = -32202,
}

fn to_tuple(e: RpcError) -> (i32, String) {
    let msg = match e {
        // Parsing errors
        RpcError::TargetParseError => "Target parse error",
        RpcError::BlockParseError => "Block parse error",
        // Miner errors
        RpcError::MiningFailed => "Mining block failed",
        RpcError::StopFailed => "Failed to stop previous request",
    };

    (e as i32, msg.to_string())
}

pub fn server_error(e: RpcError, id: u16, msg: Option<&str>) -> JsonResult {
    let (code, default_msg) = to_tuple(e);

    if let Some(message) = msg {
        return JsonError::new(ServerError(code), Some(message.to_string()), id).into()
    }

    JsonError::new(ServerError(code), Some(default_msg), id).into()
}
