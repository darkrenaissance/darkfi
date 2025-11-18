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
use tracing::{debug, error, info, warn};
use url::Url;

use darkfi::{
    blockchain::BlockInfo,
    net::settings::Settings,
    rpc::{
        jsonrpc::JsonSubscriber,
        server::{listen_and_serve, RequestHandler},
        settings::RpcSettings,
    },
    system::{ExecutorPtr, StoppableTask, StoppableTaskPtr},
    validator::{
        consensus::Fork, utils::best_fork_index, Validator, ValidatorConfig, ValidatorPtr,
    },
    zk::{empty_witnesses, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Error, Result,
};
use darkfi_money_contract::MONEY_CONTRACT_ZKAS_MINT_NS_V1;
use darkfi_sdk::crypto::{keypair::SecretKey, MONEY_CONTRACT_ID};

#[cfg(test)]
mod tests;

mod error;
use error::{server_error, RpcError};

/// JSON-RPC requests handler and methods
mod rpc;
use rpc::{DefaultRpcHandler, MinerRpcClient, MmRpcHandler};
mod rpc_blockchain;
mod rpc_tx;
mod rpc_xmr;
use rpc_xmr::BlockTemplateHash;

/// Validator async tasks
pub mod task;
use task::{consensus::ConsensusInitTaskConfig, consensus_init_task};

/// P2P net protocols
mod proto;
use proto::{DarkfidP2pHandler, DarkfidP2pHandlerPtr};

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
    /// HTTP JSON-RPC connection tracker
    mm_rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    /// Merge mining block templates
    mm_blocktemplates: Mutex<HashMap<BlockTemplateHash, (BlockInfo, SecretKey)>>,
    /// PowRewardV1 ZK data
    powrewardv1_zk: PowRewardV1Zk,
}

impl DarkfiNode {
    pub async fn new(
        p2p_handler: DarkfidP2pHandlerPtr,
        validator: ValidatorPtr,
        txs_batch_size: usize,
        subscribers: HashMap<&'static str, JsonSubscriber>,
        rpc_client: Option<Mutex<MinerRpcClient>>,
    ) -> Result<DarkfiNodePtr> {
        let powrewardv1_zk = PowRewardV1Zk::new(validator.clone())?;

        Ok(Arc::new(Self {
            p2p_handler,
            validator,
            txs_batch_size,
            subscribers,
            rpc_connections: Mutex::new(HashSet::new()),
            rpc_client,
            mm_rpc_connections: Mutex::new(HashSet::new()),
            mm_blocktemplates: Mutex::new(HashMap::new()),
            powrewardv1_zk,
        }))
    }

    /// Grab best current fork
    pub async fn best_current_fork(&self) -> Result<Fork> {
        let forks = self.validator.consensus.forks.read().await;
        let index = best_fork_index(&forks)?;
        forks[index].full_clone()
    }
}

/// ZK data used to generate the "coinbase" transaction in a block
pub(crate) struct PowRewardV1Zk {
    pub zkbin: ZkBinary,
    pub provingkey: ProvingKey,
}

impl PowRewardV1Zk {
    pub fn new(validator: ValidatorPtr) -> Result<Self> {
        info!(
            target: "darkfid::PowRewardV1Zk::new",
            "Generating PowRewardV1 ZkCircuit and ProvingKey...",
        );

        let (zkbin, _) = validator.blockchain.contracts.get_zkas(
            &validator.blockchain.sled_db,
            &MONEY_CONTRACT_ID,
            MONEY_CONTRACT_ZKAS_MINT_NS_V1,
        )?;

        let circuit = ZkCircuit::new(empty_witnesses(&zkbin)?, &zkbin);
        let provingkey = ProvingKey::build(zkbin.k, &circuit);

        Ok(Self { zkbin, provingkey })
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
    /// HTTP JSON-RPC background task
    mm_rpc_task: StoppableTaskPtr,
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
                Some(Mutex::new(MinerRpcClient::new(endpoint.clone(), ex.clone()).await))
            }
            None => None,
        };

        // Initialize node
        let node = DarkfiNode::new(p2p_handler, validator, txs_batch_size, subscribers, rpc_client)
            .await?;

        // Generate the background tasks
        let dnet_task = StoppableTask::new();
        let rpc_task = StoppableTask::new();
        let mm_rpc_task = StoppableTask::new();
        let consensus_task = StoppableTask::new();

        info!(target: "darkfid::Darkfid::init", "Darkfi daemon initialized successfully!");

        Ok(Arc::new(Self { node, dnet_task, rpc_task, mm_rpc_task, consensus_task }))
    }

    /// Start the DarkFi daemon in the given executor, using the provided JSON-RPC listen url
    /// and consensus initialization configuration.
    pub async fn start(
        &self,
        executor: &ExecutorPtr,
        rpc_settings: &RpcSettings,
        mm_rpc_settings: &Option<RpcSettings>,
        config: &ConsensusInitTaskConfig,
    ) -> Result<()> {
        info!(target: "darkfid::Darkfid::start", "Starting Darkfi daemon...");

        // Pinging minerd daemon to verify it listens
        if self.node.rpc_client.is_some() {
            if let Err(e) = self.node.ping_miner_daemon().await {
                warn!(target: "darkfid::Darkfid::start", "Failed to ping miner daemon: {e}");
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

        // Start the JSON-RPC task
        info!(target: "darkfid::Darkfid::start", "Starting JSON-RPC server");
        let node_ = self.node.clone();
        self.rpc_task.clone().start(
            listen_and_serve::<DefaultRpcHandler>(rpc_settings.clone(), self.node.clone(), None, executor.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::RpcServerStopped) => <DarkfiNode as RequestHandler<DefaultRpcHandler>>::stop_connections(&node_).await,
                    Err(e) => error!(target: "darkfid::Darkfid::start", "Failed starting JSON-RPC server: {e}"),
                }
            },
            Error::RpcServerStopped,
            executor.clone(),
        );

        // Start the HTTP JSON-RPC task
        if let Some(mm_rpc) = mm_rpc_settings {
            info!(target: "darkfid::Darkfid::start", "Starting HTTP JSON-RPC server");
            let node_ = self.node.clone();
            self.mm_rpc_task.clone().start(
                listen_and_serve::<MmRpcHandler>(mm_rpc.clone(), self.node.clone(), None, executor.clone()),
                |res| async move {
                    match res {
                        Ok(()) | Err(Error::RpcServerStopped) => <DarkfiNode as RequestHandler<MmRpcHandler>>::stop_connections(&node_).await,
                        Err(e) => error!(target: "darkfid::Darkfid::start", "Failed starting HTTP JSON-RPC server: {e}"),
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

        // Stop the JSON-RPC task
        info!(target: "darkfid::Darkfid::stop", "Stopping JSON-RPC server...");
        self.rpc_task.stop().await;

        // Stop the HTTP JSON-RPC task
        info!(target: "darkfid::Darkfid::stop", "Stopping HTTP JSON-RPC server...");
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
        info!(target: "darkfid::Darkfid::stop", "Flushed {flushed_bytes} bytes");

        // Close the JSON-RPC client, if it was initialized
        if let Some(ref rpc_client) = self.node.rpc_client {
            info!(target: "darkfid::Darkfid::stop", "Stopping JSON-RPC client...");
            rpc_client.lock().await.stop().await;
        };

        info!(target: "darkfid::Darkfid::stop", "Darkfi daemon terminated successfully!");
        Ok(())
    }
}
