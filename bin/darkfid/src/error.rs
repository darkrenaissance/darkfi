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

    // Miner configuration errors
    MinerInvalidWalletConfig = -32301,
    MinerInvalidRecipient = -32302,
    MinerInvalidRecipientPrefix = -32303,
    MinerInvalidSpendHook = -32304,
    MinerInvalidUserData = -32305,

    // Stratum errors
    MinerMissingLogin = -32306,
    MinerInvalidLogin = -32307,
    MinerMissingPassword = -32308,
    MinerInvalidPassword = -32309,
    MinerMissingAgent = -32310,
    MinerInvalidAgent = -32311,
    MinerMissingAlgo = -32312,
    MinerInvalidAlgo = -32313,
    MinerRandomXNotSupported = -32314,
    MinerMissingClientId = -32315,
    MinerInvalidClientId = -32316,
    MinerUnknownClient = -32317,
    MinerMissingJobId = -32318,
    MinerInvalidJobId = -32319,
    MinerUnknownJob = -32320,
    MinerMissingNonce = -32321,
    MinerInvalidNonce = -32322,
    MinerMissingResult = -32323,
    MinerInvalidResult = -32324,

    // Merge mining errors
    MinerMissingAddress = -32325,
    MinerInvalidAddress = -32326,
    MinerMissingAuxHash = -32327,
    MinerInvalidAuxHash = -32328,
    MinerMissingHeight = -32329,
    MinerInvalidHeight = -32330,
    MinerMissingPrevId = -32331,
    MinerInvalidPrevId = -32332,
    MinerMissingAuxBlob = -32333,
    MinerInvalidAuxBlob = -32334,
    MinerMissingBlob = -32335,
    MinerInvalidBlob = -32336,
    MinerMissingMerkleProof = -32337,
    MinerInvalidMerkleProof = -32338,
    MinerMissingPath = -32339,
    MinerInvalidPath = -32340,
    MinerMissingSeedHash = -32341,
    MinerInvalidSeedHash = -32342,
    MinerMerkleProofConstructionFailed = -32343,
    MinerMoneroPowDataConstructionFailed = -32344,
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

        // Miner configuration errors
        RpcError::MinerInvalidWalletConfig => "Request wallet configuration is invalid",
        RpcError::MinerInvalidRecipient => "Request recipient wallet address is invalid",
        RpcError::MinerInvalidRecipientPrefix => {
            "Request recipient wallet address prefix is invalid"
        }
        RpcError::MinerInvalidSpendHook => "Request spend hook is invalid",
        RpcError::MinerInvalidUserData => "Request user data is invalid",

        // Stratum errors
        RpcError::MinerMissingLogin => "Request is missing the login",
        RpcError::MinerInvalidLogin => "Request login is invalid",
        RpcError::MinerMissingPassword => "Request is missing the password",
        RpcError::MinerInvalidPassword => "Request password is invalid",
        RpcError::MinerMissingAgent => "Request is missing the agent",
        RpcError::MinerInvalidAgent => "Request agent is invalid",
        RpcError::MinerMissingAlgo => "Request is missing the algo",
        RpcError::MinerInvalidAlgo => "Request algo is invalid",
        RpcError::MinerRandomXNotSupported => "Request doesn't support rx/0",
        RpcError::MinerMissingClientId => "Request is missing the client ID",
        RpcError::MinerInvalidClientId => "Request client ID is invalid",
        RpcError::MinerUnknownClient => "Request client is unknown",
        RpcError::MinerMissingJobId => "Request is missing the job ID",
        RpcError::MinerInvalidJobId => "Request job ID is invalid",
        RpcError::MinerUnknownJob => "Request job is unknown",
        RpcError::MinerMissingNonce => "Request is missing the nonce",
        RpcError::MinerInvalidNonce => "Request nonce is invalid",
        RpcError::MinerMissingResult => "Request is missing the result",
        RpcError::MinerInvalidResult => "Request nonce is result",

        // Merge mining errors
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
