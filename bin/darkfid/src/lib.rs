/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use log::{debug, error, info};
use smol::lock::Mutex;
use url::Url;

use darkfi::{
    net::settings::Settings,
    rpc::{
        client::RpcChadClient,
        jsonrpc::JsonSubscriber,
        server::{listen_and_serve, RequestHandler},
    },
    system::{ExecutorPtr, StoppableTask, StoppableTaskPtr},
    validator::{Validator, ValidatorConfig, ValidatorPtr},
    Error, Result,
};

#[cfg(test)]
mod tests;

mod error;
use error::{server_error, RpcError};

/// JSON-RPC requests handler and methods
mod rpc;
mod rpc_blockchain;
mod rpc_tx;

/// Validator async tasks
pub mod task;
use task::{consensus::ConsensusInitTaskConfig, consensus_init_task};

/// P2P net protocols
mod proto;
use proto::{DarkfidP2pHandler, DarkfidP2pHandlerPtr};

/// Structure to hold a JSON-RPC client and its config,
/// so we can recreate it in case of an error.
pub struct MinerRpcClient {
    endpoint: Url,
    ex: ExecutorPtr,
    client: RpcChadClient,
}

impl MinerRpcClient {
    pub async fn new(endpoint: Url, ex: ExecutorPtr) -> Result<Self> {
        let client = RpcChadClient::new(endpoint.clone(), ex.clone()).await?;
        Ok(Self { endpoint, ex, client })
    }
}

/// Atomic pointer to the DarkFi node
pub type DarkfiNodePtr = Arc<DarkfiNode>;

/// Structure representing a DarkFi node
pub struct DarkfiNode {
    /// P2P network protocols handler.
    p2p_handler: DarkfidP2pHandlerPtr,
    /// Validator(node) pointer
    validator: ValidatorPtr,
    /// Garbage collection task transactions batch size
    txs_batch_size: usize,
    /// A map of various subscribers exporting live info from the blockchain
    subscribers: HashMap<&'static str, JsonSubscriber>,
    /// JSON-RPC connection tracker
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    /// JSON-RPC client to execute requests to the miner daemon
    rpc_client: Option<Mutex<MinerRpcClient>>,
}

impl DarkfiNode {
    pub async fn new(
        p2p_handler: DarkfidP2pHandlerPtr,
        validator: ValidatorPtr,
        txs_batch_size: usize,
        subscribers: HashMap<&'static str, JsonSubscriber>,
        rpc_client: Option<Mutex<MinerRpcClient>>,
    ) -> DarkfiNodePtr {
        Arc::new(Self {
            p2p_handler,
            validator,
            txs_batch_size,
            subscribers,
            rpc_connections: Mutex::new(HashSet::new()),
            rpc_client,
        })
    }
}

/// Atomic pointer to the DarkFi daemon
pub type DarkfidPtr = Arc<Darkfid>;

/// Structure representing a DarkFi daemon
pub struct Darkfid {
    /// Darkfi node instance
    node: DarkfiNodePtr,
    /// `dnet` background task
    dnet_task: StoppableTaskPtr,
    /// JSON-RPC background task
    rpc_task: StoppableTaskPtr,
    /// Consensus protocol background task
    consensus_task: StoppableTaskPtr,
}

impl Darkfid {
    /// Initialize a DarkFi daemon.
    ///
    /// Generates a new `DarkfiNode` for provided configuration,
    /// along with all the corresponding background tasks.
    pub async fn init(
        sled_db: &sled_overlay::sled::Db,
        config: &ValidatorConfig,
        net_settings: &Settings,
        minerd_endpoint: &Option<Url>,
        txs_batch_size: &Option<usize>,
        ex: &ExecutorPtr,
    ) -> Result<DarkfidPtr> {
        info!(target: "darkfid::Darkfid::init", "Initializing a Darkfi daemon...");
        // Initialize validator
        let validator = Validator::new(sled_db, config).await?;

        // Initialize P2P network
        let p2p_handler = DarkfidP2pHandler::init(net_settings, ex).await?;

        // Grab blockchain network configured transactions batch size for garbage collection
        let txs_batch_size = match txs_batch_size {
            Some(b) => {
                if *b > 0 {
                    *b
                } else {
                    50
                }
            }
            None => 50,
        };

        // Here we initialize various subscribers that can export live blockchain/consensus data.
        let mut subscribers = HashMap::new();
        subscribers.insert("blocks", JsonSubscriber::new("blockchain.subscribe_blocks"));
        subscribers.insert("txs", JsonSubscriber::new("blockchain.subscribe_txs"));
        subscribers.insert("proposals", JsonSubscriber::new("blockchain.subscribe_proposals"));
        subscribers.insert("dnet", JsonSubscriber::new("dnet.subscribe_events"));

        // Initialize JSON-RPC client to perform requests to minerd
        let rpc_client = match minerd_endpoint {
            Some(endpoint) => {
                let Ok(rpc_client) = MinerRpcClient::new(endpoint.clone(), ex.clone()).await else {
                    error!(target: "darkfid::Darkfid::init", "Failed to initialize miner daemon rpc client, check if minerd is running");
                    return Err(Error::RpcClientStopped)
                };
                Some(Mutex::new(rpc_client))
            }
            None => None,
        };

        // Initialize node
        let node =
            DarkfiNode::new(p2p_handler, validator, txs_batch_size, subscribers, rpc_client).await;

        // Generate the background tasks
        let dnet_task = StoppableTask::new();
        let rpc_task = StoppableTask::new();
        let consensus_task = StoppableTask::new();

        info!(target: "darkfid::Darkfid::init", "Darkfi daemon initialized successfully!");

        Ok(Arc::new(Self { node, dnet_task, rpc_task, consensus_task }))
    }

    /// Start the DarkFi daemon in the given executor, using the provided JSON-RPC listen url
    /// and consensus initialization configuration.
    pub async fn start(
        &self,
        executor: &ExecutorPtr,
        rpc_listen: &Url,
        config: &ConsensusInitTaskConfig,
    ) -> Result<()> {
        info!(target: "darkfid::Darkfid::start", "Starting Darkfi daemon...");

        // Pinging minerd daemon to verify it listens
        if self.node.rpc_client.is_some() {
            if let Err(e) = self.node.ping_miner_daemon().await {
                error!(target: "darkfid::Darkfid::start", "Failed to ping miner daemon: {}", e);
                return Err(Error::RpcClientStopped)
            }
        }

        // Start the `dnet` task
        info!(target: "darkfid::Darkfid::start", "Starting dnet subs task");
        let dnet_sub_ = self.node.subscribers.get("dnet").unwrap().clone();
        let p2p_ = self.node.p2p_handler.p2p.clone();
        self.dnet_task.clone().start(
            async move {
                let dnet_sub = p2p_.dnet_subscribe().await;
                loop {
                    let event = dnet_sub.receive().await;
                    debug!(target: "darkfid::Darkfid::dnet_task", "Got dnet event: {:?}", event);
                    dnet_sub_.notify(vec![event.into()].into()).await;
                }
            },
            |res| async {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "darkfid::Darkfid::start", "Failed starting dnet subs task: {}", e),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        // Start the JSON-RPC task
        info!(target: "darkfid::Darkfid::start", "Starting JSON-RPC server");
        let node_ = self.node.clone();
        self.rpc_task.clone().start(
            listen_and_serve(rpc_listen.clone(), self.node.clone(), None, executor.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::RpcServerStopped) => node_.stop_connections().await,
                    Err(e) => error!(target: "darkfid::Darkfid::start", "Failed starting JSON-RPC server: {}", e),
                }
            },
            Error::RpcServerStopped,
            executor.clone(),
        );

        // Start the P2P network
        info!(target: "darkfid::Darkfid::start", "Starting P2P network");
        self.node
            .p2p_handler
            .clone()
            .start(executor, &self.node.validator, &self.node.subscribers)
            .await?;

        // Start the consensus protocol
        info!(target: "darkfid::Darkfid::start", "Starting consensus protocol task");
        self.consensus_task.clone().start(
            consensus_init_task(
                self.node.clone(),
                config.clone(),
                executor.clone(),
            ),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::ConsensusTaskStopped) | Err(Error::MinerTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "darkfid::Darkfid::start", "Failed starting consensus initialization task: {}", e),
                }
            },
            Error::ConsensusTaskStopped,
            executor.clone(),
        );

        info!(target: "darkfid::Darkfid::start", "Darkfi daemon started successfully!");
        Ok(())
    }

    /// Stop the DarkFi daemon.
    pub async fn stop(&self) -> Result<()> {
        info!(target: "darkfid::Darkfid::stop", "Terminating Darkfi daemon...");

        // Stop the `dnet` node
        info!(target: "darkfid::Darkfid::stop", "Stopping dnet subs task...");
        self.dnet_task.stop().await;

        // Stop the JSON-RPC task
        info!(target: "darkfid::Darkfid::stop", "Stopping JSON-RPC server...");
        self.rpc_task.stop().await;

        // Stop the P2P network
        info!(target: "darkfid::Darkfid::stop", "Stopping P2P network protocols handler...");
        self.node.p2p_handler.stop().await;

        // Stop the consensus task
        info!(target: "darkfid::Darkfid::stop", "Stopping consensus task...");
        self.consensus_task.stop().await;

        // Flush sled database data
        info!(target: "darkfid::Darkfid::stop", "Flushing sled database...");
        let flushed_bytes = self.node.validator.blockchain.sled_db.flush_async().await?;
        info!(target: "darkfid::Darkfid::stop", "Flushed {} bytes", flushed_bytes);

        // Close the JSON-RPC client, if it was initialized
        if let Some(ref rpc_client) = self.node.rpc_client {
            info!(target: "darkfid::Darkfid::stop", "Stopping JSON-RPC client...");
            rpc_client.lock().await.client.stop().await;
        };

        info!(target: "darkfid::Darkfid::stop", "Darkfi daemon terminated successfully!");
        Ok(())
    }
}