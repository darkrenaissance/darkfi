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

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use smol::lock::MutexGuard;
use tinyjson::JsonValue;
use tracing::{debug, error, info};

use darkfi::{
    rpc::{
        jsonrpc::{
            ErrorCode, ErrorCode::InvalidParams, JsonError, JsonRequest, JsonResponse, JsonResult,
        },
        server::RequestHandler,
    },
    system::StoppableTaskPtr,
};

use crate::{
    error::{miner_status_response, server_error, RpcError},
    registry::model::MinerRewardsRecipientConfig,
    DarkfiNode,
};

// https://github.com/xmrig/xmrig-proxy/blob/master/doc/STRATUM.md
// https://github.com/xmrig/xmrig-proxy/blob/master/doc/STRATUM_EXT.md

/// JSON-RPC `RequestHandler` for Stratum
pub struct StratumRpcHandler;

#[async_trait]
#[rustfmt::skip]
impl RequestHandler<StratumRpcHandler> for DarkfiNode {
	async fn handle_request(&self, req: JsonRequest) -> JsonResult {
		debug!(target: "darkfid::rpc::stratum_rpc", "--> {}", req.stringify().unwrap());

		match req.method.as_str() {
			// ======================
			// Stratum mining methods
			// ======================
			"login" => self.stratum_login(req.id, req.params).await,
			"submit" => self.stratum_submit(req.id, req.params).await,
			"keepalived" => self.stratum_keepalived(req.id, req.params).await,
			_ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
		}
	}

    async fn connections_mut(&self) -> MutexGuard<'life0, HashSet<StoppableTaskPtr>> {
        self.registry.stratum_rpc_connections.lock().await
    }
}

impl DarkfiNode {
    // RPCAPI:
    // Register a new mining client to the registry and generate a new
    // job.
    //
    // **Request:**
    // * `login` : A wallet address or its base-64 encoded mining configuration
    // * `pass`  : Unused client password field
    // * `agent` : Client agent description
    // * `algo`  : Client supported mining algorithms
    //
    // **Response:**
    // * `id`     : Registry client ID
    // * `job`    : The generated mining job
    // * `status` : Response status
    //
    // The generated mining job map consists of the following fields:
    // * `blob`      : The hex encoded block hashing blob of the job block
    // * `job_id`    : Registry mining job ID
    // * `height`    : The job block height
    // * `target`    : Current mining target
    // * `algo`      : The mining algorithm - RandomX
    // * `seed_hash` : Current RandomX key
    // * `next_seed_hash`: (optional) Next RandomX key if it is known
    //
    // --> {
    //       "jsonrpc": "2.0",
    //       "method": "login",
    //       "params": {
    //         "login": "WALLET_ADDRESS",
    //         "pass": "x",
    //         "agent": "XMRig",
    //         "algo": ["rx/0"]
    //       },
    //       "id": 1
    //     }
    // <-- {
    //       "jsonrpc": "2.0",
    //       "result": {
    //         "id": "unique_connection-id",
    //         "job": {
    //           "blob": "abcdef...001234",
    //           "job_id": "unique_job-id",
    //           "height": 1234,
    //           "target": "abcd1234",
    //           "algo": "rx/0",
    //           "seed_hash": "deadbeef...0234",
    //           "next_seed_hash": "c0fefe...1243"
    //         },
    //         "status": "OK"
    //       },
    //       "id": 1
    //     }
    pub async fn stratum_login(&self, id: u16, params: JsonValue) -> JsonResult {
        // Check if node is synced before responding
        if !*self.validator.synced.read().await {
            return JsonResponse::new(JsonValue::from(HashMap::new()), id).into()
        }

        // Parse request params
        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        // Parse login
        let Some(wallet) = params.get("login") else {
            return server_error(RpcError::MinerMissingLogin, id, None)
        };
        let Some(wallet) = wallet.get::<String>() else {
            return server_error(RpcError::MinerInvalidLogin, id, None)
        };
        let config =
            match MinerRewardsRecipientConfig::from_str(&self.registry.network, wallet).await {
                Ok(c) => c,
                Err(e) => return server_error(e, id, None),
            };

        // Parse password
        let Some(pass) = params.get("pass") else {
            return server_error(RpcError::MinerMissingPassword, id, None)
        };
        let Some(_pass) = pass.get::<String>() else {
            return server_error(RpcError::MinerInvalidPassword, id, None)
        };

        // Parse agent
        let Some(agent) = params.get("agent") else {
            return server_error(RpcError::MinerMissingAgent, id, None)
        };
        let Some(agent) = agent.get::<String>() else {
            return server_error(RpcError::MinerInvalidAgent, id, None)
        };

        // Parge algo
        let Some(algo) = params.get("algo") else {
            return server_error(RpcError::MinerMissingAlgo, id, None)
        };
        let Some(algo) = algo.get::<Vec<JsonValue>>() else {
            return server_error(RpcError::MinerInvalidAlgo, id, None)
        };

        // Iterate through `algo` to see if "rx/0" is supported.
        // rx/0 is RandomX.
        let mut found_rx0 = false;
        for i in algo {
            let Some(algo) = i.get::<String>() else {
                return server_error(RpcError::MinerInvalidAlgo, id, None)
            };
            if algo == "rx/0" {
                found_rx0 = true;
                break
            }
        }
        if !found_rx0 {
            return server_error(RpcError::MinerRandomXNotSupported, id, None)
        }

        // Register the new miner
        info!(
            target: "darkfid::rpc::rpc_stratum::stratum_login",
            "[RPC-STRATUM] Got login from {wallet} ({agent})",
        );
        let (client_id, job_id, job, publisher) =
            match self.registry.register_miner(&self.validator, wallet, &config).await {
                Ok(p) => p,
                Err(e) => {
                    error!(
                        target: "darkfid::rpc::rpc_stratum::stratum_login",
                        "[RPC-STRATUM] Failed to register miner: {e}",
                    );
                    return JsonResponse::new(JsonValue::from(HashMap::new()), id).into()
                }
            };

        // Now we have the new job, we ship it to RPC
        info!(
            target: "darkfid::rpc::rpc_stratum::stratum_login",
            "[RPC-STRATUM] Created new mining job for client {client_id}: {job_id}"
        );
        let response = JsonValue::from(HashMap::from([
            ("id".to_string(), JsonValue::from(client_id)),
            ("job".to_string(), job),
            ("status".to_string(), JsonValue::from(String::from("OK"))),
        ]));
        (publisher, JsonResponse::new(response, id)).into()
    }

    // RPCAPI:
    // Miner submits a job solution.
    //
    // **Request:**
    // * `id`     : Registry client ID
    // * `job_id` : Registry mining job ID
    // * `nonce`  : The hex encoded solution header nonce.
    // * `result` : RandomX calculated hash
    //
    // **Response:**
    // * `status`: Block submit status
    //
    // --> {
    //       "jsonrpc": "2.0",
    //       "method": "submit",
    //       "params": {
    //         "id": "unique_connection-id",
    //         "job_id": "unique_job-id",
    //         "nonce": "d0030040",
    //         "result": "e1364b8782719d7683e2ccd3d8f724bc59dfa780a9e960e7c0e0046acdb40100"
    //       },
    //       "id": 1
    //     }
    // <-- {"jsonrpc": "2.0", "result": {"status": "OK"}, "id": 1}
    pub async fn stratum_submit(&self, id: u16, params: JsonValue) -> JsonResult {
        // Check if node is synced before responding
        if !*self.validator.synced.read().await {
            return miner_status_response(id, "rejected")
        }

        // Grab registry submissions lock
        let submit_lock = self.registry.submit_lock.write().await;

        // Parse request params
        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        // Parse client id
        let Some(client_id) = params.get("id") else {
            return server_error(RpcError::MinerMissingClientId, id, None)
        };
        let Some(client_id) = client_id.get::<String>() else {
            return server_error(RpcError::MinerInvalidClientId, id, None)
        };

        // If we don't know about this client, we can just abort here
        let mut jobs = self.registry.jobs.write().await;
        let Some(client) = jobs.get(client_id) else {
            return miner_status_response(id, "rejected")
        };

        // Parse job id
        let Some(job_id) = params.get("job_id") else {
            return server_error(RpcError::MinerMissingJobId, id, None)
        };
        let Some(job_id) = job_id.get::<String>() else {
            return server_error(RpcError::MinerInvalidJobId, id, None)
        };

        // If this job doesn't match the client one, we can just abort
        // here.
        if &client.job != job_id {
            return miner_status_response(id, "rejected")
        }

        // If this client job wallet template doesn't exist, we can
        // just abort here.
        let mut block_templates = self.registry.block_templates.write().await;
        let Some(block_template) = block_templates.get_mut(&client.wallet) else {
            return miner_status_response(id, "rejected")
        };

        // If this template has been already submitted, reject this
        // submission.
        if block_template.submitted {
            return miner_status_response(id, "rejected")
        }

        // Parse nonce
        let Some(nonce) = params.get("nonce") else {
            return server_error(RpcError::MinerMissingNonce, id, None)
        };
        let Some(nonce) = nonce.get::<String>() else {
            return server_error(RpcError::MinerInvalidNonce, id, None)
        };
        let Ok(nonce_bytes) = hex::decode(nonce) else {
            return server_error(RpcError::MinerInvalidNonce, id, None)
        };
        if nonce_bytes.len() != 4 {
            return server_error(RpcError::MinerInvalidNonce, id, None)
        }
        let nonce = u32::from_le_bytes(nonce_bytes.try_into().unwrap());

        // Parse result
        let Some(result) = params.get("result") else {
            return server_error(RpcError::MinerMissingResult, id, None)
        };
        let Some(_result) = result.get::<String>() else {
            return server_error(RpcError::MinerInvalidResult, id, None)
        };

        info!(
            target: "darkfid::rpc::rpc_stratum::stratum_submit",
            "[RPC-STRATUM] Got solution submission from client {client_id} for job: {job_id}",
        );

        // Update the block nonce and sign it
        let mut block = block_template.block.clone();
        block.header.nonce = nonce;
        block.sign(&block_template.secret);

        // Submit the new block through the registry
        if let Err(e) =
            self.registry.submit(&self.validator, &self.subscribers, &self.p2p_handler, block).await
        {
            error!(
                target: "darkfid::rpc::rpc_stratum::stratum_submit",
                "[RPC-STRATUM] Error submitting new block: {e}",
            );

            // Try to refresh the jobs before returning error
            let mut mm_jobs = self.registry.mm_jobs.write().await;
            if let Err(e) = self
                .registry
                .refresh_jobs(&mut block_templates, &mut jobs, &mut mm_jobs, &self.validator)
                .await
            {
                error!(
                    target: "darkfid::rpc::rpc_stratum::stratum_submit",
                    "[RPC-STRATUM] Error refreshing registry jobs: {e}",
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
        drop(jobs);
        drop(submit_lock);

        miner_status_response(id, "OK")
    }

    // RPCAPI:
    // Miner sends `keepalived` to prevent connection timeout.
    //
    // **Request:**
    // * `id` : Registry client ID
    //
    // **Response:**
    // * `status`: Response status
    //
    // --> {"jsonrpc": "2.0", "method": "keepalived", "params": {"id": "foo"}, "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {"status": "KEEPALIVED"}, "id": 1}
    pub async fn stratum_keepalived(&self, id: u16, params: JsonValue) -> JsonResult {
        // Parse request params
        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        // Parse client id
        let Some(client_id) = params.get("id") else {
            return server_error(RpcError::MinerMissingClientId, id, None)
        };
        let Some(client_id) = client_id.get::<String>() else {
            return server_error(RpcError::MinerInvalidClientId, id, None)
        };

        // If we don't know about this client job, we can just abort here
        if !self.registry.jobs.read().await.contains_key(client_id) {
            return server_error(RpcError::MinerUnknownClient, id, None)
        };

        // Respond with keepalived message
        miner_status_response(id, "KEEPALIVED")
    }
}
