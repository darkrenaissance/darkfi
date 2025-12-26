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

use crate::DarkfiNode;

impl DarkfiNode {
    /*
    // RPCAPI:
    // Queries the validator for the current mining RandomX key,
    // based on next block height.
    // If no forks exist, returns the canonical key.
    // Returns the current mining RandomX key encoded as base64 string.
    //
    // **Params:**
    // * `None`
    //
    // **Returns:**
    // * `String`: Current mining RandomX key (base64 encoded)
    //
    // --> {"jsonrpc": "2.0", "method": "miner.get_current_mining_randomx_key", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["randomx_key"], "id": 1}
    pub async fn miner_get_current_mining_randomx_key(
        &self,
        id: u16,
        params: JsonValue,
    ) -> JsonResult {
        // Verify request params
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        // Grab current mining RandomX key
        let randomx_key = match self.validator.current_mining_randomx_key().await {
            Ok(key) => key,
            Err(e) => {
                error!(
                    target: "darkfid::rpc::current_mining_randomx_key",
                    "[RPC] Retrieving current mining RandomX key failed: {e}",
                );
                return JsonError::new(ErrorCode::InternalError, None, id).into()
            }
        };

        // Encode them and build response
        let response = JsonValue::Array(vec![JsonValue::String(base64::encode(
            &serialize_async(&randomx_key).await,
        ))]);

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
    // * `String`: Current best fork mining target (base64 encoded)
    // * `String`: Current best fork next block header (base64 encoded)
    //
    // --> {"jsonrpc": "2.0", "method": "miner.get_header", "params": {"header": "hash", "recipient": "address"}, "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["randomx_key", "target", "header"], "id": 1}
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
        let Some(recipient_str) = recipient.get::<String>() else {
            return server_error(RpcError::MinerInvalidRecipient, id, None)
        };
        let Ok(recipient) = Address::from_str(recipient_str) else {
            return server_error(RpcError::MinerInvalidRecipient, id, None)
        };
        if recipient.network() != self.network {
            return server_error(RpcError::MinerInvalidRecipientPrefix, id, None)
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
        let address_bytes =
            serialize_async(&(recipient_str.clone().into_bytes(), spend_hook, user_data)).await;
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

        // Convert the target
        let target = base64::encode(&target.to_bytes_le());

        // Construct the block template
        let blocktemplate = BlockTemplate {
            block,
            randomx_key: randomx_key.clone(),
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
        let Some(recipient_str) = recipient.get::<String>() else {
            return server_error(RpcError::MinerInvalidRecipient, id, None)
        };
        let Ok(recipient) = Address::from_str(recipient_str) else {
            return server_error(RpcError::MinerInvalidRecipient, id, None)
        };
        if recipient.network() != self.network {
            return server_error(RpcError::MinerInvalidRecipientPrefix, id, None)
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
        let address_bytes =
            serialize_async(&(recipient_str.clone().into_bytes(), spend_hook, user_data)).await;
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
        block.header.nonce = *nonce as u32;
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
    */
}
