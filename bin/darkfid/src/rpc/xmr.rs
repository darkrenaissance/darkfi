/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
};

use async_trait::async_trait;
use hex::FromHex;
use smol::lock::MutexGuard;
use tinyjson::JsonValue;
use tracing::{debug, error, info};

use darkfi::{
    blockchain::{
        header_store::PowData,
        monero::{
            fixed_array::FixedByteArray, merkle_proof::MerkleProof, monero_block_deserialize,
            MoneroPowData,
        },
        HeaderHash,
    },
    rpc::{
        jsonrpc::{
            ErrorCode, ErrorCode::InvalidParams, JsonError, JsonRequest, JsonResponse, JsonResult,
        },
        server::RequestHandler,
    },
    system::StoppableTaskPtr,
};
use darkfi_sdk::crypto::keypair::Network;

use crate::{
    error::{miner_status_response, server_error, RpcError},
    registry::model::MinerRewardsRecipientConfig,
    DarkfiNode,
};

// https://github.com/SChernykh/p2pool/blob/master/docs/MERGE_MINING.MD

/// HTTP JSON-RPC `RequestHandler` for p2pool/merge mining
pub struct MmRpcHandler;

#[async_trait]
#[rustfmt::skip]
impl RequestHandler<MmRpcHandler> for DarkfiNode {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        debug!(target: "darkfid::mm_rpc", "--> {}", req.stringify().unwrap());

        match req.method.as_str() {
            // ================================================
            // P2Pool methods requested for Monero Merge Mining
            // ================================================
            "merge_mining_get_chain_id" => self.xmr_merge_mining_get_chain_id(req.id, req.params).await,
            "merge_mining_get_aux_block" => self.xmr_merge_mining_get_aux_block(req.id, req.params).await,
            "merge_mining_submit_solution" => self.xmr_merge_mining_submit_solution(req.id, req.params).await,
            _ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'life0, HashSet<StoppableTaskPtr>> {
        self.registry.mm_rpc_connections.lock().await
    }
}

impl DarkfiNode {
    // RPCAPI:
    // Gets a unique ID that identifies this merge mined chain and
    // separates it from other chains.
    //
    // * `chain_id`: A unique 32-byte hash that identifies this merge
    //   mined chain.
    //
    // darkfid will send the hash:
    //  H(genesis_hash || network || hard_fork_height)
    //
    // --> {"jsonrpc": "2.0", "method": "merge_mining_get_chain_id", "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {"chain_id": "0f28c...7863"}, "id": 1}
    pub async fn xmr_merge_mining_get_chain_id(&self, id: u16, params: JsonValue) -> JsonResult {
        // Verify request params
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        // Grab genesis block to use as chain identifier
        let (_, genesis_hash) = match self.validator.blockchain.genesis() {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "darkfid::rpc::rpc_xmr::xmr_merge_mining_get_chain_id",
                    "[RPC-XMR] Error fetching genesis block hash: {e}"
                );
                return JsonError::new(ErrorCode::InternalError, None, id).into()
            }
        };

        // Generate the chain id
        let mut hasher = blake3::Hasher::new();
        hasher.update(genesis_hash.inner());
        match self.registry.network {
            Network::Mainnet => hasher.update("mainnet".as_bytes()),
            Network::Testnet => hasher.update("testnet".as_bytes()),
        };
        hasher.update(&0u32.to_le_bytes());
        let chain_id = hasher.finalize().to_string();

        let response = HashMap::from([("chain_id".to_string(), JsonValue::from(chain_id))]);
        JsonResponse::new(JsonValue::from(response), id).into()
    }

    // RPCAPI:
    // Gets a blob of data, the blocks hash and difficutly used for
    // merge mining.
    //
    // **Request:**
    // * `address` : A wallet address or its base-64 encoded mining configuration on the merge mined chain
    // * `aux_hash`: Merge mining job that is currently being polled
    // * `height`  : Monero height
    // * `prev_id` : Hash of the previous Monero block
    //
    // **Response:**
    // * `aux_blob`: A hex-encoded blob of empty data
    // * `aux_diff`: Mining difficulty (decimal number)
    // * `aux_hash`: A 32-byte hex-encoded hash of merge mined block
    //
    // --> {
    //       "jsonrpc": "2.0",
    //       "method": "merge_mining_get_aux_block",
    //       "params": {
    //         "address": "MERGE_MINED_CHAIN_ADDRESS",
    //         "aux_hash": "f6952d6eef555ddd87aca66e56b91530222d6e318414816f3ba7cf5bf694bf0f",
    //         "height": 3000000,
    //         "prev_id":"ad505b0be8a49b89273e307106fa42133cbd804456724c5e7635bd953215d92a"
    //       },
    //       "id": 1
    //     }
    // <-- {
    //       "jsonrpc":"2.0",
    //       "result": {
    //         "aux_blob": "fad344115...3151531",
    //         "aux_diff": 123456,
    //         "aux_hash":"f6952d6eef555ddd87aca66e56b91530222d6e318414816f3ba7cf5bf694bf0f"
    //       },
    //       "id": 1
    //     }
    pub async fn xmr_merge_mining_get_aux_block(&self, id: u16, params: JsonValue) -> JsonResult {
        // Check if node is synced before responding to p2pool
        if !*self.validator.synced.read().await {
            return JsonResponse::new(JsonValue::from(HashMap::new()), id).into()
        }

        // Parse request params
        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        // Parse aux_hash
        let Some(aux_hash) = params.get("aux_hash") else {
            return server_error(RpcError::MinerMissingAuxHash, id, None)
        };
        let Some(aux_hash) = aux_hash.get::<String>() else {
            return server_error(RpcError::MinerInvalidAuxHash, id, None)
        };
        if HeaderHash::from_str(aux_hash).is_err() {
            return server_error(RpcError::MinerInvalidAuxHash, id, None)
        };

        // Check if we already have this job
        if self.registry.mm_jobs.read().await.contains_key(&aux_hash.to_string()) {
            return JsonResponse::new(JsonValue::from(HashMap::new()), id).into()
        }

        // Parse address
        let Some(wallet) = params.get("address") else {
            return server_error(RpcError::MinerMissingAddress, id, None)
        };
        let Some(wallet) = wallet.get::<String>() else {
            return server_error(RpcError::MinerInvalidAddress, id, None)
        };
        let config =
            match MinerRewardsRecipientConfig::from_str(&self.registry.network, wallet).await {
                Ok(c) => c,
                Err(e) => return server_error(e, id, None),
            };

        // Parse height
        let Some(height) = params.get("height") else {
            return server_error(RpcError::MinerMissingHeight, id, None)
        };
        let Some(height) = height.get::<f64>() else {
            return server_error(RpcError::MinerInvalidHeight, id, None)
        };
        let height = *height as u64;

        // Parse prev_id
        let Some(prev_id) = params.get("prev_id") else {
            return server_error(RpcError::MinerMissingPrevId, id, None)
        };
        let Some(prev_id) = prev_id.get::<String>() else {
            return server_error(RpcError::MinerInvalidPrevId, id, None)
        };
        let Ok(prev_id) = hex::decode(prev_id) else {
            return server_error(RpcError::MinerInvalidPrevId, id, None)
        };
        let prev_id = monero::Hash::from_slice(&prev_id);

        // Register the new merge miner
        let (job_id, difficulty) =
            match self.registry.register_merge_miner(&self.validator, wallet, &config).await {
                Ok(p) => p,
                Err(e) => {
                    error!(
                        target: "darkfid::rpc::rpc_xmr::xmr_merge_mining_get_aux_block",
                        "[RPC-XMR] Failed to register merge miner: {e}",
                    );
                    return JsonResponse::new(JsonValue::from(HashMap::new()), id).into()
                }
            };

        // Now we have the new job, we ship it to RPC
        info!(
            target: "darkfid::rpc::rpc_xmr::xmr_merge_mining_get_aux_block",
            "[RPC-XMR] Created new merge mining job: aux_hash={job_id}, height={height}, prev_id={prev_id}"
        );
        let response = JsonValue::from(HashMap::from([
            ("aux_blob".to_string(), JsonValue::from(hex::encode(vec![]))),
            ("aux_diff".to_string(), JsonValue::from(difficulty)),
            ("aux_hash".to_string(), JsonValue::from(job_id)),
        ]));
        JsonResponse::new(response, id).into()
    }

    // RPCAPI:
    // Submits a PoW solution for the merge mined chain's block. Note that
    // when merge mining with Monero, the PoW solution is always a Monero
    // block template with merge mining data included into it.
    //
    // **Request:**
    // * `aux_blob`: Blob of data returned by `merge_mining_get_aux_block`
    // * `aux_hash`: A 32-byte hex-encoded hash of merge mined block
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
    // --> {
    //       "jsonrpc":"2.0",
    //       "method": "merge_mining_submit_solution",
    //       "params": {
    //         "aux_blob": "124125....35215136",
    //         "aux_hash": "f6952d6eef555ddd87aca66e56b91530222d6e318414816f3ba7cf5bf694bf0f",
    //         "blob": "...",
    //         "merkle_proof": ["hash1", "hash2", "hash3"],
    //         "path": 3,
    //         "seed_hash": "22c3d47c595ae888b5d7fc304235f92f8854644d4fad38c5680a5d4a81009fcd"
    //       },
    //       "id": 1
    //     }
    // <-- {"jsonrpc":"2.0", "result": {"status": "accepted"}, "id": 1}
    pub async fn xmr_merge_mining_submit_solution(&self, id: u16, params: JsonValue) -> JsonResult {
        // Check if node is synced before responding to p2pool
        if !*self.validator.synced.read().await {
            return miner_status_response(id, "rejected")
        }

        // Grab registry submissions lock
        let submit_lock = self.registry.submit_lock.write().await;

        // Parse request params
        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        // Parse aux_hash
        let Some(aux_hash) = params.get("aux_hash") else {
            return server_error(RpcError::MinerMissingAuxHash, id, None)
        };
        let Some(aux_hash) = aux_hash.get::<String>() else {
            return server_error(RpcError::MinerInvalidAuxHash, id, None)
        };
        if HeaderHash::from_str(aux_hash).is_err() {
            return server_error(RpcError::MinerInvalidAuxHash, id, None)
        }

        // If we don't know about this mm job, we can just abort here
        let mut mm_jobs = self.registry.mm_jobs.write().await;
        let Some(wallet) = mm_jobs.get(aux_hash) else {
            return miner_status_response(id, "rejected")
        };

        // If this job wallet template doesn't exist, we can just
        // abort here.
        let mut block_templates = self.registry.block_templates.write().await;
        let Some(block_template) = block_templates.get_mut(wallet) else {
            return miner_status_response(id, "rejected")
        };

        // If this template has been already submitted, reject this
        // submission.
        if block_template.submitted {
            return miner_status_response(id, "rejected")
        }

        // Parse aux_blob
        let Some(aux_blob) = params.get("aux_blob") else {
            return server_error(RpcError::MinerMissingAuxBlob, id, None)
        };
        let Some(aux_blob) = aux_blob.get::<String>() else {
            return server_error(RpcError::MinerInvalidAuxBlob, id, None)
        };
        let Ok(aux_blob) = hex::decode(aux_blob) else {
            return server_error(RpcError::MinerInvalidAuxBlob, id, None)
        };
        if !aux_blob.is_empty() {
            return server_error(RpcError::MinerInvalidAuxBlob, id, None)
        }

        // Parse blob
        let Some(blob) = params.get("blob") else {
            return server_error(RpcError::MinerMissingBlob, id, None)
        };
        let Some(blob) = blob.get::<String>() else {
            return server_error(RpcError::MinerInvalidBlob, id, None)
        };
        let Ok(block) = monero_block_deserialize(blob) else {
            return server_error(RpcError::MinerInvalidBlob, id, None)
        };

        // Parse merkle_proof
        let Some(merkle_proof_j) = params.get("merkle_proof") else {
            return server_error(RpcError::MinerMissingMerkleProof, id, None)
        };
        let Some(merkle_proof_j) = merkle_proof_j.get::<Vec<JsonValue>>() else {
            return server_error(RpcError::MinerInvalidMerkleProof, id, None)
        };
        let mut merkle_proof: Vec<monero::Hash> = Vec::with_capacity(merkle_proof_j.len());
        for hash in merkle_proof_j.iter() {
            match hash.get::<String>() {
                Some(v) => {
                    let Ok(val) = monero::Hash::from_hex(v) else {
                        return server_error(RpcError::MinerInvalidMerkleProof, id, None)
                    };

                    merkle_proof.push(val);
                }
                None => return server_error(RpcError::MinerInvalidMerkleProof, id, None),
            }
        }

        // Parse path
        let Some(path) = params.get("path") else {
            return server_error(RpcError::MinerMissingPath, id, None)
        };
        let Some(path) = path.get::<f64>() else {
            return server_error(RpcError::MinerInvalidPath, id, None)
        };
        let path = *path as u32;

        // Parse seed_hash
        let Some(seed_hash) = params.get("seed_hash") else {
            return server_error(RpcError::MinerMissingSeedHash, id, None)
        };
        let Some(seed_hash) = seed_hash.get::<String>() else {
            return server_error(RpcError::MinerInvalidSeedHash, id, None)
        };
        let Ok(seed_hash) = monero::Hash::from_hex(seed_hash) else {
            return server_error(RpcError::MinerInvalidSeedHash, id, None)
        };
        let Ok(seed_hash) = FixedByteArray::from_bytes(seed_hash.as_bytes()) else {
            return server_error(RpcError::MinerInvalidSeedHash, id, None)
        };

        info!(
            target: "darkfid::rpc::rpc_xmr::xmr_merge_mining_submit_solution",
            "[RPC-XMR] Got solution submission: aux_hash={aux_hash}",
        );

        // Construct the MoneroPowData
        let Some(merkle_proof) = MerkleProof::try_construct(merkle_proof, path) else {
            return server_error(RpcError::MinerMerkleProofConstructionFailed, id, None)
        };
        let monero_pow_data = match MoneroPowData::new(block, seed_hash, merkle_proof) {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "darkfid::rpc::rpc_xmr::xmr_merge_mining_submit_solution",
                    "[RPC-XMR] Failed constructing MoneroPowData: {e}",
                );
                return server_error(RpcError::MinerMoneroPowDataConstructionFailed, id, None)
            }
        };

        // Append MoneroPowData to the DarkFi block and sign it
        let mut block = block_template.block.clone();
        block.header.pow_data = PowData::Monero(monero_pow_data);
        block.sign(&block_template.secret);

        // Submit the new block through the registry
        if let Err(e) =
            self.registry.submit(&self.validator, &self.subscribers, &self.p2p_handler, block).await
        {
            error!(
                target: "darkfid::rpc::rpc_xmr::xmr_merge_mining_submit_solution",
                "[RPC-XMR] Error submitting new block: {e}",
            );

            // Try to refresh the jobs before returning error
            let mut jobs = self.registry.jobs.write().await;
            if let Err(e) = self
                .registry
                .refresh_jobs(&mut block_templates, &mut jobs, &mut mm_jobs, &self.validator)
                .await
            {
                error!(
                    target: "darkfid::rpc::rpc_xmr::xmr_merge_mining_submit_solution",
                    "[RPC-XMR] Error refreshing registry jobs: {e}",
                );
            }

            // Release all locks
            drop(block_templates);
            drop(jobs);
            drop(mm_jobs);
            drop(submit_lock);

            return miner_status_response(id, "rejected")
        }

        // Mark block as submitted
        block_template.submitted = true;

        // Release all locks
        drop(block_templates);
        drop(mm_jobs);
        drop(submit_lock);

        miner_status_response(id, "accepted")
    }
}
