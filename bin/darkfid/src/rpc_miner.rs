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
    blockchain::{BlockInfo, Header, HeaderHash},
    rpc::jsonrpc::{ErrorCode, ErrorCode::InvalidParams, JsonError, JsonResponse, JsonResult},
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    util::{encoding::base64, time::Timestamp},
    validator::{
        consensus::{Fork, Proposal},
        pow::{RANDOMX_KEY_CHANGE_DELAY, RANDOMX_KEY_CHANGING_HEIGHT},
        verification::apply_producer_transaction,
    },
    zk::ProvingKey,
    zkas::ZkBinary,
    Error, Result,
};
use darkfi_money_contract::{client::pow_reward_v1::PoWRewardCallBuilder, MoneyFunction};
use darkfi_sdk::{
    crypto::{
        pasta_prelude::PrimeField, FuncId, Keypair, MerkleTree, PublicKey, SecretKey,
        MONEY_CONTRACT_ID,
    },
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{serialize_async, Encodable};
use num_bigint::BigUint;
use rand::rngs::OsRng;
use tinyjson::JsonValue;
use tracing::{error, info};

use crate::{proto::ProposalMessage, server_error, DarkfiNode, RpcError};

/// Auxiliary structure representing node miner rewards recipient configuration.
pub struct MinerRewardsRecipientConfig {
    /// Wallet mining address to receive mining rewards
    pub recipient: PublicKey,
    /// Optional contract spend hook to use in the mining reward
    pub spend_hook: Option<FuncId>,
    /// Optional contract user data to use in the mining reward.
    /// This is not arbitrary data.
    pub user_data: Option<pallas::Base>,
}

/// Auxiliary structure representing a block template for native mining.
pub struct BlockTemplate {
    /// The block that is being mined
    pub block: BlockInfo,
    /// The base64 encoded RandomX key used
    randomx_key: String,
    /// The base64 encoded next RandomX key used
    next_randomx_key: String,
    /// The base64 encoded mining target used
    target: String,
    /// The signing secret for this block
    secret: SecretKey,
}

impl DarkfiNode {
    // RPCAPI:
    // Queries the validator for the current and next RandomX keys.
    // If no forks exist, retrieves the canonical ones.
    // Returns the current and next RandomX keys, both encoded as
    // base64 strings.
    //
    // **Params:**
    // * `None`
    //
    // **Returns:**
    // * `String`: Current RandomX key (base64 encoded)
    // * `String`: Current next RandomX key (base64 encoded)
    //
    // --> {"jsonrpc": "2.0", "method": "miner.get_current_randomx_keys", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["randomx_key", "next_randomx_key"], "id": 1}
    pub async fn miner_get_current_randomx_keys(&self, id: u16, params: JsonValue) -> JsonResult {
        // Verify request params
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        // Grab current RandomX keys
        let (randomx_key, next_randomx_key) = match self.validator.current_randomx_keys().await {
            Ok(keys) => keys,
            Err(e) => {
                error!(
                    target: "darkfid::rpc::miner_get_current_randomx_keys",
                    "[RPC] Retrieving current RandomX keys failed: {e}",
                );
                return JsonError::new(ErrorCode::InternalError, None, id).into()
            }
        };

        // Encode them and build response
        let response = JsonValue::Array(vec![
            JsonValue::String(base64::encode(&serialize_async(&randomx_key).await)),
            JsonValue::String(base64::encode(&serialize_async(&next_randomx_key).await)),
        ]);

        JsonResponse::new(response, id).into()
    }

    // RPCAPI:
    // Queries the validator for the current best fork next header to
    // mine.
    // Returns the current RandomX key, the mining target and the next
    // block header, all encoded as base64 strings.
    //
    // **Params:**
    // * `header`    : Mining job Header hash that is currently being polled (as string)
    // * `recipient` : Wallet mining address to receive mining rewards (as string)
    // * `spend_hook`: Optional contract spend hook to use in the mining reward (as string)
    // * `user_data` : Optional contract user data (not arbitrary data) to use in the mining reward (as string)
    //
    // **Returns:**
    // * `String`: Current best fork RandomX key (base64 encoded)
    // * `String`: Current best fork next RandomX key (base64 encoded)
    // * `String`: Current best fork mining target (base64 encoded)
    // * `String`: Current best fork next block header (base64 encoded)
    //
    // --> {"jsonrpc": "2.0", "method": "miner.get_header", "params": {"header": "hash", "recipient": "address"}, "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["randomx_key", "next_randomx_key", "target", "header"], "id": 1}
    pub async fn miner_get_header(&self, id: u16, params: JsonValue) -> JsonResult {
        // Check if node is synced before responding to miner
        if !*self.validator.synced.read().await {
            return server_error(RpcError::NotSynced, id, None)
        }

        // Parse request params
        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if params.len() < 2 || params.len() > 4 {
            return JsonError::new(InvalidParams, None, id).into()
        }

        // Parse header hash
        let Some(header_hash) = params.get("header") else {
            return server_error(RpcError::MinerMissingHeader, id, None)
        };
        let Some(header_hash) = header_hash.get::<String>() else {
            return server_error(RpcError::MinerInvalidHeader, id, None)
        };
        let Ok(header_hash) = HeaderHash::from_str(header_hash) else {
            return server_error(RpcError::MinerInvalidHeader, id, None)
        };

        // Parse recipient wallet address
        let Some(recipient) = params.get("recipient") else {
            return server_error(RpcError::MinerMissingRecipient, id, None)
        };
        let Some(recipient) = recipient.get::<String>() else {
            return server_error(RpcError::MinerInvalidRecipient, id, None)
        };
        let Ok(recipient) = PublicKey::from_str(recipient) else {
            return server_error(RpcError::MinerInvalidRecipient, id, None)
        };

        // Parse spend hook
        let spend_hook = match params.get("spend_hook") {
            Some(spend_hook) => {
                let Some(spend_hook) = spend_hook.get::<String>() else {
                    return server_error(RpcError::MinerInvalidSpendHook, id, None)
                };
                let Ok(spend_hook) = FuncId::from_str(spend_hook) else {
                    return server_error(RpcError::MinerInvalidSpendHook, id, None)
                };
                Some(spend_hook)
            }
            None => None,
        };

        // Parse user data
        let user_data: Option<pallas::Base> = match params.get("user_data") {
            Some(user_data) => {
                let Some(user_data) = user_data.get::<String>() else {
                    return server_error(RpcError::MinerInvalidUserData, id, None)
                };
                let Ok(bytes) = bs58::decode(&user_data).into_vec() else {
                    return server_error(RpcError::MinerInvalidUserData, id, None)
                };
                let bytes: [u8; 32] = match bytes.try_into() {
                    Ok(b) => b,
                    Err(_) => return server_error(RpcError::MinerInvalidUserData, id, None),
                };
                let Some(user_data) = pallas::Base::from_repr(bytes).into() else {
                    return server_error(RpcError::MinerInvalidUserData, id, None)
                };
                Some(user_data)
            }
            None => None,
        };

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
        let address_bytes = serialize_async(&(recipient, spend_hook, user_data)).await;
        let mut blocktemplates = self.blocktemplates.lock().await;
        let mut extended_fork = match self.validator.best_current_fork().await {
            Ok(f) => f,
            Err(e) => {
                error!(
                    target: "darkfid::rpc::miner_get_header",
                    "[RPC] Finding best fork index failed: {e}",
                );
                return JsonError::new(ErrorCode::InternalError, None, id).into()
            }
        };
        if let Some(blocktemplate) = blocktemplates.get(&address_bytes) {
            let last_proposal = match extended_fork.last_proposal() {
                Ok(p) => p,
                Err(e) => {
                    error!(
                        target: "darkfid::rpc::miner_get_header",
                        "[RPC] Retrieving best fork last proposal failed: {e}",
                    );
                    return JsonError::new(ErrorCode::InternalError, None, id).into()
                }
            };
            if last_proposal.hash == blocktemplate.block.header.previous {
                return if blocktemplate.block.header.hash() != header_hash {
                    JsonResponse::new(
                        JsonValue::Array(vec![
                            JsonValue::String(blocktemplate.randomx_key.clone()),
                            JsonValue::String(blocktemplate.next_randomx_key.clone()),
                            JsonValue::String(blocktemplate.target.clone()),
                            JsonValue::String(base64::encode(
                                &serialize_async(&blocktemplate.block.header).await,
                            )),
                        ]),
                        id,
                    )
                    .into()
                } else {
                    JsonResponse::new(JsonValue::Array(vec![]), id).into()
                }
            }
            blocktemplates.remove(&address_bytes);
        }

        // At this point, we should query the Validator for a new blocktemplate.
        // We first need to construct `MinerRewardsRecipientConfig` from the
        // address configuration provided to us through the RPC.
        let recipient_str = format!("{recipient}");
        let spend_hook_str = match spend_hook {
            Some(spend_hook) => format!("{spend_hook}"),
            None => String::from("-"),
        };
        let user_data_str = match user_data {
            Some(user_data) => bs58::encode(user_data.to_repr()).into_string(),
            None => String::from("-"),
        };
        let recipient_config = MinerRewardsRecipientConfig { recipient, spend_hook, user_data };

        // Now let's try to construct the blocktemplate.
        let (target, block, secret) = match generate_next_block(
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
                    target: "darkfid::rpc::miner_get_header",
                    "[RPC] Failed to generate next blocktemplate: {e}",
                );
                return JsonError::new(ErrorCode::InternalError, None, id).into()
            }
        };

        // Grab the RandomX key to use.
        // We only use the next key when the next block is the
        // height changing one.
        let randomx_key = if block.header.height > RANDOMX_KEY_CHANGING_HEIGHT &&
            block.header.height % RANDOMX_KEY_CHANGING_HEIGHT == RANDOMX_KEY_CHANGE_DELAY
        {
            base64::encode(&serialize_async(&extended_fork.module.darkfi_rx_keys.1).await)
        } else {
            base64::encode(&serialize_async(&extended_fork.module.darkfi_rx_keys.0).await)
        };

        // Grab the next RandomX key to use so miner can pregenerate
        // mining VMs.
        let next_randomx_key =
            base64::encode(&serialize_async(&extended_fork.module.darkfi_rx_keys.1).await);

        // Convert the target
        let target = base64::encode(&target.to_bytes_le());

        // Construct the block template
        let blocktemplate = BlockTemplate {
            block,
            randomx_key: randomx_key.clone(),
            next_randomx_key: next_randomx_key.clone(),
            target: target.clone(),
            secret,
        };

        // Now we have the blocktemplate. We'll mark it down in memory,
        // and then ship it to RPC.
        let header_hash = blocktemplate.block.header.hash().to_string();
        let header = base64::encode(&serialize_async(&blocktemplate.block.header).await);
        blocktemplates.insert(address_bytes, blocktemplate);
        info!(
            target: "darkfid::rpc::miner_get_header",
            "[RPC] Created new blocktemplate: address={recipient_str}, spend_hook={spend_hook_str}, user_data={user_data_str}, hash={header_hash}"
        );

        let response = JsonValue::Array(vec![
            JsonValue::String(randomx_key),
            JsonValue::String(next_randomx_key),
            JsonValue::String(target),
            JsonValue::String(header),
        ]);

        JsonResponse::new(response, id).into()
    }

    // RPCAPI:
    // Submits a PoW solution header nonce for a block.
    // Returns the block submittion status.
    //
    // **Params:**
    // * `recipient` : Wallet mining address used (as string)
    // * `spend_hook`: Optional contract spend hook used (as string)
    // * `user_data` : Optional contract user data (not arbitrary data) used (as string)
    // * `nonce`     : The solution header nonce (as f64)
    //
    // **Returns:**
    // * `String`: Block submit status
    //
    // --> {"jsonrpc": "2.0", "method": "miner.submit_solution", "params": {"recipient": "address", "nonce": 42}, "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "accepted", "id": 1}
    pub async fn miner_submit_solution(&self, id: u16, params: JsonValue) -> JsonResult {
        // Check if node is synced before responding to p2pool
        if !*self.validator.synced.read().await {
            return server_error(RpcError::NotSynced, id, None)
        }

        // Parse request params
        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if params.len() < 2 || params.len() > 4 {
            return JsonError::new(InvalidParams, None, id).into()
        }

        // Parse recipient wallet address
        let Some(recipient) = params.get("recipient") else {
            return server_error(RpcError::MinerMissingRecipient, id, None)
        };
        let Some(recipient) = recipient.get::<String>() else {
            return server_error(RpcError::MinerInvalidRecipient, id, None)
        };
        let Ok(recipient) = PublicKey::from_str(recipient) else {
            return server_error(RpcError::MinerInvalidRecipient, id, None)
        };

        // Parse spend hook
        let spend_hook = match params.get("spend_hook") {
            Some(spend_hook) => {
                let Some(spend_hook) = spend_hook.get::<String>() else {
                    return server_error(RpcError::MinerInvalidSpendHook, id, None)
                };
                let Ok(spend_hook) = FuncId::from_str(spend_hook) else {
                    return server_error(RpcError::MinerInvalidSpendHook, id, None)
                };
                Some(spend_hook)
            }
            None => None,
        };

        // Parse user data
        let user_data: Option<pallas::Base> = match params.get("user_data") {
            Some(user_data) => {
                let Some(user_data) = user_data.get::<String>() else {
                    return server_error(RpcError::MinerInvalidUserData, id, None)
                };
                let Ok(bytes) = bs58::decode(&user_data).into_vec() else {
                    return server_error(RpcError::MinerInvalidUserData, id, None)
                };
                let bytes: [u8; 32] = match bytes.try_into() {
                    Ok(b) => b,
                    Err(_) => return server_error(RpcError::MinerInvalidUserData, id, None),
                };
                let Some(user_data) = pallas::Base::from_repr(bytes).into() else {
                    return server_error(RpcError::MinerInvalidUserData, id, None)
                };
                Some(user_data)
            }
            None => None,
        };

        // Parse nonce
        let Some(nonce) = params.get("nonce") else {
            return server_error(RpcError::MinerMissingNonce, id, None)
        };
        let Some(nonce) = nonce.get::<f64>() else {
            return server_error(RpcError::MinerInvalidNonce, id, None)
        };

        // If we don't know about this job, we can just abort here.
        let address_bytes = serialize_async(&(recipient, spend_hook, user_data)).await;
        let mut blocktemplates = self.blocktemplates.lock().await;
        let Some(blocktemplate) = blocktemplates.get(&address_bytes) else {
            return server_error(RpcError::MinerUnknownJob, id, None)
        };

        info!(
            target: "darkfid::rpc::miner_submit_solution",
            "[RPC] Got solution submission for block template: {}", blocktemplate.block.header.hash(),
        );

        // Sign the DarkFi block
        let mut block = blocktemplate.block.clone();
        block.header.nonce = *nonce as u64;
        block.sign(&blocktemplate.secret);
        info!(
            target: "darkfid::rpc::miner_submit_solution",
            "[RPC] Mined block header hash: {}", blocktemplate.block.header.hash(),
        );

        // At this point we should be able to remove the submitted job.
        // We still won't release the lock in hope of proposing the block
        // first.
        blocktemplates.remove(&address_bytes);

        // Propose the new block
        info!(
            target: "darkfid::rpc::miner_submit_solution",
            "[RPC] Proposing new block to network",
        );
        let proposal = Proposal::new(block);
        if let Err(e) = self.validator.append_proposal(&proposal).await {
            error!(
                target: "darkfid::rpc::miner_submit_solution",
                "[RPC] Error proposing new block: {e}",
            );
            return JsonResponse::new(JsonValue::String(String::from("rejected")), id).into()
        }

        let proposals_sub = self.subscribers.get("proposals").unwrap();
        let enc_prop = JsonValue::String(base64::encode(&serialize_async(&proposal).await));
        proposals_sub.notify(vec![enc_prop].into()).await;

        info!(
            target: "darkfid::rpc::miner_submit_solution",
            "[RPC] Broadcasting new block to network",
        );
        let message = ProposalMessage(proposal);
        self.p2p_handler.p2p.broadcast(&message).await;

        JsonResponse::new(JsonValue::String(String::from("accepted")), id).into()
    }
}

/// Auxiliary function to generate next block in an atomic manner.
pub async fn generate_next_block(
    extended_fork: &mut Fork,
    recipient_config: &MinerRewardsRecipientConfig,
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    block_target: u32,
    verify_fees: bool,
) -> Result<(BigUint, BlockInfo, SecretKey)> {
    // Grab forks' last block proposal(previous)
    let last_proposal = extended_fork.last_proposal()?;

    // Grab forks' next block height
    let next_block_height = last_proposal.block.header.height + 1;

    // Grab forks' unproposed transactions
    let (mut txs, _, fees, overlay) = extended_fork
        .unproposed_txs(&extended_fork.blockchain, next_block_height, block_target, verify_fees)
        .await?;

    // Create an ephemeral block signing keypair. Its secret key will
    // be stored in the PowReward transaction's encrypted note for
    // later retrieval. It is encrypted towards the recipient's public
    // key.
    let block_signing_keypair = Keypair::random(&mut OsRng);

    // Generate reward transaction
    let tx = generate_transaction(
        next_block_height,
        fees,
        &block_signing_keypair,
        recipient_config,
        zkbin,
        pk,
    )?;

    // Apply producer transaction in the overlay
    let _ = apply_producer_transaction(
        &overlay,
        next_block_height,
        block_target,
        &tx,
        &mut MerkleTree::new(1),
    )
    .await?;
    txs.push(tx);

    // Grab the updated contracts states root
    overlay.lock().unwrap().contracts.update_state_monotree(&mut extended_fork.state_monotree)?;
    let Some(state_root) = extended_fork.state_monotree.get_headroot()? else {
        return Err(Error::ContractsStatesRootNotFoundError);
    };

    // Drop new trees opened by the unproposed transactions overlay
    overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;

    // Generate the new header
    let mut header =
        Header::new(last_proposal.hash, next_block_height, Timestamp::current_time(), 0);
    header.state_root = state_root;

    // Generate the block
    let mut next_block = BlockInfo::new_empty(header);

    // Add transactions to the block
    next_block.append_txs(txs);

    // Grab the next mine target
    let target = extended_fork.module.next_mine_target()?;

    Ok((target, next_block, block_signing_keypair.secret))
}

/// Auxiliary function to generate a Money::PoWReward transaction.
fn generate_transaction(
    block_height: u32,
    fees: u64,
    block_signing_keypair: &Keypair,
    recipient_config: &MinerRewardsRecipientConfig,
    zkbin: &ZkBinary,
    pk: &ProvingKey,
) -> Result<Transaction> {
    // Build the transaction debris
    let debris = PoWRewardCallBuilder {
        signature_keypair: *block_signing_keypair,
        block_height,
        fees,
        recipient: Some(recipient_config.recipient),
        spend_hook: recipient_config.spend_hook,
        user_data: recipient_config.user_data,
        mint_zkbin: zkbin.clone(),
        mint_pk: pk.clone(),
    }
    .build()?;

    // Generate and sign the actual transaction
    let mut data = vec![MoneyFunction::PoWRewardV1 as u8];
    debris.params.encode(&mut data)?;
    let call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };
    let mut tx_builder =
        TransactionBuilder::new(ContractCallLeaf { call, proofs: debris.proofs }, vec![])?;
    let mut tx = tx_builder.build()?;
    let sigs = tx.create_sigs(&[block_signing_keypair.secret])?;
    tx.signatures = vec![sigs];

    Ok(tx)
}
