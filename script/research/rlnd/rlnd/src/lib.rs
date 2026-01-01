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

use std::{collections::HashSet, sync::Arc};

use smol::lock::Mutex;
use tracing::{error, info};
use url::Url;

use darkfi::{
    rpc::{
        server::{listen_and_serve, RequestHandler},
        settings::RpcSettings,
    },
    system::{ExecutorPtr, StoppableTask, StoppableTaskPtr},
    Error, Result,
};

pub mod error;

/// DarkFi RLN state management database
pub mod database;
use database::RlndDatabase;

/// rlnd JSON-RPC related methods
pub mod rpc;
use rpc::{DarkircRpcClient, PrivateRpcHandler, PublicRpcHandler};

/// Atomic pointer to the DarkFi RLN state management node
pub type RlnNodePtr = Arc<RlnNode>;

/// Structure representing a DarkFi RLN state management node
pub struct RlnNode {
    /// Main pointer to the sled db connection
    database: RlndDatabase,
    /// Private JSON-RPC connection tracker
    private_rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    /// Publicly exposed JSON-RPC connection tracker
    public_rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    /// JSON-RPC client to execute requests to the darkirc daemon
    rpc_client: Mutex<DarkircRpcClient>,
}

impl RlnNode {
    pub async fn new(database: RlndDatabase, rpc_client: Mutex<DarkircRpcClient>) -> RlnNodePtr {
        Arc::new(Self {
            database,
            private_rpc_connections: Mutex::new(HashSet::new()),
            public_rpc_connections: Mutex::new(HashSet::new()),
            rpc_client,
        })
    }
}

/// Atomic pointer to the DarkFi RLN state management daemon
pub type RlndPtr = Arc<Rlnd>;

/// Structure representing a DarkFi RLN state management daemon
pub struct Rlnd {
    /// Darkfi RLN state management node instance
    node: RlnNodePtr,
    /// Private JSON-RPC background task
    private_rpc_task: StoppableTaskPtr,
    /// Publicly exposed JSON-RPC background task
    public_rpc_task: StoppableTaskPtr,
}

impl Rlnd {
    /// Initialize a DarkFi RLN state management daemon.
    ///
    /// Generates a new `RlnNode` for provided configuration,
    /// along with all the corresponding background tasks.
    pub async fn init(db_path: &str, endpoint: &Url, ex: &ExecutorPtr) -> Result<RlndPtr> {
        info!(target: "rlnd::Rlnd::init", "Initializing a Darkfi RLN state management daemon...");

        // Initialize database
        let database = RlndDatabase::new(db_path)?;

        // Initialize JSON-RPC client to perform requests to darkirc
        let Ok(rpc_client) = DarkircRpcClient::new(endpoint.clone(), ex.clone()).await else {
            error!(target: "rlnd::Rlnd::init", "Failed to initialize darkirc daemon rpc client, check if darkirc is running");
            return Err(Error::RpcClientStopped)
        };
        let rpc_client = Mutex::new(rpc_client);

        // Initialize node
        let node = RlnNode::new(database, rpc_client).await;

        // Generate the background tasks
        let private_rpc_task = StoppableTask::new();
        let public_rpc_task = StoppableTask::new();

        info!(target: "rlnd::Rlnd::init", "Darkfi RLN state management daemon initialized successfully!");

        Ok(Arc::new(Self { node, private_rpc_task, public_rpc_task }))
    }

    /// Start the DarkFi RLN state management daemon in the given executor, using the provided
    /// JSON-RPC configurations.
    pub async fn start(
        &self,
        executor: &ExecutorPtr,
        private_rpc_settings: &RpcSettings,
        public_rpc_settings: &RpcSettings,
    ) -> Result<()> {
        info!(target: "rlnd::Rlnd::start", "Starting Darkfi RLN state management daemon...");

        // Pinging darkirc daemon to verify it listens
        if let Err(e) = self.node.ping_darkirc_daemon().await {
            error!(target: "rlnd::Rlnd::start", "Failed to ping darkirc daemon: {}", e);
            return Err(Error::RpcClientStopped)
        }

        // Start the private JSON-RPC task
        info!(target: "rlnd::Rlnd::start", "Starting private JSON-RPC server");
        let node_ = self.node.clone();
        self.private_rpc_task.clone().start(
            listen_and_serve::<PrivateRpcHandler>(private_rpc_settings.clone(), self.node.clone(), None, executor.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::RpcServerStopped) => <RlnNode as RequestHandler<PrivateRpcHandler>>::stop_connections(&node_).await,
                    Err(e) => error!(target: "rlnd::Rlnd::start", "Failed starting private JSON-RPC server: {}", e),
                }
            },
            Error::RpcServerStopped,
            executor.clone(),
        );

        // Start the publicly exposed JSON-RPC task
        info!(target: "rlnd::Rlnd::start", "Starting publicly exposed JSON-RPC server");
        let node_ = self.node.clone();
        self.public_rpc_task.clone().start(
            listen_and_serve::<PublicRpcHandler>(public_rpc_settings.clone(), self.node.clone(), None, executor.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::RpcServerStopped) => <RlnNode as RequestHandler<PublicRpcHandler>>::stop_connections(&node_).await,
                    Err(e) => error!(target: "rlnd::Rlnd::start", "Failed starting publicly exposed JSON-RPC server: {}", e),
                }
            },
            Error::RpcServerStopped,
            executor.clone(),
        );

        info!(target: "rlnd::Rlnd::start", "Darkfi RLN state management daemon started successfully!");
        Ok(())
    }

    /// Stop the DarkFi RLN state management daemon.
    pub async fn stop(&self) -> Result<()> {
        info!(target: "rlnd::Rlnd::stop", "Terminating Darkfi RLN state management daemon...");

        // Stop the JSON-RPC task
        info!(target: "rlnd::Rlnd::stop", "Stopping private JSON-RPC server...");
        self.private_rpc_task.stop().await;

        // Stop the JSON-RPC task
        info!(target: "rlnd::Rlnd::stop", "Stopping publicly exposed JSON-RPC server...");
        self.public_rpc_task.stop().await;

        // Flush sled database data
        info!(target: "rlnd::Rlnd::stop", "Flushing sled database...");
        let flushed_bytes = self.node.database.sled_db.flush_async().await?;
        info!(target: "rlnd::Rlnd::stop", "Flushed {} bytes", flushed_bytes);

        // Close the JSON-RPC client, if it was initialized
        info!(target: "rlnd::Rlnd::stop", "Stopping JSON-RPC client...");
        self.node.rpc_client.lock().await.stop().await;

        info!(target: "rlnd::Rlnd::stop", "Darkfi RLN state management daemon terminated successfully!");
        Ok(())
    }
}
