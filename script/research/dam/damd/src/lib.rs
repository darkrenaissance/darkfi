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
    Error, Result,
};

/// JSON-RPC server methods
mod rpc;

/// P2P net protocols
mod proto;
use proto::{DamP2pHandler, DamP2pHandlerPtr};

/// P2P network flooder
mod flooder;
use flooder::{DamFlooder, DamFlooderPtr};

/// Atomic pointer to the Denial-of-service Analysis Multitool node
pub type DamNodePtr = Arc<DamNode>;

/// Structure representing a Denial-of-service Analysis Multitool node
pub struct DamNode {
    /// P2P network protocols handler.
    p2p_handler: DamP2pHandlerPtr,
    /// A map of various subscribers exporting live info from the node
    subscribers: HashMap<&'static str, JsonSubscriber>,
    /// JSON-RPC connection tracker
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    /// Network flooder
    flooder: DamFlooderPtr,
}

impl DamNode {
    pub async fn new(
        p2p_handler: DamP2pHandlerPtr,
        subscribers: HashMap<&'static str, JsonSubscriber>,
        flooder: DamFlooderPtr,
    ) -> DamNodePtr {
        Arc::new(Self {
            p2p_handler,
            subscribers,
            rpc_connections: Mutex::new(HashSet::new()),
            flooder,
        })
    }
}

/// Atomic pointer to the Denial-of-service Analysis Multitool daemon
pub type DamdPtr = Arc<Damd>;

/// Structure representing a Denial-of-service Analysis Multitool daemon
pub struct Damd {
    /// Darkfi node instance
    node: DamNodePtr,
    /// `dnet` background task
    dnet_task: StoppableTaskPtr,
    /// JSON-RPC background task
    rpc_task: StoppableTaskPtr,
}

impl Damd {
    /// Initialize a Denial-of-service Analysis Multitool daemon.
    ///
    /// Generates a new `DamNode` for provided configuration,
    /// along with all the corresponding background tasks.
    pub async fn init(net_settings: &Settings, ex: &ExecutorPtr) -> Result<DamdPtr> {
        info!(target: "damd::Damd::init", "Initializing a Denial-of-service Analysis Multitool daemon...");

        // Initialize P2P network
        let p2p_handler = DamP2pHandler::init(net_settings, ex).await?;

        // Here we initialize various subscribers that can export live network data.
        let mut subscribers = HashMap::new();
        subscribers.insert("dnet", JsonSubscriber::new("dnet.subscribe_events"));
        subscribers.insert("foo", JsonSubscriber::new("protocols.subscribe_foo"));
        subscribers.insert("attack_foo", JsonSubscriber::new("protocols.subscribe_attack_foo"));
        subscribers.insert("bar", JsonSubscriber::new("protocols.subscribe_bar"));
        subscribers.insert("attack_bar", JsonSubscriber::new("protocols.subscribe_attack_bar"));

        // Initialize flooder
        let flooder = DamFlooder::init(&p2p_handler.p2p, ex);

        // Initialize node
        let node = DamNode::new(p2p_handler, subscribers, flooder).await;

        // Generate the background tasks
        let dnet_task = StoppableTask::new();
        let rpc_task = StoppableTask::new();

        info!(target: "damd::Damd::init", "Denial-of-service Analysis Multitool daemon initialized successfully!");

        Ok(Arc::new(Self { node, dnet_task, rpc_task }))
    }

    /// Start the Denial-of-service Analysis Multitool daemon in the given executor,
    /// using the provided JSON-RPC configuration.
    pub async fn start(&self, executor: &ExecutorPtr, rpc_settings: &RpcSettings) -> Result<()> {
        info!(target: "damd::Damd::start", "Starting Denial-of-service Analysis Multitool daemon...");

        // Start the `dnet` task
        info!(target: "damd::Damd::start", "Starting dnet subs task");
        let dnet_sub_ = self.node.subscribers.get("dnet").unwrap().clone();
        let p2p_ = self.node.p2p_handler.p2p.clone();
        self.dnet_task.clone().start(
            async move {
                let dnet_sub = p2p_.dnet_subscribe().await;
                loop {
                    let event = dnet_sub.receive().await;
                    debug!(target: "damd::Damd::dnet_task", "Got dnet event: {:?}", event);
                    dnet_sub_.notify(vec![event.into()].into()).await;
                }
            },
            |res| async {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => {
                        error!(target: "damd::Damd::start", "Failed starting dnet subs task: {}", e)
                    }
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        // Start the JSON-RPC task
        info!(target: "damd::Damd::start", "Starting JSON-RPC server");
        let node_ = self.node.clone();
        self.rpc_task.clone().start(
            listen_and_serve(rpc_settings.clone(), self.node.clone(), None, executor.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::RpcServerStopped) => node_.stop_connections().await,
                    Err(e) => error!(target: "damd::Damd::start", "Failed starting JSON-RPC server: {}", e),
                }
            },
            Error::RpcServerStopped,
            executor.clone(),
        );

        // Start the P2P network
        info!(target: "damd::Damd::start", "Starting P2P network");
        self.node.p2p_handler.clone().start(executor, &self.node.subscribers).await?;

        info!(target: "damd::Damd::start", "Denial-of-service Analysis Multitool daemon started successfully!");
        Ok(())
    }

    /// Stop the Denial-of-service Analysis Multitool daemon.
    pub async fn stop(&self) -> Result<()> {
        info!(target: "damd::Damd::stop", "Terminating Denial-of-service Analysis Multitool daemon...");

        // Stop the flooder
        info!(target: "damd::Damd::stop", "Stopping the flooder...");
        self.node.flooder.stop().await;

        // Stop the `dnet` node
        info!(target: "damd::Damd::stop", "Stopping dnet subs task...");
        self.dnet_task.stop().await;

        // Stop the JSON-RPC task
        info!(target: "damd::Damd::stop", "Stopping JSON-RPC server...");
        self.rpc_task.stop().await;

        // Stop the P2P network
        info!(target: "damd::Damd::stop", "Stopping P2P network protocols handler...");
        self.node.p2p_handler.stop().await;

        info!(target: "damd::Damd::stop", "Denial-of-service Analysis Multitool daemon terminated successfully!");
        Ok(())
    }
}
