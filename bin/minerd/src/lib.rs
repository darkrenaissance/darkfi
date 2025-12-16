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
use darkfi_sdk::crypto::keypair::{Address, Keypair, Network, StandardAddress};

/// Miner benchmarking related methods
pub mod benchmark;

/// darkfid JSON-RPC related methods
mod rpc;
use rpc::{polling_task, DarkfidRpcClient};

/// Auxiliary structure representing miner node configuration.
pub struct MinerNodeConfig {
    /// Flag indicating whether to mine in fast mode
    fast_mode: bool,
    /// Flag indicating whether to mine with Large Pages
    large_pages: bool,
    /// Flag indicating whether to mine with secure access to JIT memory (if supported)
    secure: bool,
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
        let address: Address =
            StandardAddress::from_public(Network::Mainnet, Keypair::default().public).into();
        Self::new(
            true,
            false,
            false,
            1,
            5,
            0,
            HashMap::from([(String::from("recipient"), JsonValue::String(address.to_string()))]),
        )
    }
}

impl MinerNodeConfig {
    pub fn new(
        fast_mode: bool,
        large_pages: bool,
        secure: bool,
        threads: usize,
        polling_rate: u64,
        stop_at_height: u32,
        wallet_config: HashMap<String, JsonValue>,
    ) -> Self {
        Self {
            fast_mode,
            large_pages,
            secure,
            threads,
            polling_rate,
            stop_at_height,
            wallet_config,
        }
    }
}

/// Atomic pointer to the DarkFi mining node
pub type MinerNodePtr = Arc<MinerNode>;

/// Structure representing a DarkFi mining node
pub struct MinerNode {
    /// Node configuration
    config: MinerNodeConfig,
    /// Sender and receiver to stop mining threads
    mining_channel: (Sender<()>, Receiver<()>),
    /// Sender and receiver to stop background threads
    background_channel: (Sender<()>, Receiver<()>),
    /// JSON-RPC client to execute requests to darkfid daemon
    rpc_client: RwLock<DarkfidRpcClient>,
}

impl MinerNode {
    pub async fn new(config: MinerNodeConfig, endpoint: Url, ex: &ExecutorPtr) -> MinerNodePtr {
        // Initialize the smol channels to send signal between the threads
        let mining_channel = smol::channel::bounded(1);
        let background_channel = smol::channel::bounded(1);

        // Initialize JSON-RPC client
        let rpc_client = RwLock::new(DarkfidRpcClient::new(endpoint, ex.clone()).await);

        Arc::new(Self { config, mining_channel, background_channel, rpc_client })
    }

    /// Auxiliary function to abort all pending tasks.
    pub async fn abort(&self) {
        self.abort_mining().await;
        self.abort_background().await;
    }

    /// Auxiliary function to abort pending mining task.
    pub async fn abort_mining(&self) {
        Self::abort_task(&self.mining_channel.0, &self.mining_channel.1, "mining").await;
    }

    /// Auxiliary function to abort pending background Randomx VMs
    /// generation task.
    pub async fn abort_background(&self) {
        Self::abort_task(&self.background_channel.0, &self.background_channel.1, "VMs generation")
            .await;
    }

    /// Auxiliary function to abort pending task by signaling provided
    /// channels.
    async fn abort_task(sender: &Sender<()>, stop_signal: &Receiver<()>, task: &str) {
        // Check if a pending task is being processed
        debug!(target: "minerd::abort_task", "Checking if a pending {task} task is being processed...");
        if stop_signal.receiver_count() <= 1 {
            debug!(target: "minerd::abort_task", "No pending {task} task!");
            return
        }

        info!(target: "minerd::abort_task", "Pending {task} is in progress, sending stop signal...");
        // Send stop signal to worker
        if let Err(e) = sender.try_send(()) {
            error!(target: "minerd::abort_task", "Failed to stop pending {task} task: {e}");
            return
        }

        // Wait for worker to terminate
        info!(target: "minerd::abort_task", "Waiting for {task} task to terminate...");
        while stop_signal.receiver_count() > 1 {
            sleep(1).await;
        }
        info!(target: "minerd::abort_task", "Pending {task} task terminated!");

        // Consume channel item so its empty again
        if let Err(e) = stop_signal.try_recv() {
            error!(target: "minerd::abort_task", "Failed to cleanup stop signal channel: {e}");
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

        // Stop the mining node
        info!(target: "minerd::Minerd::stop", "Stopping miner background tasks...");
        self.node.abort().await;

        // Stop the polling task
        info!(target: "minerd::Minerd::stop", "Stopping polling task...");
        self.polling_task.stop().await;

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
