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

use std::{collections::HashMap, str::FromStr};

use darkfi::{
    blockchain::{
        header_store::PowData,
        monero::{
            fixed_array::FixedByteArray, merkle_proof::MerkleProof, monero_block_deserialize,
            MoneroPowData,
        },
        HeaderHash,
    },
    rpc::jsonrpc::{ErrorCode, ErrorCode::InvalidParams, JsonError, JsonResponse, JsonResult},
    util::encoding::base64,
    validator::consensus::Proposal,
};
use darkfi_sdk::crypto::PublicKey;
use darkfi_serial::serialize_async;
use hex::FromHex;
use tinyjson::JsonValue;
use tracing::{error, info};

use crate::{
    proto::ProposalMessage,
    server_error,
    task::miner::{generate_next_block, MinerRewardsRecipientConfig},
    DarkfiNode, RpcError,
};

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
    pub async fn xmr_merge_mining_get_chain_id(&self, id: u16, params: JsonValue) -> JsonResult {
        // Check request doesn't contain params
        if !params.get::<Vec<JsonValue>>().unwrap().is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        // Grab genesis block to use as chain identifier
        let (_, genesis_hash) = match self.validator.blockchain.genesis() {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "darkfid::rpc::xmr_merge_mining_get_chain_id",
                    "[RPC-XMR] Error fetching genesis block hash: {e}"
                );
                return JsonError::new(ErrorCode::InternalError, None, id).into()
            }
        };

        // TODO: XXX: This should also have more specialized identifiers.
        // e.g. chain_id = H(genesis || aux_nonce || checkpoint_height)

        let response =
            HashMap::from([("chain_id".to_string(), JsonValue::from(genesis_hash.to_string()))]);
        JsonResponse::new(JsonValue::from(response), id).into()
    }

    // RPCAPI:
    // Gets a blob of data, the blocks hash and difficutly used for
    // merge mining.
    //
    // **Request:**
    // * `address` : A wallet address on the merge mined chain
    // * `aux_hash`: Merge mining job that is currently being polled
    // * `height`  : Monero height
    // * `prev_id` : Hash of the previous Monero block
    //
    // **Response:**
    // * `aux_blob`: The hex-encoded wallet address blob
    // * `aux_diff`: Mining difficulty (decimal number)
    // * `aux_hash`: A 32-byte hex-encoded hash of merge mined block
    //
    // --> {"jsonrpc":"2.0", "method": "merge_mining_get_aux_block", "params": {"address": "MERGE_MINED_CHAIN_ADDRESS", "aux_hash": "f6952d6eef555ddd87aca66e56b91530222d6e318414816f3ba7cf5bf694bf0f", "height": 3000000, "prev_id":"ad505b0be8a49b89273e307106fa42133cbd804456724c5e7635bd953215d92a"}, "id": 1}
    // <-- {"jsonrpc":"2.0", "result": {"aux_blob": "4c6f72656d20697073756d", "aux_diff": 123456, "aux_hash":"f6952d6eef555ddd87aca66e56b91530222d6e318414816f3ba7cf5bf694bf0f"}, "id": 1}
    pub async fn xmr_merge_mining_get_aux_block(&self, id: u16, params: JsonValue) -> JsonResult {
        // Check if node is synced before responding to p2pool
        if !*self.validator.synced.read().await {
            return server_error(RpcError::NotSynced, id, None)
        }

        // Parse request params
        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        // Parse address
        let Some(address) = params.get("address") else {
            return JsonError::new(InvalidParams, Some("missing address".to_string()), id).into()
        };
        let Some(address) = address.get::<String>() else {
            return JsonError::new(InvalidParams, Some("invalid address format".to_string()), id)
                .into()
        };
        let Ok(address) = PublicKey::from_str(address) else {
            return JsonError::new(InvalidParams, Some("invalid address format".to_string()), id)
                .into()
        };

        // Parse aux_hash
        let Some(aux_hash) = params.get("aux_hash") else {
            return JsonError::new(InvalidParams, Some("missing aux_hash".to_string()), id).into()
        };
        let Some(aux_hash) = aux_hash.get::<String>() else {
            return JsonError::new(InvalidParams, Some("invalid aux_hash format".to_string()), id)
                .into()
        };
        let Ok(aux_hash) = HeaderHash::from_str(aux_hash) else {
            return JsonError::new(InvalidParams, Some("invalid aux_hash format".to_string()), id)
                .into()
        };

        // Parse height
        let Some(height) = params.get("height") else {
            return JsonError::new(InvalidParams, Some("missing height".to_string()), id).into()
        };
        let Some(height) = height.get::<f64>() else {
            return JsonError::new(InvalidParams, Some("invalid height format".to_string()), id)
                .into()
        };
        let height = *height as u64;

        // Parse prev_id
        let Some(prev_id) = params.get("prev_id") else {
            return JsonError::new(InvalidParams, Some("missing prev_id".to_string()), id).into()
        };
        let Some(prev_id) = prev_id.get::<String>() else {
            return JsonError::new(InvalidParams, Some("invalid prev_id format".to_string()), id)
                .into()
        };
        let Ok(prev_id) = hex::decode(prev_id) else {
            return JsonError::new(InvalidParams, Some("invalid prev_id format".to_string()), id)
                .into()
        };
        let prev_id = monero::Hash::from_slice(&prev_id);

        // Now that method params format is correct, we can check if we
        // already have a mining job for this wallet. If we already
        // have it, we check if the fork it extends is still the best
        // one. If both checks pass, we can just return an empty
        // response if the request `aux_hash` matches the job one,
        // otherwise return the job block template hash. In case the
        // best fork has changed, we drop this job and generate a
        // new one. If we don't know this wallet, we create a new job.
        // We'll also obtain a lock here to avoid getting polled
        // multiple times and potentially missing a job. The lock is
        // released when this function exits.
        let mut mm_blocktemplates = self.mm_blocktemplates.lock().await;
        let mut extended_fork = match self.best_current_fork().await {
            Ok(f) => f,
            Err(e) => {
                error!(
                    target: "darkfid::rpc_xmr::xmr_merge_mining_get_aux_block",
                    "[RPC-XMR] Finding best fork index failed: {e}",
                );
                return JsonError::new(ErrorCode::InternalError, None, id).into()
            }
        };
        let address_bytes = address.to_bytes();
        if let Some((block, difficulty, _)) = mm_blocktemplates.get(&address_bytes) {
            let last_proposal = match extended_fork.last_proposal() {
                Ok(p) => p,
                Err(e) => {
                    error!(
                        target: "darkfid::rpc_xmr::xmr_merge_mining_get_aux_block",
                        "[RPC-XMR] Retrieving best fork last proposal failed: {e}",
                    );
                    return JsonError::new(ErrorCode::InternalError, None, id).into()
                }
            };
            if last_proposal.hash == block.header.previous {
                let blockhash = block.header.template_hash();
                return if blockhash != aux_hash {
                    JsonResponse::new(
                        JsonValue::from(HashMap::from([
                            ("aux_blob".to_string(), JsonValue::from(hex::encode(address_bytes))),
                            ("aux_diff".to_string(), JsonValue::from(*difficulty)),
                            ("aux_hash".to_string(), JsonValue::from(blockhash.as_string())),
                        ])),
                        id,
                    )
                    .into()
                } else {
                    JsonResponse::new(JsonValue::from(HashMap::new()), id).into()
                }
            }
            mm_blocktemplates.remove(&address_bytes);
        }

        // At this point, we should query the Validator for a new blocktemplate.
        // We first need to construct `MinerRewardsRecipientConfig` from the
        // address provided to us through the RPC.
        let recipient_config =
            MinerRewardsRecipientConfig { recipient: address, spend_hook: None, user_data: None };

        // Now let's try to construct the blocktemplate.
        // Find the difficulty. Note we cast it to f64 here.
        let difficulty: f64 = match extended_fork.module.next_difficulty() {
            Ok(v) => {
                // We will attempt to cast it to f64. This should always work.
                v.to_string().parse().unwrap()
            }
            Err(e) => {
                error!(
                    target: "darkfid::rpc_xmr::xmr_merge_mining_get_aux_block",
                    "[RPC-XMR] Finding next mining difficulty failed: {e}",
                );
                return JsonError::new(ErrorCode::InternalError, None, id).into()
            }
        };

        let (_, blocktemplate, block_signing_secret) = match generate_next_block(
            &mut extended_fork,
            &recipient_config,
            &self.powrewardv1_zk.zkbin,
            &self.powrewardv1_zk.provingkey,
            self.validator.consensus.module.read().await.target,
            self.validator.verify_fees,
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "darkfid::rpc_xmr::xmr_merge_mining_get_aux_block",
                    "[RPC-XMR] Failed to generate next blocktemplate: {e}",
                );
                return JsonError::new(ErrorCode::InternalError, None, id).into()
            }
        };

        // Now we have the blocktemplate. We'll mark it down in memory,
        // and then ship it to RPC.
        let blockhash = blocktemplate.header.template_hash();
        mm_blocktemplates.insert(address_bytes, (blocktemplate, difficulty, block_signing_secret));
        info!(
            target: "darkfid::rpc_xmr::xmr_merge_mining_get_aux_block",
            "[RPC-XMR] Created new blocktemplate: address={address}, aux_hash={blockhash}, height={height}, prev_id={prev_id}"
        );

        let response = JsonValue::from(HashMap::from([
            ("aux_blob".to_string(), JsonValue::from(hex::encode(address_bytes))),
            ("aux_diff".to_string(), JsonValue::from(difficulty)),
            ("aux_hash".to_string(), JsonValue::from(blockhash.as_string())),
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
    // --> {"jsonrpc":"2.0", "method": "merge_mining_submit_solution", "params": {"aux_blob": "4c6f72656d20697073756d", "aux_hash": "f6952d6eef555ddd87aca66e56b91530222d6e318414816f3ba7cf5bf694bf0f", "blob": "...", "merkle_proof": ["hash1", "hash2", "hash3"], "path": 3, "seed_hash": "22c3d47c595ae888b5d7fc304235f92f8854644d4fad38c5680a5d4a81009fcd"}, "id": 1}
    // <-- {"jsonrpc":"2.0", "result": {"status": "accepted"}, "id": 1}
    pub async fn xmr_merge_mining_submit_solution(&self, id: u16, params: JsonValue) -> JsonResult {
        // Check if node is synced before responding to p2pool
        if !*self.validator.synced.read().await {
            return server_error(RpcError::NotSynced, id, None)
        }

        // Parse request params
        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        // Parse address from aux_blob
        let Some(aux_blob) = params.get("aux_blob") else {
            return JsonError::new(InvalidParams, Some("missing aux_blob".to_string()), id).into()
        };
        let Some(aux_blob) = aux_blob.get::<String>() else {
            return JsonError::new(InvalidParams, Some("invalid aux_blob format".to_string()), id)
                .into()
        };
        let mut address_bytes = [0u8; 32];
        if hex::decode_to_slice(aux_blob, &mut address_bytes).is_err() {
            return JsonError::new(InvalidParams, Some("invalid aux_blob format".to_string()), id)
                .into()
        };
        if PublicKey::from_bytes(address_bytes).is_err() {
            return JsonError::new(InvalidParams, Some("invalid aux_blob format".to_string()), id)
                .into()
        };

        // Parse aux_hash
        let Some(aux_hash) = params.get("aux_hash") else {
            return JsonError::new(InvalidParams, Some("missing aux_hash".to_string()), id).into()
        };
        let Some(aux_hash) = aux_hash.get::<String>() else {
            return JsonError::new(InvalidParams, Some("invalid aux_hash format".to_string()), id)
                .into()
        };
        let Ok(aux_hash) = HeaderHash::from_str(aux_hash) else {
            return JsonError::new(InvalidParams, Some("invalid aux_hash format".to_string()), id)
                .into()
        };

        // If we don't know about this job, we can just abort here.
        let mut mm_blocktemplates = self.mm_blocktemplates.lock().await;
        if !mm_blocktemplates.contains_key(&address_bytes) {
            return JsonError::new(InvalidParams, Some("unknown address".to_string()), id).into()
        }

        // Parse blob
        let Some(blob) = params.get("blob") else {
            return JsonError::new(InvalidParams, Some("missing blob".to_string()), id).into()
        };
        let Some(blob) = blob.get::<String>() else {
            return JsonError::new(InvalidParams, Some("invalid blob format".to_string()), id).into()
        };
        let Ok(block) = monero_block_deserialize(blob) else {
            return JsonError::new(InvalidParams, Some("invalid blob format".to_string()), id).into()
        };

        // Parse merkle_proof
        let Some(merkle_proof_j) = params.get("merkle_proof") else {
            return JsonError::new(InvalidParams, Some("missing merkle_proof".to_string()), id)
                .into()
        };
        let Some(merkle_proof_j) = merkle_proof_j.get::<Vec<JsonValue>>() else {
            return JsonError::new(
                InvalidParams,
                Some("invalid merkle_proof format".to_string()),
                id,
            )
            .into()
        };
        let mut merkle_proof: Vec<monero::Hash> = Vec::with_capacity(merkle_proof_j.len());
        for hash in merkle_proof_j.iter() {
            match hash.get::<String>() {
                Some(v) => {
                    let Ok(val) = monero::Hash::from_hex(v) else {
                        return JsonError::new(
                            InvalidParams,
                            Some("invalid merkle_proof format".to_string()),
                            id,
                        )
                        .into()
                    };

                    merkle_proof.push(val);
                }
                None => {
                    return JsonError::new(
                        InvalidParams,
                        Some("invalid merkle_proof format".to_string()),
                        id,
                    )
                    .into()
                }
            }
        }

        // Parse path
        let Some(path) = params.get("path") else {
            return JsonError::new(InvalidParams, Some("missing path".to_string()), id).into()
        };
        let Some(path) = path.get::<f64>() else {
            return JsonError::new(InvalidParams, Some("invalid path format".to_string()), id).into()
        };
        let path = *path as u32;

        // Parse seed_hash
        let Some(seed_hash) = params.get("seed_hash") else {
            return JsonError::new(InvalidParams, Some("missing seed_hash".to_string()), id).into()
        };
        let Some(seed_hash) = seed_hash.get::<String>() else {
            return JsonError::new(InvalidParams, Some("invalid seed_hash format".to_string()), id)
                .into()
        };
        let Ok(seed_hash) = monero::Hash::from_hex(seed_hash) else {
            return JsonError::new(InvalidParams, Some("invalid seed_hash format".to_string()), id)
                .into()
        };
        let Ok(seed_hash) = FixedByteArray::from_bytes(seed_hash.as_bytes()) else {
            return JsonError::new(InvalidParams, Some("invalid seed_hash format".to_string()), id)
                .into()
        };

        info!(
            target: "darkfid::rpc_xmr::xmr_merge_mining_submit_solution",
            "[RPC-XMR] Got solution submission: aux_hash={aux_hash}",
        );

        // Construct the MoneroPowData
        let Some(merkle_proof) = MerkleProof::try_construct(merkle_proof, path) else {
            return JsonError::new(
                InvalidParams,
                Some("could not construct aux chain merkle proof".to_string()),
                id,
            )
            .into()
        };
        let monero_pow_data = match MoneroPowData::new(block, seed_hash, merkle_proof) {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "darkfid::rpc_xmr::xmr_merge_mining_submit_solution",
                    "[RPC-XMR] Failed constructing MoneroPowData: {e}",
                );
                return JsonError::new(
                    InvalidParams,
                    Some("failed constructing moneropowdata".to_string()),
                    id,
                )
                .into()
            }
        };

        // Append MoneroPowData to the DarkFi block and sign it
        let (block, _, secret) = &mm_blocktemplates.get(&address_bytes).unwrap();
        let mut block = block.clone();
        block.header.pow_data = PowData::Monero(monero_pow_data);
        block.sign(secret);

        // At this point we should be able to remove the submitted job.
        // We still won't release the lock in hope of proposing the block
        // first.
        mm_blocktemplates.remove(&address_bytes);

        // Propose the new block
        info!(
            target: "darkfid::rpc_xmr::xmr_merge_mining_submit_solution",
            "[RPC-XMR] Proposing new block to network",
        );
        let proposal = Proposal::new(block);
        if let Err(e) = self.validator.append_proposal(&proposal).await {
            error!(
                target: "darkfid::rpc_xmr::xmr_merge_submit_solution",
                "[RPC-XMR] Error proposing new block: {e}",
            );
            return JsonResponse::new(
                JsonValue::from(HashMap::from([(
                    "status".to_string(),
                    JsonValue::from("rejected".to_string()),
                )])),
                id,
            )
            .into()
        }

        let proposals_sub = self.subscribers.get("proposals").unwrap();
        let enc_prop = JsonValue::String(base64::encode(&serialize_async(&proposal).await));
        proposals_sub.notify(vec![enc_prop].into()).await;

        info!(
            target: "darkfid::rpc_xmr::xmr_merge_mining_submit_solution",
            "[RPC-XMR] Broadcasting new block to network",
        );
        let message = ProposalMessage(proposal);
        self.p2p_handler.p2p.broadcast(&message).await;

        JsonResponse::new(
            JsonValue::from(HashMap::from([(
                "status".to_string(),
                JsonValue::from("accepted".to_string()),
            )])),
            id,
        )
        .into()
    }
}
