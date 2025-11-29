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

use num_bigint::BigUint;
use tracing::{debug, error, info};
use url::Url;

use darkfi::{
    blockchain::{Header, HeaderHash},
    rpc::{client::RpcClient, jsonrpc::JsonRequest, util::JsonValue},
    system::{sleep, ExecutorPtr, StoppableTask},
    util::encoding::base64,
    validator::pow::mine_block,
    Error, Result,
};
use darkfi_serial::deserialize_async;

use crate::{MinerNode, MinerNodePtr};

/// Structure to hold a JSON-RPC client and its config,
/// so we can recreate it in case of an error.
pub struct DarkfidRpcClient {
    endpoint: Url,
    ex: ExecutorPtr,
    client: Option<RpcClient>,
}

impl DarkfidRpcClient {
    pub async fn new(endpoint: Url, ex: ExecutorPtr) -> Self {
        let client = RpcClient::new(endpoint.clone(), ex.clone()).await.ok();
        Self { endpoint, ex, client }
    }

    /// Stop the client.
    pub async fn stop(&self) {
        if let Some(ref client) = self.client {
            client.stop().await
        }
    }
}

impl MinerNode {
    /// Auxiliary function to poll configured darkfid daemon for a new
    /// mining job.
    async fn poll(&self, header: &str) -> Result<(HeaderHash, BigUint, Header)> {
        loop {
            debug!(target: "minerd::rpc::poll", "Executing poll request to darkfid...");
            let mut request_params = self.config.wallet_config.clone();
            request_params.insert(String::from("header"), JsonValue::String(String::from(header)));
            let params = match self
                .darkfid_daemon_request("miner.get_header", &JsonValue::from(request_params))
                .await
            {
                Ok(params) => params,
                Err(e) => {
                    error!(target: "minerd::rpc::poll", "darkfid poll failed: {e}");
                    self.sleep().await?;
                    continue
                }
            };
            debug!(target: "minerd::rpc::poll", "Got reply: {params:?}");

            // Verify response parameters
            if !params.is_array() {
                error!(target: "minerd::rpc::poll", "darkfid responded with invalid params: {params:?}");
                self.sleep().await?;
                continue
            }
            let params = params.get::<Vec<JsonValue>>().unwrap();
            if params.is_empty() {
                debug!(target: "minerd::rpc::poll", "darkfid response is empty");
                self.sleep().await?;
                continue
            }
            if params.len() != 3 ||
                !params[0].is_string() ||
                !params[1].is_string() ||
                !params[2].is_string()
            {
                error!(target: "minerd::rpc::poll", "darkfid responded with invalid params: {params:?}");
                self.sleep().await?;
                continue
            }

            // Parse parameters
            let Some(randomx_key_bytes) = base64::decode(params[0].get::<String>().unwrap()) else {
                error!(target: "minerd::rpc::poll", "Failed to parse RandomX key bytes");
                self.sleep().await?;
                continue
            };
            let Ok(randomx_key) = deserialize_async::<HeaderHash>(&randomx_key_bytes).await else {
                error!(target: "minerd::rpc::poll", "Failed to parse RandomX key");
                self.sleep().await?;
                continue
            };
            let Some(target_bytes) = base64::decode(params[1].get::<String>().unwrap()) else {
                error!(target: "minerd::rpc::poll", "Failed to parse target bytes");
                self.sleep().await?;
                continue
            };
            let target = BigUint::from_bytes_le(&target_bytes);
            let Some(header_bytes) = base64::decode(params[2].get::<String>().unwrap()) else {
                error!(target: "minerd::rpc::poll", "Failed to parse header bytes");
                self.sleep().await?;
                continue
            };
            let Ok(header) = deserialize_async::<Header>(&header_bytes).await else {
                error!(target: "minerd::rpc::poll", "Failed to parse header");
                self.sleep().await?;
                continue
            };

            return Ok((randomx_key, target, header))
        }
    }

    /// Auxiliary function to submit a mining solution to configured
    /// darkfid daemon.
    async fn submit(&self, nonce: f64) -> String {
        debug!(target: "minerd::rpc::submit", "Executing submit request to darkfid...");
        let mut request_params = self.config.wallet_config.clone();
        request_params.insert(String::from("nonce"), JsonValue::Number(nonce));
        let result = match self
            .darkfid_daemon_request("miner.submit_solution", &JsonValue::from(request_params))
            .await
        {
            Ok(result) => result,
            Err(e) => return format!("darkfid submit failed: {e}"),
        };
        debug!(target: "minerd::rpc::submit", "Got reply: {result:?}");

        // Parse response
        match result.get::<String>() {
            Some(result) => result.clone(),
            None => format!("darkfid responded with invalid params: {result:?}"),
        }
    }

    /// Auxiliary function to execute a request towards the configured
    /// darkfid daemon JSON-RPC endpoint.
    async fn darkfid_daemon_request(&self, method: &str, params: &JsonValue) -> Result<JsonValue> {
        let mut lock = self.rpc_client.write().await;
        let req = JsonRequest::new(method, params.clone());

        // Check the client is initialized
        if let Some(ref client) = lock.client {
            // Execute request
            if let Ok(rep) = client.request(req.clone()).await {
                drop(lock);
                return Ok(rep);
            }
        }

        // Reset the rpc client in case of an error and try again
        let client = RpcClient::new(lock.endpoint.clone(), lock.ex.clone()).await?;
        let rep = client.request(req).await?;
        lock.client = Some(client);
        drop(lock);
        Ok(rep)
    }

    /// Auxiliary function to stop current JSON-RPC client, if its
    /// initialized.
    pub async fn stop_rpc_client(&self) {
        self.rpc_client.read().await.stop().await;
    }

    /// Auxiliary function to sleep for configured polling rate time.
    async fn sleep(&self) -> Result<()> {
        // Check if stop signal is received
        if self.stop_signal.is_full() {
            debug!(target: "minerd::rpc::sleep", "Stop signal received, exiting polling task");
            return Err(Error::DetachedTaskStopped);
        }
        debug!(target: "minerd::rpc::sleep", "Sleeping for {} until next poll...", self.config.polling_rate);
        sleep(self.config.polling_rate).await;
        Ok(())
    }
}

/// Async task to poll darkfid for new mining jobs. Once a new job is
/// received, spawns a mining task in the background.
pub async fn polling_task(miner: MinerNodePtr, ex: ExecutorPtr) -> Result<()> {
    // Initialize a dummy Header to use on first poll
    let mut current_job = Header::default().hash().to_string();
    loop {
        // Poll darkfid for a mining job
        let (randomx_key, target, header) = miner.poll(&current_job).await?;
        let header_hash = header.hash().to_string();
        debug!(target: "minerd::rpc::polling_task", "Received job:");
        debug!(target: "minerd::rpc::polling_task", "\tRandomX key - {randomx_key}");
        debug!(target: "minerd::rpc::polling_task", "\tTarget - {target}");
        debug!(target: "minerd::rpc::polling_task", "\tHeader - {header_hash}");

        // Check if we are already processing this job
        if header_hash == current_job {
            debug!(target: "minerd::rpc::polling_task", "Already received job, skipping...");
            miner.sleep().await?;
            continue
        }

        // Check if we reached the stop height
        if miner.config.stop_at_height > 0 && header.height > miner.config.stop_at_height {
            info!(target: "minerd::rpc::polling_task", "Reached requested mining height: {}", miner.config.stop_at_height);
            info!(target: "minerd::rpc::polling_task", "Daemon can be safely terminated now!");
            break
        }

        info!(target: "minerd::rpc::polling_task", "Received new job to mine block header {header_hash} with key {randomx_key} for target: 0x{target:064x}");

        // Abord pending job
        miner.abort_pending().await;

        // Detach mining task
        StoppableTask::new().start(
            mining_task(miner.clone(), randomx_key, target, header),
            |res| async {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "minerd::rpc::polling_task", "Failed starting mining task: {e}"),
                }
            },
            Error::DetachedTaskStopped,
            ex.clone(),
        );

        // Update current job
        current_job = header_hash;

        // Sleep until next poll
        miner.sleep().await?;
    }

    Ok(())
}

/// Async task to mine provided header and submit solution to darkfid.
async fn mining_task(
    miner: MinerNodePtr,
    randomx_key: HeaderHash,
    target: BigUint,
    mut header: Header,
) -> Result<()> {
    // Mine provided block header
    let header_hash = header.hash().to_string();
    info!(target: "minerd::rpc::mining_task", "Mining block header {header_hash} with key {randomx_key} for target: 0x{target:064x}");
    if let Err(e) = mine_block(
        &randomx_key,
        &target,
        &mut header,
        miner.config.threads,
        &miner.stop_signal.clone(),
    ) {
        error!(target: "minerd::rpc::mining_task", "Failed mining block header {header_hash} with error: {e}");
        return Err(Error::DetachedTaskStopped)
    }
    info!(target: "minerd::rpc::mining_task", "Mined block header {header_hash} with nonce: {}", header.nonce);
    info!(target: "minerd::rpc::mining_task", "Mined block header hash: {}", header.hash());

    // Submit solution to darkfid
    info!(target: "minerd::rpc::submit", "Submitting solution to darkfid...");
    let result = miner.submit(header.nonce as f64).await;
    info!(target: "minerd::rpc::submit", "Submition result: {result}");

    Ok(())
}
