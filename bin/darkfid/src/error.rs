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

use darkfi::rpc::jsonrpc::{ErrorCode::ServerError, JsonError, JsonResult};

/// Custom RPC errors available for darkfid.
/// Please sort them sensefully.
pub enum RpcError {
    /*
    // Wallet/Key-related errors
    NoRowsFoundInWallet = -32101,
    Keygen = -32101,
    KeypairFetch = -32102,
    KeypairNotFound = -32103,
    InvalidKeypair = -32104,
    InvalidAddressParam = -32105,
    DecryptionFailed = -32106,
    */
    // Transaction-related errors
    TxSimulationFail = -32110,
    TxBroadcastFail = -32111,

    // State-related errors,
    NotSynced = -32120,
    UnknownSlot = -32121,

    // Parsing errors
    ParseError = -32190,

    // Contract-related errors
    ContractZkasDbNotFound = -32200,
}

fn to_tuple(e: RpcError) -> (i32, String) {
    let msg = match e {
        /*
        // Wallet/Key-related errors
        RpcError::NoRowsFoundInWallet => "No queried rows found in wallet",
        RpcError::Keygen => "Failed generating keypair",
        RpcError::KeypairFetch => "Failed fetching keypairs from wallet",
        RpcError::KeypairNotFound => "Keypair not found",
        RpcError::InvalidKeypair => "Invalid keypair",
        RpcError::InvalidAddressParam => "Invalid address parameter",
        RpcError::DecryptionFailed => "Decryption failed",
        */
        // Transaction-related errors
        RpcError::TxSimulationFail => "Failed simulating transaction state change",
        RpcError::TxBroadcastFail => "Failed broadcasting transaction",
        // State-related errors
        RpcError::NotSynced => "Blockchain is not synced",
        RpcError::UnknownSlot => "Did not find slot",
        // Parsing errors
        RpcError::ParseError => "Parse error",
        // Contract-related errors
        RpcError::ContractZkasDbNotFound => "zkas database not found for given contract",
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
