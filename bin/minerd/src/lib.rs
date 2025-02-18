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

use std::{collections::HashSet, sync::Arc};

use log::{error, info};
use smol::{
    channel::{Receiver, Sender},
    lock::Mutex,
};

use darkfi::{
    rpc::{
        server::{listen_and_serve, RequestHandler},
        settings::RpcSettings,
    },
    system::{ExecutorPtr, StoppableTask, StoppableTaskPtr},
    Error, Result,
};

/// Daemon error codes
mod error;

/// JSON-RPC server methods
mod rpc;

/// Atomic pointer to the DarkFi mining node
pub type MinerNodePtr = Arc<MinerNode>;

/// Structure representing a DarkFi mining node
pub struct MinerNode {
    /// PoW miner number of threads to use
    threads: usize,
    /// Sender to stop miner threads
    sender: Sender<()>,
    /// Receiver to stop miner threads
    stop_signal: Receiver<()>,
    /// JSON-RPC connection tracker
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
}

impl MinerNode {
    pub fn new(threads: usize, sender: Sender<()>, stop_signal: Receiver<()>) -> MinerNodePtr {
        Arc::new(Self { threads, sender, stop_signal, rpc_connections: Mutex::new(HashSet::new()) })
    }
}

/// Atomic pointer to the DarkFi mining daemon
pub type MinerdPtr = Arc<Minerd>;

/// Structure representing a DarkFi mining daemon
pub struct Minerd {
    /// Miner node instance conducting the mining operations
    node: MinerNodePtr,
    /// JSON-RPC background task
    rpc_task: StoppableTaskPtr,
}

impl Minerd {
    /// Initialize a DarkFi mining daemon.
    ///
    /// Corresponding communication channels are setup to generate a new `MinerNode`,
    /// and a new task is generated to handle the JSON-RPC API.
    pub fn init(threads: usize) -> MinerdPtr {
        info!(target: "minerd::Minerd::init", "Initializing a new mining daemon...");

        // Initialize the smol channels to send signal between the threads
        let (sender, stop_signal) = smol::channel::bounded(1);

        // Generate the node
        let node = MinerNode::new(threads, sender, stop_signal);

        // Generate the JSON-RPC task
        let rpc_task = StoppableTask::new();

        info!(target: "minerd::Minerd::init", "Mining daemon initialized successfully!");

        Arc::new(Self { node, rpc_task })
    }

    /// Start the DarkFi mining daemon in the given executor, using the provided JSON-RPC listen url.
    pub fn start(&self, executor: &ExecutorPtr, rpc_settings: &RpcSettings) {
        info!(target: "minerd::Minerd::start", "Starting mining daemon...");

        // Start the JSON-RPC task
        let node_ = self.node.clone();
        self.rpc_task.clone().start(
            listen_and_serve(rpc_settings.clone(), self.node.clone(), None, executor.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::RpcServerStopped) => node_.stop_connections().await,
                    Err(e) => error!(target: "minerd::Minerd::start", "Failed starting JSON-RPC server: {}", e),
                }
            },
            Error::RpcServerStopped,
            executor.clone(),
        );

        info!(target: "minerd::Minerd::start", "Mining daemon started successfully!");
    }

    /// Stop the DarkFi mining daemon.
    pub async fn stop(&self) -> Result<()> {
        info!(target: "minerd::Minerd::stop", "Terminating mining daemon...");

        // Stop the mining node
        info!(target: "minerd::Minerd::stop", "Stopping miner threads...");
        self.node.sender.send(()).await?;

        // Stop the JSON-RPC task
        info!(target: "minerd::Minerd::stop", "Stopping JSON-RPC server...");
        self.rpc_task.stop().await;

        // Consume channel item so its empty again
        if self.node.stop_signal.is_full() {
            self.node.stop_signal.recv().await?;
        }

        info!(target: "minerd::Minerd::stop", "Mining daemon terminated successfully!");
        Ok(())
    }
}


#[cfg(test)]
use url::Url;

#[test]
/// Test the programmatic control of `Minerd`.
///
/// First we initialize a daemon, start it and then perform
/// couple of restarts to verify everything works as expected.
fn minerd_programmatic_control() -> Result<()> {
    // Initialize logger
    let mut cfg = simplelog::ConfigBuilder::new();

    // We check this error so we can execute same file tests in parallel,
    // otherwise second one fails to init logger here.
    if simplelog::TermLogger::init(
        simplelog::LevelFilter::Info,
        //simplelog::LevelFilter::Debug,
        //simplelog::LevelFilter::Trace,
        cfg.build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )
    .is_err()
    {
        log::debug!(target: "minerd_programmatic_control", "Logger initialized");
    }

    // Daemon configuration
    let threads = 4;
    let rpc_settings = RpcSettings {
        listen: Url::parse("tcp://127.0.0.1:28467")?,
        ..RpcSettings::default()
    };

    // Create an executor and communication signals
    let ex = Arc::new(smol::Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    // Generate a dummy mining job
    let target = darkfi::rpc::util::JsonValue::String(
        num_bigint::BigUint::from_bytes_be(&[0xFF; 32]).to_string(),
    );
    let block = darkfi::rpc::util::JsonValue::String(darkfi::util::encoding::base64::encode(
        &darkfi_serial::serialize(&darkfi::blockchain::BlockInfo::default()),
    ));
    let mining_job = darkfi::rpc::jsonrpc::JsonRequest::new(
        "mine",
        darkfi::rpc::util::JsonValue::Array(vec![target, block]),
    );

    easy_parallel::Parallel::new()
        .each(0..threads, |_| smol::block_on(ex.run(shutdown.recv())))
        .finish(|| {
            smol::block_on(async {
                // Initialize a daemon
                let daemon = Minerd::init(threads);

                // Start it
                daemon.start(&ex, &rpc_settings);

                // Generate a JSON-RPC client to send mining jobs
                let mut rpc_client =
                    darkfi::rpc::client::RpcClient::new(rpc_settings.listen.clone(), ex.clone()).await;
                while rpc_client.is_err() {
                    rpc_client =
                        darkfi::rpc::client::RpcClient::new(rpc_settings.listen.clone(), ex.clone()).await;
                }
                let rpc_client = rpc_client.unwrap();

                // Send a mining job but stop the daemon after it starts mining
                smol::future::or(
                    async {
                        rpc_client.request(mining_job).await.unwrap();
                    },
                    async {
                        // Wait node to start mining
                        darkfi::system::sleep(2).await;
                        daemon.stop().await.unwrap();
                    },
                )
                .await;
                rpc_client.stop().await;

                // Start it again
                daemon.start(&ex, &rpc_settings);

                // Stop it
                daemon.stop().await.unwrap();

                // Shutdown entirely
                drop(signal);
            })
        });

    Ok(())
}
