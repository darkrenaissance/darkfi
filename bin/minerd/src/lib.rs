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

use std::{collections::HashMap, sync::Arc};

use smol::{
    channel::{Receiver, Sender},
    lock::RwLock,
};
use tracing::{debug, error, info};
use url::Url;

use darkfi::{
    rpc::util::JsonValue,
    system::{sleep, ExecutorPtr, StoppableTask, StoppableTaskPtr},
    Error,
};
use darkfi_sdk::crypto::Keypair;

/// darkfid JSON-RPC related methods
mod rpc;
use rpc::{polling_task, DarkfidRpcClient};

/// Auxiliary structure representing miner node configuration.
pub struct MinerNodeConfig {
    /// PoW miner number of threads to use
    threads: usize,
    /// Polling rate to ask darkfid for mining jobs
    polling_rate: u64,
    /// Stop mining at this height (0 mines forever)
    stop_at_height: u32,
    /// Wallet mining configuration to receive mining rewards
    wallet_config: HashMap<String, JsonValue>,
}

impl Default for MinerNodeConfig {
    fn default() -> Self {
        Self::new(
            1,
            5,
            0,
            HashMap::from([(
                String::from("recipient"),
                JsonValue::String(Keypair::default().public.to_string()),
            )]),
        )
    }
}

impl MinerNodeConfig {
    pub fn new(
        threads: usize,
        polling_rate: u64,
        stop_at_height: u32,
        wallet_config: HashMap<String, JsonValue>,
    ) -> Self {
        Self { threads, polling_rate, stop_at_height, wallet_config }
    }
}

/// Atomic pointer to the DarkFi mining node
pub type MinerNodePtr = Arc<MinerNode>;

/// Structure representing a DarkFi mining node
pub struct MinerNode {
    /// Node configuration
    config: MinerNodeConfig,
    /// Sender to stop miner threads
    sender: Sender<()>,
    /// Receiver to stop miner threads
    stop_signal: Receiver<()>,
    /// JSON-RPC client to execute requests to darkfid daemon
    rpc_client: RwLock<DarkfidRpcClient>,
}

impl MinerNode {
    pub async fn new(config: MinerNodeConfig, endpoint: Url, ex: &ExecutorPtr) -> MinerNodePtr {
        // Initialize the smol channels to send signal between the threads
        let (sender, stop_signal) = smol::channel::bounded(1);

        // Initialize JSON-RPC client
        let rpc_client = RwLock::new(DarkfidRpcClient::new(endpoint, ex.clone()).await);

        Arc::new(Self { config, sender, stop_signal, rpc_client })
    }

    /// Auxiliary function to abort pending job.
    pub async fn abort_pending(&self) {
        // Check if a pending request is being processed
        debug!(target: "minerd::abort_pending", "Checking if a pending job is being processed...");
        if self.stop_signal.receiver_count() <= 1 {
            debug!(target: "minerd::rpc", "No pending job!");
            return
        }

        info!(target: "minerd::abort_pending", "Pending job is in progress, sending stop signal...");
        // Send stop signal to worker
        if let Err(e) = self.sender.try_send(()) {
            error!(target: "minerd::abort_pending", "Failed to stop pending job: {e}");
            return
        }

        // Wait for worker to terminate
        info!(target: "minerd::abort_pending", "Waiting for job to terminate...");
        while self.stop_signal.receiver_count() > 1 {
            sleep(1).await;
        }
        info!(target: "minerd::abort_pending", "Pending job terminated!");

        // Consume channel item so its empty again
        if let Err(e) = self.stop_signal.try_recv() {
            error!(target: "minerd::abort_pending", "Failed to cleanup stop signal channel: {e}");
        }
    }
}

/// Atomic pointer to the DarkFi mining daemon
pub type MinerdPtr = Arc<Minerd>;

/// Structure representing a DarkFi mining daemon
pub struct Minerd {
    /// Miner node instance conducting the mining operations
    node: MinerNodePtr,
    /// Miner darkfid polling background task
    polling_task: StoppableTaskPtr,
}

impl Minerd {
    /// Initialize a DarkFi mining daemon.
    ///
    /// Generate a new `MinerNode` and a new task to handle the darkfid
    /// polling.
    pub async fn init(config: MinerNodeConfig, endpoint: Url, ex: &ExecutorPtr) -> MinerdPtr {
        info!(target: "minerd::Minerd::init", "Initializing a new mining daemon...");

        // Generate the node
        let node = MinerNode::new(config, endpoint, ex).await;

        // Generate the polling task
        let polling_task = StoppableTask::new();

        info!(target: "minerd::Minerd::init", "Mining daemon initialized successfully!");

        Arc::new(Self { node, polling_task })
    }

    /// Start the DarkFi mining daemon in the given executor.
    pub fn start(&self, ex: &ExecutorPtr) {
        info!(target: "minerd::Minerd::start", "Starting mining daemon...");

        // Start the polling task
        self.polling_task.clone().start(
            polling_task(self.node.clone(), ex.clone()),
            |res| async {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => {
                        error!(target: "minerd::Minerd::start", "Failed starting polling task: {e}")
                    }
                }
            },
            Error::DetachedTaskStopped,
            ex.clone(),
        );

        info!(target: "minerd::Minerd::start", "Mining daemon started successfully!");
    }

    /// Stop the DarkFi mining daemon.
    pub async fn stop(&self) {
        info!(target: "minerd::Minerd::stop", "Terminating mining daemon...");

        // Stop the polling task
        info!(target: "minerd::Minerd::stop", "Stopping polling task...");
        self.polling_task.stop().await;

        // Stop the mining node
        info!(target: "minerd::Minerd::stop", "Stopping miner threads...");
        self.node.abort_pending().await;

        // Close the JSON-RPC client
        info!(target: "minerd::Minerd::stop", "Stopping JSON-RPC client...");
        self.node.stop_rpc_client().await;

        info!(target: "minerd::Minerd::stop", "Mining daemon terminated successfully!");
    }
}

#[cfg(test)]
use {
    darkfi::util::logger::{setup_test_logger, Level},
    tracing::warn,
};

#[test]
/// Test the programmatic control of `Minerd`.
///
/// First we initialize a daemon, start it and then perform
/// couple of restarts to verify everything works as expected.
fn minerd_programmatic_control() {
    // We check this error so we can execute same file tests in parallel,
    // otherwise second one fails to init logger here.
    if setup_test_logger(
        &[],
        false,
        Level::Info,
        //Level::Verbose,
        //Level::Debug,
        //Level::Trace,
    )
    .is_err()
    {
        warn!(target: "minerd_programmatic_control", "Logger already initialized");
    }

    // Create an executor and communication signals
    let ex = Arc::new(smol::Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new().each(0..1, |_| smol::block_on(ex.run(shutdown.recv()))).finish(
        || {
            smol::block_on(async {
                // Initialize a daemon
                let daemon = Minerd::init(
                    MinerNodeConfig::default(),
                    Url::parse("tcp://127.0.0.1:12345").unwrap(),
                    &ex,
                )
                .await;

                // Start it
                daemon.start(&ex);

                // Stop it
                daemon.stop().await;

                // Start it again
                daemon.start(&ex);

                // Stop it
                daemon.stop().await;

                // Shutdown entirely
                drop(signal);
            })
        },
    );
}
