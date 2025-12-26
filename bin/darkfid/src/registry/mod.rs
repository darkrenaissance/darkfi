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
use tracing::{error, info};

use darkfi::{
    rpc::{
        server::{listen_and_serve, RequestHandler},
        settings::RpcSettings,
    },
    system::{ExecutorPtr, StoppableTask, StoppableTaskPtr},
    validator::ValidatorPtr,
    Error, Result,
};

use crate::{DarkfiNode, DarkfiNodePtr, MmRpcHandler, StratumRpcHandler};

/// Block related structures
pub mod model;
use model::{BlockTemplate, MiningJobs, MmBlockTemplate, PowRewardV1Zk};

/// Atomic pointer to the DarkFi node miners registry.
pub type DarkfiMinersRegistryPtr = Arc<DarkfiMinersRegistry>;

/// DarkFi node miners registry.
pub struct DarkfiMinersRegistry {
    /// PowRewardV1 ZK data
    pub powrewardv1_zk: PowRewardV1Zk,
    /// Native mining block templates
    pub blocktemplates: Mutex<HashMap<Vec<u8>, BlockTemplate>>,
    /// Active native mining jobs per connection ID
    pub mining_jobs: Mutex<HashMap<[u8; 32], MiningJobs>>,
    /// Merge mining block templates
    pub mm_blocktemplates: Mutex<HashMap<Vec<u8>, MmBlockTemplate>>,
    /// Stratum JSON-RPC background task
    stratum_rpc_task: StoppableTaskPtr,
    /// Stratum JSON-RPC connection tracker
    pub stratum_rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    /// HTTP JSON-RPC background task
    mm_rpc_task: StoppableTaskPtr,
    /// HTTP JSON-RPC connection tracker
    pub mm_rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
}

impl DarkfiMinersRegistry {
    /// Initialize a DarkFi node miners registry.
    pub fn init(validator: &ValidatorPtr) -> Result<DarkfiMinersRegistryPtr> {
        info!(
            target: "darkfid::registry::mod::DarkfiMinersRegistry::init",
            "Initializing a new DarkFi node miners registry..."
        );

        // Generate the PowRewardV1 ZK data
        let powrewardv1_zk = PowRewardV1Zk::new(validator)?;

        // Generate the stratum JSON-RPC background task and its
        // connections tracker.
        let stratum_rpc_task = StoppableTask::new();
        let stratum_rpc_connections = Mutex::new(HashSet::new());

        // Generate the HTTP JSON-RPC background task and its
        // connections tracker.
        let mm_rpc_task = StoppableTask::new();
        let mm_rpc_connections = Mutex::new(HashSet::new());

        info!(
            target: "darkfid::registry::mod::DarkfiMinersRegistry::init",
            "DarkFi node miners registry generated successfully!"
        );

        Ok(Arc::new(Self {
            powrewardv1_zk,
            blocktemplates: Mutex::new(HashMap::new()),
            mining_jobs: Mutex::new(HashMap::new()),
            mm_blocktemplates: Mutex::new(HashMap::new()),
            stratum_rpc_task,
            stratum_rpc_connections,
            mm_rpc_task,
            mm_rpc_connections,
        }))
    }

    /// Start the DarkFi node miners registry for provided DarkFi node
    /// instance.
    pub fn start(
        &self,
        executor: &ExecutorPtr,
        node: &DarkfiNodePtr,
        stratum_rpc_settings: &Option<RpcSettings>,
        mm_rpc_settings: &Option<RpcSettings>,
    ) -> Result<()> {
        info!(
            target: "darkfid::registry::mod::DarkfiMinersRegistry::start",
            "Starting the DarkFi node miners registry..."
        );

        // Start the stratum server JSON-RPC task
        if let Some(stratum_rpc) = stratum_rpc_settings {
            info!(target: "darkfid::registry::mod::DarkfiMinersRegistry::start", "Starting Stratum JSON-RPC server");
            let node_ = node.clone();
            self.stratum_rpc_task.clone().start(
                listen_and_serve::<StratumRpcHandler>(stratum_rpc.clone(), node.clone(), None, executor.clone()),
                |res| async move {
                    match res {
                        Ok(()) | Err(Error::RpcServerStopped) => <DarkfiNode as RequestHandler<StratumRpcHandler>>::stop_connections(&node_).await,
                        Err(e) => error!(target: "darkfid::registry::mod::DarkfiMinersRegistry::start", "Failed starting Stratum JSON-RPC server: {e}"),
                    }
                },
                Error::RpcServerStopped,
                executor.clone(),
            );
        } else {
            // Create a dummy task
            self.stratum_rpc_task.clone().start(
                async { Ok(()) },
                |_| async { /* Do nothing */ },
                Error::RpcServerStopped,
                executor.clone(),
            );
        }

        // Start the merge mining JSON-RPC task
        if let Some(mm_rpc) = mm_rpc_settings {
            info!(target: "darkfid::registry::mod::DarkfiMinersRegistry::start", "Starting merge mining JSON-RPC server");
            let node_ = node.clone();
            self.mm_rpc_task.clone().start(
                listen_and_serve::<MmRpcHandler>(mm_rpc.clone(), node.clone(), None, executor.clone()),
                |res| async move {
                    match res {
                        Ok(()) | Err(Error::RpcServerStopped) => <DarkfiNode as RequestHandler<MmRpcHandler>>::stop_connections(&node_).await,
                        Err(e) => error!(target: "darkfid::registry::mod::DarkfiMinersRegistry::start", "Failed starting merge mining JSON-RPC server: {e}"),
                    }
                },
                Error::RpcServerStopped,
                executor.clone(),
            );
        } else {
            // Create a dummy task
            self.mm_rpc_task.clone().start(
                async { Ok(()) },
                |_| async { /* Do nothing */ },
                Error::RpcServerStopped,
                executor.clone(),
            );
        }

        info!(
            target: "darkfid::registry::mod::DarkfiMinersRegistry::start",
            "DarkFi node miners registry started successfully!"
        );

        Ok(())
    }

    /// Stop the DarkFi node miners registry.
    pub async fn stop(&self) {
        info!(target: "darkfid::registry::mod::DarkfiMinersRegistry::stop", "Terminating DarkFi node miners registry...");

        // Stop the Stratum JSON-RPC task
        info!(target: "darkfid::registry::mod::DarkfiMinersRegistry::stop", "Stopping Stratum JSON-RPC server...");
        self.stratum_rpc_task.stop().await;

        // Stop the merge mining JSON-RPC task
        info!(target: "darkfid::registry::mod::DarkfiMinersRegistry::stop", "Stopping merge mining JSON-RPC server...");
        self.mm_rpc_task.stop().await;

        info!(target: "darkfid::registry::mod::DarkfiMinersRegistry::stop", "DarkFi node miners registry terminated successfully!");
    }
}
