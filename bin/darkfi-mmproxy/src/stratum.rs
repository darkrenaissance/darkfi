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

use std::{collections::HashMap, io, str::FromStr, sync::Arc, time::Duration};

use darkfi::{
    rpc::{
        jsonrpc::{
            ErrorCode::{InternalError, InvalidParams, ServerError},
            JsonError, JsonRequest, JsonResponse, JsonResult, JsonSubscriber,
        },
        util::JsonValue,
    },
    system::{sleep, timeout::timeout, StoppableTask, StoppableTaskPtr},
    Error, Result,
};
use log::{debug, error, info, warn};
use monero::blockdata::transaction::{ExtraField, RawExtraField, SubField::MergeMining};
use smol::{channel, lock::RwLock};
use url::Url;
use uuid::Uuid;

use super::{error::RpcError, MiningProxy};

/// Algo string representing Monero's RandomX
pub const RANDOMX_ALGO: &str = "rx/0";

/// A mining job instance
#[derive(Clone)]
struct MiningJob {
    /// Current job ID for the worker
    pub job_id: blake3::Hash,
    /// Full block being mined
    pub block: monero::Block,
    /// Difficulty target,
    pub target: String,
    /// Block height
    pub height: f64,
    /// RandomX seed hash
    pub seed_hash: String,
}

/// Single worker connected to the mining proxy
pub struct Worker {
    /// Wallet address
    addr: monero::Address,
    /// Miner useragent
    _agent: String,
    /// JSON-RPC notification subscriber, used to send new job notifications
    job_sub: JsonSubscriber,
    /// Background keepalive task reference
    _ka_task: StoppableTaskPtr,
    /// Keepalive sender channel, pinged from Stratum keepalived
    ka_send: channel::Sender<()>,
    /// Background mining job task reference
    _job_task: StoppableTaskPtr,
    /// Block submit trigger sender channel, pinged from Stratum submit
    submit_send: channel::Sender<()>,
    /// Current mining job
    mining_job: MiningJob,
}

impl Worker {
    async fn notify_job(&mut self, mining_job: MiningJob) -> Result<()> {
        // Update the mining job
        self.mining_job = mining_job.clone();

        // Build notification params
        let params: JsonValue = JsonValue::Object(HashMap::from([
            ("blob".to_string(), hex::encode(mining_job.block.serialize_hashable()).into()),
            ("job_id".to_string(), mining_job.job_id.to_string().into()),
            ("target".to_string(), mining_job.target.into()),
            ("height".to_string(), mining_job.height.into()),
            ("seed_hash".to_string(), mining_job.seed_hash.into()),
            ("algo".to_string(), RANDOMX_ALGO.to_string().into()),
        ]));

        info!(
            target: "worker::notify_job",
            "[STRATUM] Sending mining job notification to worker",
        );
        self.job_sub.notify(params).await;
        Ok(())
    }
}

/// Send a HTTP JSON-RPC request to the given monerod RPC endpoint
async fn monerod_request(endpoint: &Url, req: JsonRequest) -> Result<JsonValue> {
    let client = surf::Client::new();

    let mut response = match client
        .get(endpoint)
        .header("Content-Type", "application/json")
        .body(req.stringify().unwrap())
        .send()
        .await
    {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "stratum::monerod_request",
                "[STRATUM] Failed sending RPC request to monerod: {}", e,
            );
            return Err(io::Error::new(io::ErrorKind::Other, e).into())
        }
    };

    let response_bytes = match response.body_bytes().await {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "stratum::monerod_request",
                "[STRATUM] Failed reading monerod RPC response: {}", e,
            );
            return Err(io::Error::new(io::ErrorKind::Other, e).into())
        }
    };

    let response_string = match String::from_utf8(response_bytes) {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "stratum::monerod_request",
                "[STRATUM] Failed parsing monerod RPC response: {}", e,
            );
            return Err(io::Error::new(io::ErrorKind::Other, e).into())
        }
    };

    let response_json: JsonValue = match response_string.parse() {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "stratum::monerod_request",
                "[STRATUM] Failed parsing monerod RPC response JSON: {}", e,
            );
            return Err(io::Error::new(io::ErrorKind::Other, e).into())
        }
    };

    Ok(response_json)
}

/// Perform getblocktemplate from monerod and inject it with the
/// necessary merge mining data.
/// Returns data necessary to create a mining job
async fn getblocktemplate(endpoint: &Url, wallet_address: &monero::Address) -> Result<MiningJob> {
    // Create the Merge Mining Tag: (`depth`, `merkle_root`)
    let mm_tag = MergeMining(Some(monero::VarInt(32)), monero::Hash([0_u8; 32]));

    // Construct `tx_extra` from all the extra fields we have to
    // add to the coinbase transaction in the block we're mining
    let tx_extra: RawExtraField = ExtraField(vec![mm_tag]).into();

    // Create the monerod JSON-RPC request. `reserve_size` is the space
    // we need to create for the `tx_extra` field created above.
    let req = JsonRequest::new(
        "get_block_template",
        HashMap::from([
            ("wallet_address".to_string(), wallet_address.to_string().into()),
            ("reserve_size".to_string(), (tx_extra.0.len() as f64).into()),
        ])
        .into(),
    );

    // Get block template from monerod
    let rep = match monerod_request(endpoint, req).await {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "stratum::getblocktemplate",
                "[STRATUM] Failed sending getblocktemplate to monerod: {}", e,
            );
            return Err(io::Error::new(io::ErrorKind::Other, e).into())
        }
    };

    // Now we have to modify the block template:
    // * Update the coinbase tx with our tx_extra field
    // * Update the `blockhashing_blob` in order to perform correct PoW

    // Deserialize the block template
    let mut block_template = monero::consensus::deserialize::<monero::Block>(
        &hex::decode(rep["result"]["blocktemplate_blob"].get::<String>().unwrap()).unwrap(),
    )
    .unwrap();

    // Modify the coinbase tx with our additional merge mining data
    block_template.miner_tx.prefix.extra = tx_extra;

    // Get the difficulty target
    let target = rep["result"]["wide_difficulty"]
        .get::<String>()
        .unwrap()
        .strip_prefix("0x")
        .unwrap()
        .to_string();

    // Get the remaining metadata
    let height = *rep["result"]["height"].get::<f64>().unwrap();
    let seed_hash = rep["result"]["seed_hash"].get::<String>().unwrap().to_string();

    // Create a deterministic job id
    let mut hasher = blake3::Hasher::new();
    hasher.update(&wallet_address.as_bytes());
    hasher.update(&height.to_le_bytes());
    hasher.update(seed_hash.as_bytes());
    let job_id = hasher.finalize();

    // Return the necessary data
    Ok(MiningJob { job_id, block: block_template, target, height, seed_hash })
}

impl MiningProxy {
    /// Background task listening for keepalives from a worker.
    /// If timeout is reached, the worker will be dropped.
    async fn keepalive_task(
        workers: Arc<RwLock<HashMap<Uuid, Worker>>>,
        uuid: Uuid,
        ka_recv: channel::Receiver<()>,
    ) -> Result<()> {
        debug!(target: "stratum::keepalive_task", "Spawned keepalive_task for worker {}", uuid);
        const TIMEOUT: Duration = Duration::from_secs(65);

        loop {
            let Ok(r) = timeout(TIMEOUT, ka_recv.recv()).await else {
                // Timeout, remove worker
                warn!(
                    target: "stratum::keepalive_task",
                    "keepalive_task for worker {} timed out", uuid,
                );
                workers.write().await.remove(&uuid);
                break
            };

            match r {
                Ok(()) => {
                    debug!(
                        target: "stratum::keepalive_task",
                        "keepalive_task for worker {} got ping", uuid,
                    );
                    continue
                }
                Err(e) => {
                    error!(
                        target: "stratum::keepalive_task",
                        "keepalive_task for worker {} channel recv error: {}", uuid, e,
                    );
                    warn!(
                        target: "stratum::keepalive_task",
                        "Dropping worker {}", uuid,
                    );
                    workers.write().await.remove(&uuid);
                    break
                }
            }
        }

        Ok(())
    }

    /// Background task used to notify a worker about new mining jobs.
    /// `keepalive_task` iis able to remove workers from the worker pool,
    /// so this task can easily exit if the worker is not found.
    async fn job_task(
        workers: Arc<RwLock<HashMap<Uuid, Worker>>>,
        uuid: Uuid,
        endpoint: Url,
        submit_recv: channel::Receiver<()>,
    ) -> Result<()> {
        debug!(target: "stratum::job_task", "Spawned job_task for worker {}", uuid);
        const POLL_INTERVAL: Duration = Duration::from_secs(60);

        // Comfy wait for settling the Stratum login RPC call
        sleep(2).await;

        // In this loop, we'll be getting the block template for mining.
        // At the beginning of the loop, we'll perform a getblocktemplate,
        // and then inject our Merge Mining stuff, and forward it to the
        // miner. After the notification, we'll either poll or wait for a
        // trigger for a submitted block and reiterate the loop again in
        // order to get the next mining job.
        loop {
            // Get the workers lock and the worker reference
            let mut workers_ptr = workers.write().await;
            let Some(worker) = workers_ptr.get_mut(&uuid) else {
                info!(
                    target: "stratum::job_task",
                    "[STRATUM] Worker {} disconnected, exiting job_task", uuid,
                );
                break
            };

            // Get the next mining job
            let mining_job = match getblocktemplate(&endpoint, &worker.addr).await {
                Ok(v) => v,
                Err(e) => {
                    error!(
                        target: "stratum::job_task",
                        "[STRATUM] Failed fetching getblocktemplate for worker {}: {}", uuid, e,
                    );
                    warn!(
                        target: "stratum::job_task",
                        "[STRATUM] Exiting job_task for worker {}", uuid,
                    );
                    break
                }
            };

            // In case it's the same job, we'll wait and try again
            if worker.mining_job.job_id == mining_job.job_id {
                match timeout(POLL_INTERVAL, submit_recv.recv()).await {
                    Ok(_) => continue,
                    Err(_) => continue,
                }
            }

            // Notify the worker about the new job
            if let Err(e) = worker.notify_job(mining_job).await {
                error!(
                    target: "stratum::job_task",
                    "[STRATUM] Failed sending job to worker {}: {}", uuid, e,
                );
                warn!(
                    target: "stratum::job_task",
                    "[STRATUM] Exiting job_task for worker {}", uuid,
                );
                break
            }

            drop(workers_ptr);

            // Now poll or wait for a trigger for a new job.
            match timeout(POLL_INTERVAL, submit_recv.recv()).await {
                Ok(_) => continue,
                Err(_) => continue,
            }
        }

        Ok(())
    }

    /// Stratum login method
    ///
    /// `darkfi-mmproxy` will check that the worker provided a valid
    /// address as the username, and will enforce `RANDOMX_ALGO` to
    /// be supported. Upon success, we will fetch the block template
    /// from monerod, inject it with our necessary merge mining info,
    /// and forward it to the worker.
    /// Additionally, we will spawn background tasks for new job and
    /// keepalive notifications for this worker.
    pub async fn stratum_login(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        if !params.contains_key("login") ||
            !params.contains_key("pass") ||
            !params.contains_key("agent") ||
            !params.contains_key("algo")
        {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let Some(login) = params["login"].get::<String>() else {
            return JsonError::new(InvalidParams, Some("Invalid \"login\" object".to_string()), id)
                .into()
        };

        let Some(_pass) = params["pass"].get::<String>() else {
            return JsonError::new(InvalidParams, Some("Invalid \"pass\" object".to_string()), id)
                .into()
        };

        let Some(agent) = params["agent"].get::<String>() else {
            return JsonError::new(InvalidParams, Some("Invalid \"agent\" object".to_string()), id)
                .into()
        };

        let Some(algos) = params["algo"].get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, Some("Invalid \"algo\" object".to_string()), id)
                .into()
        };

        // We'll only support `RANDOMX_ALGO`
        let mut found_randomx_algo = false;
        for algo in algos.iter() {
            if !algo.is_string() {
                return JsonError::new(InvalidParams, Some("Algo is not a string".to_string()), id)
                    .into()
            }

            if algo.get::<String>().unwrap() == RANDOMX_ALGO {
                found_randomx_algo = true;
                break
            }
        }

        if !found_randomx_algo {
            return JsonError::new(
                RpcError::UnsupportedMiningAlgo.into(),
                Some("Unsupported mining algos".to_string()),
                id,
            )
            .into()
        }

        // Check valid login. We will parse the username as a Monero
        // address, and validate that it corresponds to the network
        // we're mining on.
        let addr = match monero::Address::from_str(login) {
            Ok(v) => v,
            Err(e) => {
                return JsonError::new(
                    RpcError::InvalidWorkerLogin.into(),
                    Some(format!("Invalid Monero address login: {}", e)),
                    id,
                )
                .into()
            }
        };

        if addr.network != self.monerod_network {
            return JsonError::new(
                RpcError::InvalidWorkerLogin.into(),
                Some(format!(
                    "Invalid Monero address network, expected \"{:?}\"",
                    self.monerod_network
                )),
                id,
            )
            .into()
        }

        if addr.addr_type != monero::AddressType::Standard {
            return JsonError::new(
                RpcError::InvalidWorkerLogin.into(),
                Some(format!(
                    "Invalid Monero address type, expected \"{}\"",
                    monero::AddressType::Standard
                )),
                id,
            )
            .into()
        }

        // Now we have a valid address for mining.
        // Create a new UUID for the worker, and initialize the `Worker`
        // struct that will live throughout the miner's lifetime.
        let worker_uuid = Uuid::new_v4();

        // Create job subscriber
        let job_sub = JsonSubscriber::new("job");

        // Create keepalive channel
        let (ka_send, ka_recv) = channel::unbounded();

        // Create submit trigger channel
        let (submit_send, submit_recv) = channel::unbounded();

        // Create background keepalive task
        let ka_task = StoppableTask::new();

        // Create background job task
        let job_task = StoppableTask::new();

        // Get the current mining job for the worker
        let mining_job = match getblocktemplate(&self.monerod_rpc, &addr).await {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "stratum::login",
                    "[STRATUM] Failed fetching block template for worker: {}", e,
                );
                return JsonError::new(InternalError, None, id).into()
            }
        };

        // Create worker
        let worker = Worker {
            addr,
            _agent: agent.clone(),
            job_sub: job_sub.clone(),
            _ka_task: ka_task.clone(),
            ka_send,
            _job_task: job_task.clone(),
            submit_send,
            mining_job: mining_job.clone(),
        };

        // Insert the worker into connections map
        self.workers.write().await.insert(worker_uuid, worker);

        // Spawn keepalive background task
        ka_task.start(
            Self::keepalive_task(self.workers.clone(), worker_uuid, ka_recv),
            move |_| async move { debug!("keepalive_task for {} exited", worker_uuid) },
            Error::DetachedTaskStopped,
            self.executor.clone(),
        );

        // Spawn job notification background task
        job_task.start(
            Self::job_task(
                self.workers.clone(),
                worker_uuid,
                self.monerod_rpc.clone(),
                submit_recv,
            ),
            move |_| async move { debug!("job_task for {} exited", worker_uuid) },
            Error::DetachedTaskStopped,
            self.executor.clone(),
        );

        info!("[STRATUM] Added worker {}", worker_uuid);

        // Finally, we return the job notification subscriber, along with the
        // initial job response as noted in:
        // https://github.com/xmrig/xmrig-proxy/blob/master/doc/STRATUM.md#example-success-reply
        let blob = hex::encode(mining_job.block.serialize_hashable());
        let response = JsonResponse::new(
            HashMap::from([
                ("status".to_string(), "OK".to_string().into()),
                ("id".to_string(), worker_uuid.to_string().into()),
                (
                    "extensions".to_string(),
                    vec!["algo".to_string().into(), "keepalive".to_string().into()].into(),
                ),
                (
                    "job".to_string(),
                    HashMap::from([
                        ("blob".to_string(), blob.into()),
                        ("job_id".to_string(), mining_job.job_id.to_string().into()),
                        ("target".to_string(), mining_job.target.to_string().into()),
                        ("height".to_string(), mining_job.height.into()),
                        ("seed_hash".to_string(), mining_job.seed_hash.to_string().into()),
                        ("algo".to_string(), RANDOMX_ALGO.to_string().into()),
                    ])
                    .into(),
                ),
            ])
            .into(),
            id,
        );

        JsonResult::SubscriberWithReply(job_sub, response)
    }

    /// Stratum submit method
    ///
    /// The miner submits the request after a share was found.
    pub async fn stratum_submit(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        if !params.contains_key("id") ||
            !params.contains_key("job_id") ||
            !params.contains_key("nonce") ||
            !params.contains_key("result")
        {
            return JsonError::new(InvalidParams, None, id).into()
        }

        // Validate all the parameters
        let Some(worker_uuid) = params["id"].get::<String>() else {
            return JsonError::new(InvalidParams, Some("Invalid \"id\" field".to_string()), id)
                .into()
        };

        let Ok(worker_uuid) = Uuid::try_from(worker_uuid.as_str()) else {
            return JsonError::new(InvalidParams, Some("Invalid \"id\" field".to_string()), id)
                .into()
        };

        let Some(job_id) = params["job_id"].get::<String>() else {
            return JsonError::new(InvalidParams, Some("Invalid \"job_id\" field".to_string()), id)
                .into()
        };

        let Ok(job_id) = blake3::Hash::from_str(job_id) else {
            return JsonError::new(InvalidParams, Some("Invalid \"job_id\" field".to_string()), id)
                .into()
        };

        let Some(nonce) = params["nonce"].get::<String>() else {
            return JsonError::new(InvalidParams, Some("Invalid \"nonce\" field".to_string()), id)
                .into()
        };

        let Ok(nonce) = u32::from_str_radix(nonce, 16) else {
            return JsonError::new(InvalidParams, Some("Invalid \"nonce\" field".to_string()), id)
                .into()
        };

        let Some(_result) = params["result"].get::<String>() else {
            return JsonError::new(InvalidParams, Some("Invalid \"result\" field".to_string()), id)
                .into()
        };

        // Get the worker reference and confirm this is submitted for the current job
        let workers_ptr = self.workers.read().await;
        let Some(worker) = workers_ptr.get(&worker_uuid) else {
            return JsonError::new(InvalidParams, Some("Unknown worker UUID".to_string()), id).into()
        };

        if worker.mining_job.job_id != job_id {
            return JsonError::new(InvalidParams, Some("Job ID mismatch".to_string()), id).into()
        }

        // Get the block template from the worker reference and update the nonce
        let mut block_template = worker.mining_job.block.clone();
        block_template.header.nonce = nonce;

        // Submit the block to monerod
        let block = monero::consensus::serialize_hex(&block_template);
        let params: JsonValue = vec![block.into()].into();
        let req = JsonRequest::new("submit_block", params);

        let resp = match monerod_request(&self.monerod_rpc, req).await {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "stratum::submit",
                    "[STRATUM] Failed submitting block to monerod: {}", e,
                );
                return JsonError::new(
                    InternalError,
                    Some("Failed submitting block".to_string()),
                    id,
                )
                .into()
            }
        };

        // Ping the job_task to reiterate.
        // We don't release the lock after this, so that we can hopefully first
        // return the result of the `submit` call, and then unlock job_task for
        // the new notification.
        let _ = worker.submit_send.send(()).await;

        match JsonResult::try_from_value(&resp) {
            Ok(JsonResult::Response(r)) => {
                info!(
                    target: "stratum::submit",
                    "[STRATUM] Sucessfully submitted block to monerod: {:?}", r,
                );

                let result = HashMap::from([("status".to_string(), "OK".to_string().into())]);
                JsonResponse::new(result.into(), id).into()
            }
            Ok(JsonResult::Error(e)) => {
                error!(
                    target: "stratum::submit",
                    "[STRATUM] Failed submitting block to monerod: {:?}", e,
                );
                JsonError::new(ServerError(e.error.code), Some(e.error.message), id).into()
            }
            Ok(x) => {
                error!(
                    target: "stratum::submit",
                    "[STRATUM] Unexpected RPC reply from monerod: {:?}", x,
                );
                JsonError::new(InternalError, Some("Failed submitting block".to_string()), id)
                    .into()
            }
            Err(e) => {
                error!(
                    target: "stratum::submit",
                    "[STRATUM] Unexpected RPC reply from monerod: {}", e,
                );
                JsonError::new(InternalError, Some("Failed submitting block".to_string()), id)
                    .into()
            }
        }
    }

    /// Nonstandard, but widely supported protocol extension.
    /// The miner sends `keepalived` to prevent connection timeout.
    /// `darkfi-mmproxy` makes having keepalived mandatory.
    pub async fn stratum_keepalived(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        if !params.contains_key("id") {
            return JsonError::new(InvalidParams, Some("Missing \"id\" field".to_string()), id)
                .into()
        };

        let Some(worker_uuid) = params["id"].get::<String>() else {
            return JsonError::new(InvalidParams, Some("Invalid \"id\" field".to_string()), id)
                .into()
        };

        let Ok(worker_uuid) = Uuid::try_from(worker_uuid.as_str()) else {
            return JsonError::new(InvalidParams, Some("Invalid \"id\" field".to_string()), id)
                .into()
        };

        // Get the worker reference
        let workers_ptr = self.workers.read().await;
        let Some(worker) = workers_ptr.get(&worker_uuid) else {
            return JsonError::new(InvalidParams, Some("Invalid \"id\" field".to_string()), id)
                .into()
        };

        // Ping the keepalive task
        if let Err(e) = worker.ka_send.send(()).await {
            error!(
                target: "stratum::keepalived",
                "[STRATUM] Keepalive task ping error for {}: {}", worker_uuid, e,
            );
            return JsonError::new(InternalError, None, id).into()
        }

        JsonResponse::new(
            JsonValue::Object(HashMap::from([(
                "status".to_string(),
                "KEEPALIVED".to_string().into(),
            )])),
            id,
        )
        .into()
    }
}
