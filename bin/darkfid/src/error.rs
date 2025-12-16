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
    ContractStateNotFound = -32201,
    ContractStateKeyNotFound = -32202,

    // Miner errors
    MinerMissingHeader = -32300,
    MinerInvalidHeader = -32301,
    MinerMissingRecipient = -32302,
    MinerInvalidRecipient = -32303,
    MinerInvalidRecipientPrefix = -32304,
    MinerInvalidSpendHook = -32305,
    MinerInvalidUserData = -32306,
    MinerMissingNonce = -32307,
    MinerInvalidNonce = -32308,
    MinerMissingAddress = -32309,
    MinerInvalidAddress = -32310,
    MinerMissingAuxHash = -32311,
    MinerInvalidAuxHash = -32312,
    MinerMissingHeight = -32313,
    MinerInvalidHeight = -32314,
    MinerMissingPrevId = -32315,
    MinerInvalidPrevId = -32316,
    MinerMissingAuxBlob = -32317,
    MinerInvalidAuxBlob = -32318,
    MinerMissingBlob = -32319,
    MinerInvalidBlob = -32320,
    MinerMissingMerkleProof = -32321,
    MinerInvalidMerkleProof = -32322,
    MinerMissingPath = -32323,
    MinerInvalidPath = -32324,
    MinerMissingSeedHash = -32325,
    MinerInvalidSeedHash = -32326,
    MinerMerkleProofConstructionFailed = -32327,
    MinerMoneroPowDataConstructionFailed = -32328,
    MinerUnknownJob = -32329,
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
        RpcError::ContractStateNotFound => "Records not found for given contract state",
        RpcError::ContractStateKeyNotFound => "Value not found for given contract state key",
        // Miner errors
        RpcError::MinerMissingHeader => "Request is missing the Header hash",
        RpcError::MinerInvalidHeader => "Request Header hash is invalid",
        RpcError::MinerMissingRecipient => "Request is missing the recipient wallet address",
        RpcError::MinerInvalidRecipient => "Request recipient wallet address is invalid",
        RpcError::MinerInvalidRecipientPrefix => {
            "Request recipient wallet address prefix is invalid"
        }
        RpcError::MinerInvalidSpendHook => "Request spend hook is invalid",
        RpcError::MinerInvalidUserData => "Request user data is invalid",
        RpcError::MinerMissingNonce => "Request is missing the Header nonce",
        RpcError::MinerInvalidNonce => "Request Header nonce is invalid",
        RpcError::MinerMissingAddress => {
            "Request is missing the recipient wallet address configuration"
        }
        RpcError::MinerInvalidAddress => {
            "Request recipient wallet address configuration is invalid"
        }
        RpcError::MinerMissingAuxHash => "Request is missing the merge mining job (aux_hash)",
        RpcError::MinerInvalidAuxHash => "Request merge mining job (aux_hash) is invalid",
        RpcError::MinerMissingHeight => "Request is missing the Monero height",
        RpcError::MinerInvalidHeight => "Request Monero height is invalid",
        RpcError::MinerMissingPrevId => "Request is missing the hash of the previous Monero block",
        RpcError::MinerInvalidPrevId => "Request hash of the previous Monero block is invalid",
        RpcError::MinerMissingAuxBlob => "Request is missing the merge mining blob",
        RpcError::MinerInvalidAuxBlob => "Request merge mining bob is invalid",
        RpcError::MinerMissingBlob => "Request is missing the Monero block template",
        RpcError::MinerInvalidBlob => "Request Monero block template is invalid",
        RpcError::MinerMissingMerkleProof => "Request is missing the Merkle proof",
        RpcError::MinerInvalidMerkleProof => "Request Merkle proof is invalid",
        RpcError::MinerMissingPath => "Request is missing the Merkle proof path",
        RpcError::MinerInvalidPath => "Request Merkle proof path is invalid",
        RpcError::MinerMissingSeedHash => "Request is missing the RandomX seed key",
        RpcError::MinerInvalidSeedHash => "Request RandomX seed key is invalid",
        RpcError::MinerMerkleProofConstructionFailed => {
            "failed constructing aux chain Merkle proof"
        }
        RpcError::MinerMoneroPowDataConstructionFailed => "Failed constructing Monero PoW data",
        RpcError::MinerUnknownJob => "Request job is unknown",
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
