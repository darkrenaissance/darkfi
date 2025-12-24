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
    rpc::jsonrpc::{ErrorCode, ErrorCode::InvalidParams, JsonError, JsonResponse, JsonResult},
    util::{encoding::base64, time::Timestamp},
    validator::consensus::Proposal,
};
use darkfi_sdk::crypto::keypair::Address;
use darkfi_serial::serialize_async;
use tinyjson::JsonValue;
use tracing::{error, info};

use crate::{
    proto::ProposalMessage,
    rpc_miner::{generate_next_block, MinerRewardsRecipientConfig},
    BlockTemplate, DarkfiNode, MiningJobs,
};

// https://github.com/xmrig/xmrig-proxy/blob/master/doc/STRATUM.md
// https://github.com/xmrig/xmrig-proxy/blob/master/doc/STRATUM_EXT.md

// TODO: We often just return InvalidParams. These should be cleaned up
// and more verbose.
// TODO: The jobs storing method is not the most ideal. Think of a better one.
// `self.mining_jobs`

// Random testnet address for reference:
// fUfG4WhbHP5C2MhYW3FHctVqi2jfXHamoQeU8KiirKVtoMBEUejkwq9F

impl DarkfiNode {
    // RPCAPI:
    // Miner sends a `login` request after establishing connection
    // in order to authorize.
    //
    // The server will return a job along with an id.
    // ```
    // "job": {
    //     "blob": 070780e6b9d...4d62fa6c77e76c3001",
    //     "job_id": "q7PLUPL25UV0z5Ij14IyMk8htXbj",
    //     "target": "b88d0600",
    //     "algo": "rx/0"
    // }
    // ```
    //
    // --> {"jsonrpc":"2.0", "method": "login", "id": 1, "params": {"login": "receiving_address", "pass": "x", "agent": "XMRig", "algo": ["rx/0"]}}
    // <-- {"jsonrpc":"2.0", "id": 1, "result": {"id": "1be0b7b6-b15a-47be-a17d-46b2911cf7d0", "job": { ... }, "status": "OK"}}
    pub async fn stratum_login(&self, id: u16, params: JsonValue) -> JsonResult {
        // TODO: Fail when not synced
        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        let Some(login) = params.get("login") else {
            return JsonError::new(InvalidParams, Some("Missing 'login'".to_string()), id).into()
        };
        let Some(login) = login.get::<String>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        let Some(pass) = params.get("pass") else {
            return JsonError::new(InvalidParams, Some("Missing 'pass'".to_string()), id).into()
        };
        let Some(_pass) = pass.get::<String>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        let Some(agent) = params.get("agent") else {
            return JsonError::new(InvalidParams, Some("Missing 'agent'".to_string()), id).into()
        };
        let Some(agent) = agent.get::<String>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        let Some(algo) = params.get("algo") else {
            return JsonError::new(InvalidParams, Some("Missing 'algo'".to_string()), id).into()
        };
        let Some(algo) = algo.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        // Try to parse `login` as valid address. This will be the
        // block reward recipient.
        let Ok(address) = Address::from_str(login) else {
            return JsonError::new(InvalidParams, Some("Invalid address".to_string()), id).into()
        };
        if address.network() != self.network {
            return JsonError::new(InvalidParams, Some("Invalid address prefix".to_string()), id)
                .into()
        }

        // Iterate through `algo` to see if "rx/0" is supported.
        // rx/0 is RandomX.
        let mut found_rx0 = false;
        for i in algo {
            let Some(algo) = i.get::<String>() else {
                return JsonError::new(InvalidParams, None, id).into()
            };
            if algo == "rx/0" {
                found_rx0 = true;
                break
            }
        }
        if !found_rx0 {
            return JsonError::new(InvalidParams, Some("rx/0 not supported".to_string()), id).into()
        }

        info!("[STRATUM] Got login from {} ({})", address, agent);
        let conn_id = {
            let mut hasher = blake3::Hasher::new();
            hasher.update(&address.to_string().into_bytes());
            hasher.update(&Timestamp::current_time().inner().to_le_bytes());
            hasher.finalize().as_bytes().clone()
        };

        // Now we should register this login, and create a blocktemplate and
        // a job for them.
        // TODO: We also have to spawn the notification task that will send
        // JSONRPC notifications to this connection when a new job is available.

        // We'll clear any existing jobs for this login.
        let mut mining_jobs = self.mining_jobs.lock().await;
        mining_jobs.insert(conn_id, MiningJobs::default());

        // Find applicable chain fork
        let mut extended_fork = match self.validator.best_current_fork().await {
            Ok(f) => f,
            Err(e) => {
                error!(
                    target: "darkfid::rpc_stratum::stratum_login",
                    "[STRATUM] Finding best fork index failed: {e}",
                );
                return JsonError::new(ErrorCode::InternalError, None, id).into()
            }
        };

        // Query the Validator for a new blocktemplate.
        // We first need to construct `MinerRewardsRecipientConfig` from the
        // address configuration provided to us through the RPC.
        // TODO: Parse any spend hook from the login. We might also want to
        // define a specific address format if it includes extra data.
        // We could also include arbitrary information in the login password.
        let recipient_config =
            MinerRewardsRecipientConfig { recipient: address, spend_hook: None, user_data: None };

        // Find next block target
        let target = self.validator.consensus.module.read().await.target;

        // Generate blocktemplate with all the information.
        // This will return the mining target, the entire block, and the
        // ephemeral secret used to sign the mined block.
        let (target, block, secret) = match generate_next_block(
            &mut extended_fork,
            &recipient_config,
            &self.powrewardv1_zk.zkbin,
            &self.powrewardv1_zk.provingkey,
            target,
            self.validator.verify_fees,
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "darkfid::rpc_stratum::stratum_login",
                    "[STRATUM] Failed to generate next blocktemplate: {e}",
                );
                return JsonError::new(ErrorCode::InternalError, None, id).into()
            }
        };

        // Reference the RandomX dataset seed
        // TODO: We can also send `next_seed_hash` when we know it.
        let seed_hash = extended_fork.module.darkfi_rx_keys.0.inner();

        // We will store this in our mining jobs map for reference when
        // a miner solution is submitted.
        let blocktemplate =
            BlockTemplate { block, randomx_key: *seed_hash, target: target.clone(), secret };

        // Construct everything needed for the Stratum response.
        let blob = blocktemplate.block.header.to_blockhashing_blob();
        let job_id = blocktemplate.block.header.hash().inner().clone();
        let height = blocktemplate.block.header.height as f64;
        // The target should be compacted to 8 bytes little-endian.
        let target = &target.to_bytes_le()[..8];

        // Store the job. unwrap should be fine because we created this above.
        let jobs = mining_jobs.get_mut(&conn_id).unwrap();
        jobs.insert(job_id, blocktemplate);

        // Construct response
        let job: HashMap<String, JsonValue> = HashMap::from([
            ("blob".to_string(), hex::encode(&blob).to_string().into()),
            ("job_id".to_string(), hex::encode(&job_id).to_string().into()),
            ("height".to_string(), height.into()),
            ("target".to_string(), hex::encode(target).into()),
            ("algo".to_string(), "rx/0".to_string().into()),
            ("seed_hash".to_string(), hex::encode(&seed_hash).into()),
        ]);

        let result = HashMap::from([
            ("id".to_string(), hex::encode(&conn_id).into()),
            ("job".to_string(), job.into()),
            ("status".to_string(), "OK".to_string().into()),
        ]);

        // Ship it.
        JsonResponse::new(result.into(), id).into()
    }

    // RPCAPI:
    // Miner submits a job solution.
    //
    // --> {"jsonrpc":"2.0", "method": "submit", "id": 1, "params": {"id": "...", "job_id": "...", "nonce": "d0030040", "result": "e1364b8782719d7683e2ccd3d8f724bc59dfa780a9e960e7c0e0046acdb40100"}}
    // <-- {"jsonrpc":"2.0", "id": 1, "result": {"status": "OK"}}
    pub async fn stratum_submit(&self, id: u16, params: JsonValue) -> JsonResult {
        // TODO: Maybe grab an exclusive lock to avoid the xmrig spam while
        // we're doing the pow verification. xmrig spams us whenever it gets
        // a solution, and this will end up in cloning a bunch of blocktemplates
        // and is going to cause memory usage to go up significantly.
        // Ideally we should block here until we finish each submit one-by-one
        // and find a valid one. Then when we do find a valid one, we should
        // clear the existing job(s) so this method will just return an error
        // and not have to do all the block shenanigans.
        // Additionally when a block is proposed successfully, the node should
        // send a new job notification to xmrig so we should be fine.
        // That notification part should also clear the existing jobs.
        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        let Some(conn_id) = params.get("id") else {
            return JsonError::new(InvalidParams, Some("Missing 'id'".to_string()), id).into()
        };
        let Some(conn_id) = conn_id.get::<String>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        let Some(job_id) = params.get("job_id") else {
            return JsonError::new(InvalidParams, Some("Missing 'job_id'".to_string()), id).into()
        };
        let Some(job_id) = job_id.get::<String>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        let Some(nonce) = params.get("nonce") else {
            return JsonError::new(InvalidParams, Some("Missing 'nonce'".to_string()), id).into()
        };
        let Some(nonce) = nonce.get::<String>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        // result is the RandomX calculated hash. Useful to verify/debug.
        let Some(result) = params.get("result") else {
            return JsonError::new(InvalidParams, Some("Missing 'result'".to_string()), id).into()
        };
        let Some(_result) = result.get::<String>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        let Ok(conn_id) = hex::decode(&conn_id) else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if conn_id.len() != 32 {
            return JsonError::new(InvalidParams, None, id).into()
        }
        let Ok(job_id) = hex::decode(&job_id) else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if job_id.len() != 32 {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let conn_id: [u8; 32] = conn_id.try_into().unwrap();
        let job_id: [u8; 32] = job_id.try_into().unwrap();

        // We should be aware of this conn_id and job_id.
        let mut mining_jobs = self.mining_jobs.lock().await;
        let Some(jobs) = mining_jobs.get_mut(&conn_id) else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        // Get the blocktemplate.
        let Some(blocktemplate) = jobs.get_mut(&job_id) else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        // Parse the nonce into u32.
        let Ok(nonce_bytes) = hex::decode(&nonce) else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if nonce_bytes.len() != 4 {
            return JsonError::new(InvalidParams, None, id).into()
        }
        let nonce = u32::from_le_bytes(nonce_bytes.try_into().unwrap());

        // We clone the block, update the nonce,
        // sign it, and ship it into a proposal.
        let mut block = blocktemplate.block.clone();
        block.header.nonce = nonce;
        block.sign(&blocktemplate.secret);

        info!(
            target: "darkfid::rpc_stratum::stratum_submit",
            "[STRATUM] Proposing new block to network",
        );
        let proposal = Proposal::new(block);
        if let Err(e) = self.validator.append_proposal(&proposal).await {
            error!(
                target: "darkfid::rpc_stratum::stratum_submit",
                "[STRATUM] Error proposing new block: {e}",
            );
            return JsonError::new(InvalidParams, None, id).into()
        }

        // Proposal passed. We will now clear the jobs as it's assumed
        // a new block needs to be mined.
        mining_jobs.insert(conn_id, MiningJobs::default());

        // Broadcast to network
        let proposals_sub = self.subscribers.get("proposals").unwrap();
        let enc_prop = JsonValue::String(base64::encode(&serialize_async(&proposal).await));
        proposals_sub.notify(vec![enc_prop].into()).await;

        info!(
            target: "darkfid::rpc_stratum::stratum_submit",
            "[STRATUM] Broadcasting new block to network",
        );
        let message = ProposalMessage(proposal);
        self.p2p_handler.p2p.broadcast(&message).await;

        JsonResponse::new(
            HashMap::from([("status".to_string(), "OK".to_string().into())]).into(),
            id,
        )
        .into()
    }

    // RPCAPI:
    // Miner sends `keepalived` to prevent connection timeout.
    //
    // --> {"jsonrpc":"2.0", "method": "keepalived", "id": 1, "params": {"id": "foo"}}
    // <-- {"jsonrpc":"2.0", "id": 1, "result": {"status": "KEEPALIVED"}}
    pub async fn stratum_keepalived(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        let Some(_conn_id) = params.get("id") else {
            return JsonError::new(InvalidParams, Some("Missing 'id'".to_string()), id).into()
        };

        // TODO: This conn_id should likely exist. We should probably check
        // that. Otherwise we might not want to reply at all.

        JsonResponse::new(
            JsonValue::from(HashMap::from([("status".into(), "KEEPALIVED".to_string().into())])),
            id,
        )
        .into()
    }
}
