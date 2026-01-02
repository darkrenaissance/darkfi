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

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use smol::lock::Mutex;
use tracing::{debug, error, info};

use darkfi::{
    net::settings::Settings,
    rpc::{
        jsonrpc::JsonSubscriber,
        server::{listen_and_serve, RequestHandler},
        settings::RpcSettings,
    },
    system::{ExecutorPtr, StoppableTask, StoppableTaskPtr},
    validator::{Validator, ValidatorConfig, ValidatorPtr},
    Error, Result,
};
use darkfi_sdk::crypto::keypair::Network;

#[cfg(test)]
mod tests;

mod error;
use error::{server_error, RpcError};

/// JSON-RPC requests handler and methods
mod rpc;
use rpc::{management::ManagementRpcHandler, DefaultRpcHandler};

/// Validator async tasks
pub mod task;
use task::{consensus::ConsensusInitTaskConfig, consensus_init_task};

/// P2P net protocols
mod proto;
use proto::{DarkfidP2pHandler, DarkfidP2pHandlerPtr};

/// Miners registry
mod registry;
use registry::{DarkfiMinersRegistry, DarkfiMinersRegistryPtr};

/// Atomic pointer to the DarkFi node
pub type DarkfiNodePtr = Arc<DarkfiNode>;

/// Structure representing a DarkFi node
pub struct DarkfiNode {
    /// Validator(node) pointer
    validator: ValidatorPtr,
    /// P2P network protocols handler
    p2p_handler: DarkfidP2pHandlerPtr,
    /// Node miners registry pointer
    registry: DarkfiMinersRegistryPtr,
    /// Garbage collection task transactions batch size
    txs_batch_size: usize,
    /// A map of various subscribers exporting live info from the blockchain
    subscribers: HashMap<&'static str, JsonSubscriber>,
    /// Main JSON-RPC connection tracker
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    /// Management JSON-RPC connection tracker
    management_rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
}

impl DarkfiNode {
    pub async fn new(
        validator: ValidatorPtr,
        p2p_handler: DarkfidP2pHandlerPtr,
        registry: DarkfiMinersRegistryPtr,
        txs_batch_size: usize,
        subscribers: HashMap<&'static str, JsonSubscriber>,
    ) -> Result<DarkfiNodePtr> {
        Ok(Arc::new(Self {
            validator,
            p2p_handler,
            registry,
            txs_batch_size,
            subscribers,
            rpc_connections: Mutex::new(HashSet::new()),
            management_rpc_connections: Mutex::new(HashSet::new()),
        }))
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
    /// Main JSON-RPC background task
    rpc_task: StoppableTaskPtr,
    /// Management JSON-RPC background task
    management_rpc_task: StoppableTaskPtr,
    /// Consensus protocol background task
    consensus_task: StoppableTaskPtr,
}

impl Darkfid {
    /// Initialize a DarkFi daemon.
    ///
    /// Generates a new `DarkfiNode` for provided configuration,
    /// along with all the corresponding background tasks.
    pub async fn init(
        network: Network,
        sled_db: &sled_overlay::sled::Db,
        config: &ValidatorConfig,
        net_settings: &Settings,
        txs_batch_size: &Option<usize>,
        ex: &ExecutorPtr,
    ) -> Result<DarkfidPtr> {
        info!(target: "darkfid::Darkfid::init", "Initializing a Darkfi daemon...");
        // Initialize validator
        let validator = Validator::new(sled_db, config).await?;

        // Initialize P2P network
        let p2p_handler = DarkfidP2pHandler::init(net_settings, ex).await?;

        // Initialize the miners registry
        let registry = DarkfiMinersRegistry::init(network, &validator)?;

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

        // Initialize node
        let node =
            DarkfiNode::new(validator, p2p_handler, registry, txs_batch_size, subscribers).await?;

        // Generate the background tasks
        let dnet_task = StoppableTask::new();
        let rpc_task = StoppableTask::new();
        let management_rpc_task = StoppableTask::new();
        let consensus_task = StoppableTask::new();

        info!(target: "darkfid::Darkfid::init", "Darkfi daemon initialized successfully!");

        Ok(Arc::new(Self { node, dnet_task, rpc_task, management_rpc_task, consensus_task }))
    }

    /// Start the DarkFi daemon in the given executor, using the
    /// provided JSON-RPC settings and consensus initialization
    /// configuration.
    pub async fn start(
        &self,
        executor: &ExecutorPtr,
        rpc_settings: &RpcSettings,
        management_rpc_settings: &RpcSettings,
        stratum_rpc_settings: &Option<RpcSettings>,
        mm_rpc_settings: &Option<RpcSettings>,
        config: &ConsensusInitTaskConfig,
    ) -> Result<()> {
        info!(target: "darkfid::Darkfid::start", "Starting Darkfi daemon...");

        // Start the `dnet` task
        info!(target: "darkfid::Darkfid::start", "Starting dnet subs task");
        let dnet_sub_ = self.node.subscribers.get("dnet").unwrap().clone();
        let p2p_ = self.node.p2p_handler.p2p.clone();
        self.dnet_task.clone().start(
            async move {
                let dnet_sub = p2p_.dnet_subscribe().await;
                loop {
                    let event = dnet_sub.receive().await;
                    debug!(target: "darkfid::Darkfid::dnet_task", "Got dnet event: {event:?}");
                    dnet_sub_.notify(vec![event.into()].into()).await;
                }
            },
            |res| async {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "darkfid::Darkfid::start", "Failed starting dnet subs task: {e}"),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        // Start the main JSON-RPC task
        info!(target: "darkfid::Darkfid::start", "Starting main JSON-RPC server");
        let node_ = self.node.clone();
        self.rpc_task.clone().start(
            listen_and_serve::<DefaultRpcHandler>(rpc_settings.clone(), self.node.clone(), None, executor.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::RpcServerStopped) => <DarkfiNode as RequestHandler<DefaultRpcHandler>>::stop_connections(&node_).await,
                    Err(e) => error!(target: "darkfid::Darkfid::start", "Failed starting main JSON-RPC server: {e}"),
                }
            },
            Error::RpcServerStopped,
            executor.clone(),
        );

        // Start the management JSON-RPC task
        info!(target: "darkfid::Darkfid::start", "Starting management JSON-RPC server");
        let node_ = self.node.clone();
        self.management_rpc_task.clone().start(
            listen_and_serve::<ManagementRpcHandler>(management_rpc_settings.clone(), self.node.clone(), None, executor.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::RpcServerStopped) => <DarkfiNode as RequestHandler<ManagementRpcHandler>>::stop_connections(&node_).await,
                    Err(e) => error!(target: "darkfid::Darkfid::start", "Failed starting management JSON-RPC server: {e}"),
                }
            },
            Error::RpcServerStopped,
            executor.clone(),
        );

        // Start the miners registry
        info!(target: "darkfid::Darkfid::start", "Starting miners registry");
        self.node.registry.start(executor, &self.node, stratum_rpc_settings, mm_rpc_settings)?;

        // Start the P2P network
        info!(target: "darkfid::Darkfid::start", "Starting P2P network");
        self.node.p2p_handler.start(executor, &self.node.validator, &self.node.subscribers).await?;

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
                    Err(e) => error!(target: "darkfid::Darkfid::start", "Failed starting consensus initialization task: {e}"),
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

        // Stop the main JSON-RPC task
        info!(target: "darkfid::Darkfid::stop", "Stopping main JSON-RPC server...");
        self.rpc_task.stop().await;

        // Stop the management JSON-RPC task
        info!(target: "darkfid::Darkfid::stop", "Stopping management JSON-RPC server...");
        self.management_rpc_task.stop().await;

        // Stop the miners registry
        info!(target: "darkfid::Darkfid::stop", "Stopping miners registry...");
        self.node.registry.stop().await;

        // Stop the P2P network
        info!(target: "darkfid::Darkfid::stop", "Stopping P2P network protocols handler...");
        self.node.p2p_handler.stop().await;

        // Stop the consensus task
        info!(target: "darkfid::Darkfid::stop", "Stopping consensus task...");
        self.consensus_task.stop().await;

        // Flush sled database data
        info!(target: "darkfid::Darkfid::stop", "Flushing sled database...");
        let flushed_bytes = self.node.validator.blockchain.sled_db.flush_async().await?;
        info!(target: "darkfid::Darkfid::stop", "Flushed {flushed_bytes} bytes");

        info!(target: "darkfid::Darkfid::stop", "Darkfi daemon terminated successfully!");
        Ok(())
    }
}
