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

/// Custom RPC errors available for darkfid.
/// Please sort them sensefully.
pub enum RpcError {
    // Transaction-related errors
    TxSimulationFail = -32110,
    TxGasCalculationFail = -32111,

    // State-related errors,
    NotSynced = -32120,
    UnknownBlockHeight = -32121,

    // Parsing errors
    ParseError = -32190,

    // Contract-related errors
    ContractZkasDbNotFound = -32200,

    // Misc errors
    PingFailed = -32300,
}

fn to_tuple(e: RpcError) -> (i32, String) {
    let msg = match e {
        // Transaction-related errors
        RpcError::TxSimulationFail => "Failed simulating transaction state change",
        RpcError::TxGasCalculationFail => "Failed to calculate transaction's gas",
        // State-related errors
        RpcError::NotSynced => "Blockchain is not synced",
        RpcError::UnknownBlockHeight => "Did not find block height",
        // Parsing errors
        RpcError::ParseError => "Parse error",
        // Contract-related errors
        RpcError::ContractZkasDbNotFound => "zkas database not found for given contract",
        // Misc errors
        RpcError::PingFailed => "Miner daemon ping error",
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
