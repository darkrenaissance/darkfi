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
use smol::{lock::Mutex, stream::StreamExt};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use url::Url;

use darkfi::{
    async_daemonize,
    blockchain::BlockInfo,
    cli_desc,
    net::{settings::SettingsOpt, P2pPtr},
    rpc::{
        client::RpcChadClient,
        jsonrpc::JsonSubscriber,
        server::{listen_and_serve, RequestHandler},
    },
    system::{StoppableTask, StoppableTaskPtr},
    util::{encoding::base64, path::expand_path},
    validator::{Validator, ValidatorConfig, ValidatorPtr},
    Error, Result,
};
use darkfi_serial::deserialize_async;

#[cfg(test)]
mod tests;

mod error;
use error::{server_error, RpcError};

/// JSON-RPC requests handler and methods
mod rpc;
mod rpc_blockchain;
mod rpc_mm;
mod rpc_tx;

/// Validator async tasks
mod task;
use task::{consensus::ConsensusInitTaskConfig, consensus_init_task};

/// P2P net protocols
mod proto;

/// Utility functions
mod utils;
use utils::{parse_blockchain_config, spawn_p2p};

const CONFIG_FILE: &str = "darkfid_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../darkfid_config.toml");
/// Note:
/// If you change these don't forget to remove their corresponding database folder,
/// since if it already has a genesis block, provided one is ignored.
const GENESIS_BLOCK_LOCALNET: &str = include_str!("../genesis_block_localnet");
const GENESIS_BLOCK_TESTNET: &str = include_str!("../genesis_block_testnet");
const GENESIS_BLOCK_MAINNET: &str = include_str!("../genesis_block_mainnet");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "darkfid", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(short, long, default_value = "testnet")]
    /// Blockchain network to use
    network: String,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

/// Defines a blockchain network configuration.
/// Default values correspond to a local network.
#[derive(Clone, Debug, serde::Deserialize, structopt::StructOpt, structopt_toml::StructOptToml)]
#[structopt()]
pub struct BlockchainNetwork {
    #[structopt(long, default_value = "tcp://127.0.0.1:8240")]
    /// JSON-RPC listen URL
    pub rpc_listen: Url,

    #[structopt(long, default_value = "tcp://127.0.0.1:8241")]
    /// Merge Mining JSON-RPC listen URL
    pub mm_rpc_listen: Url,

    #[structopt(long, default_value = "~/.local/darkfi/darkfid/localnet")]
    /// Path to blockchain database
    pub database: String,

    #[structopt(long, default_value = "3")]
    /// Finalization threshold, denominated by number of blocks
    pub threshold: usize,

    #[structopt(long, default_value = "tcp://127.0.0.1:28467")]
    /// minerd JSON-RPC endpoint
    pub minerd_endpoint: Url,

    #[structopt(long, default_value = "10")]
    /// PoW block production target, in seconds
    pub pow_target: u32,

    #[structopt(long)]
    /// Optional fixed PoW difficulty, used for testing
    pub pow_fixed_difficulty: Option<usize>,

    #[structopt(long)]
    /// Participate in block production
    pub miner: bool,

    #[structopt(long)]
    /// Wallet address to receive mining rewards
    pub recipient: Option<String>,

    #[structopt(long)]
    /// Optional contract spend hook to use in the mining reward
    pub spend_hook: Option<String>,

    #[structopt(long)]
    /// Optional user data to use in the mining reward
    pub user_data: Option<String>,

    #[structopt(long)]
    /// Skip syncing process and start node right away
    pub skip_sync: bool,

    #[structopt(long)]
    /// Disable transaction's fee verification, used for testing
    pub skip_fees: bool,

    #[structopt(long)]
    /// Optional sync checkpoint height
    pub checkpoint_height: Option<u32>,

    #[structopt(long)]
    /// Optional sync checkpoint hash
    pub checkpoint: Option<String>,

    #[structopt(long)]
    /// Optional bootstrap timestamp
    pub bootstrap: Option<u64>,

    #[structopt(long)]
    /// Garbage collection task transactions batch size
    pub txs_batch_size: Option<usize>,

    /// P2P network settings
    #[structopt(flatten)]
    pub net: SettingsOpt,
}

/// Structure to hold a JSON-RPC client and its config,
/// so we can recreate it in case of an error.
pub struct MinerRpcClient {
    endpoint: Url,
    ex: Arc<smol::Executor<'static>>,
    client: RpcChadClient,
}

impl MinerRpcClient {
    pub async fn new(endpoint: Url, ex: Arc<smol::Executor<'static>>) -> Result<Self> {
        let client = RpcChadClient::new(endpoint.clone(), ex.clone()).await?;
        Ok(Self { endpoint, ex, client })
    }
}

/// Daemon structure
pub struct Darkfid {
    /// P2P network pointer
    p2p: P2pPtr,
    /// Validator(node) pointer
    validator: ValidatorPtr,
    /// Flag to specify node is a miner
    miner: bool,
    /// Garbage collection task transactions batch size
    txs_batch_size: usize,
    /// A map of various subscribers exporting live info from the blockchain
    subscribers: HashMap<&'static str, JsonSubscriber>,
    /// JSON-RPC connection tracker
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    /// JSON-RPC client to execute requests to the miner daemon
    rpc_client: Option<Mutex<MinerRpcClient>>,
    /// dnet JSON-RPC subscriber
    dnet_sub: JsonSubscriber,
}

impl Darkfid {
    pub async fn new(
        p2p: P2pPtr,
        validator: ValidatorPtr,
        miner: bool,
        txs_batch_size: usize,
        subscribers: HashMap<&'static str, JsonSubscriber>,
        rpc_client: Option<Mutex<MinerRpcClient>>,
        dnet_sub: JsonSubscriber,
    ) -> Self {
        Self {
            p2p,
            validator,
            miner,
            txs_batch_size,
            subscribers,
            rpc_connections: Mutex::new(HashSet::new()),
            rpc_client,
            dnet_sub,
        }
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    info!(target: "darkfid", "Initializing DarkFi node...");

    // Grab blockchain network configuration
    let (blockchain_config, genesis_block) = match args.network.as_str() {
        "localnet" => {
            (parse_blockchain_config(args.config, "localnet").await?, GENESIS_BLOCK_LOCALNET)
        }
        "testnet" => {
            (parse_blockchain_config(args.config, "testnet").await?, GENESIS_BLOCK_TESTNET)
        }
        "mainnet" => {
            (parse_blockchain_config(args.config, "mainnet").await?, GENESIS_BLOCK_MAINNET)
        }
        _ => {
            error!("Unsupported chain `{}`", args.network);
            return Err(Error::UnsupportedChain)
        }
    };

    // Parse the genesis block
    let bytes = base64::decode(genesis_block.trim()).unwrap();
    let genesis_block: BlockInfo = deserialize_async(&bytes).await?;

    // Compute the bootstrap timestamp
    let bootstrap = match blockchain_config.bootstrap {
        Some(b) => b,
        None => genesis_block.header.timestamp.inner(),
    };

    // Initialize or open sled database
    let db_path = expand_path(&blockchain_config.database)?;
    let sled_db = sled::open(&db_path)?;

    // Initialize validator configuration
    let pow_fixed_difficulty = if let Some(diff) = blockchain_config.pow_fixed_difficulty {
        info!(target: "darkfid", "Node is configured to run with fixed PoW difficulty: {}", diff);
        Some(diff.into())
    } else {
        None
    };

    let config = ValidatorConfig {
        finalization_threshold: blockchain_config.threshold,
        pow_target: blockchain_config.pow_target,
        pow_fixed_difficulty,
        genesis_block,
        verify_fees: !blockchain_config.skip_fees,
    };

    // Initialize validator
    let validator = Validator::new(&sled_db, config).await?;

    // Here we initialize various subscribers that can export live blockchain/consensus data.
    let mut subscribers = HashMap::new();
    subscribers.insert("blocks", JsonSubscriber::new("blockchain.subscribe_blocks"));
    subscribers.insert("txs", JsonSubscriber::new("blockchain.subscribe_txs"));
    subscribers.insert("proposals", JsonSubscriber::new("blockchain.subscribe_proposals"));

    // Initialize P2P network
    let p2p =
        spawn_p2p(&blockchain_config.net.into(), &validator, &subscribers, ex.clone()).await?;

    // Initialize JSON-RPC client to perform requests to minerd
    let rpc_client = if blockchain_config.miner {
        let Ok(rpc_client) =
            MinerRpcClient::new(blockchain_config.minerd_endpoint, ex.clone()).await
        else {
            error!(target: "darkfid", "Failed to initialize miner daemon rpc client, check if minerd is running");
            return Err(Error::RpcClientStopped)
        };
        Some(Mutex::new(rpc_client))
    } else {
        None
    };

    // Grab blockchain network configured transactions batch size for garbage collection
    let txs_batch_size = match blockchain_config.txs_batch_size {
        Some(b) => {
            if b > 0 {
                b
            } else {
                50
            }
        }
        None => 50,
    };

    info!(target: "darkfid", "Starting dnet subs task");
    let dnet_sub = JsonSubscriber::new("dnet.subscribe_events");
    let dnet_sub_ = dnet_sub.clone();
    let p2p_ = p2p.clone();
    let dnet_task = StoppableTask::new();
    dnet_task.clone().start(
        async move {
            let dnet_sub = p2p_.dnet_subscribe().await;
            loop {
                let event = dnet_sub.receive().await;
                debug!(target: "darkfid", "Got dnet event: {:?}", event);
                dnet_sub_.notify(vec![event.into()].into()).await;
            }
        },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => error!(target: "darkfid", "Failed starting dnet subs task: {}", e),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );

    // Initialize node
    let darkfid = Darkfid::new(
        p2p.clone(),
        validator,
        blockchain_config.miner,
        txs_batch_size,
        subscribers,
        rpc_client,
        dnet_sub,
    )
    .await;
    let darkfid = Arc::new(darkfid);
    info!(target: "darkfid", "Node initialized successfully!");

    // Pinging minerd daemon to verify it listens
    if blockchain_config.miner {
        if let Err(e) = darkfid.ping_miner_daemon().await {
            error!(target: "darkfid", "Failed to ping miner daemon: {}", e);
            return Err(Error::RpcClientStopped)
        }
    }

    // JSON-RPC server
    info!(target: "darkfid", "Starting JSON-RPC server on {}", blockchain_config.rpc_listen);
    // Here we create a task variable so we can manually close the
    // task later. P2P tasks don't need this since it has its own
    // stop() function to shut down, also terminating the task we
    // created for it.
    let rpc_task = StoppableTask::new();
    let darkfid_ = darkfid.clone();
    rpc_task.clone().start(
        listen_and_serve(blockchain_config.rpc_listen, darkfid.clone(), None, ex.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => darkfid_.stop_connections().await,
                Err(e) => error!(target: "darkfid", "Failed starting sync JSON-RPC server: {}", e),
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );

    // Merge mining JSON-RPC server
    info!(target: "darkfid", "Starting Merge mining server on {}", blockchain_config.mm_rpc_listen);
    let darkfid_ = darkfid.clone();
    let darkfid__ = darkfid.clone();
    let mm_rpc_task = StoppableTask::new();
    let mut mm_rpc = tide::new();
    mm_rpc.at("/json_rpc").post(move |req| {
        let darkfid__ = darkfid__.clone();
        async move { darkfid__.handle_mm_rpc_request(req).await }
    });

    mm_rpc_task.clone().start(
        async move {
            mm_rpc
                .listen(&format!(
                    "{}:{}",
                    blockchain_config.mm_rpc_listen.host_str().unwrap(),
                    blockchain_config.mm_rpc_listen.port().unwrap()
                ))
                .await?;
            Ok(())
        },
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => darkfid_.stop_connections().await,
                Err(e) => {
                    error!(target: "darkfid", "Failed starting merge-mining server: {}", e)
                }
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );

    info!(target: "darkfid", "Starting P2P network");
    p2p.clone().start().await?;

    // Consensus protocol
    info!(target: "darkfid", "Starting consensus protocol task");
    let consensus_task = StoppableTask::new();
    consensus_task.clone().start(
        consensus_init_task(
            darkfid.clone(),
            ConsensusInitTaskConfig {
                skip_sync: blockchain_config.skip_sync,
                checkpoint_height: blockchain_config.checkpoint_height,
                checkpoint: blockchain_config.checkpoint,
                miner: blockchain_config.miner,
                recipient: blockchain_config.recipient,
                spend_hook: blockchain_config.spend_hook,
                user_data: blockchain_config.user_data,
                bootstrap,
            },
            ex.clone(),
        ),
        |res| async move {
            match res {
                Ok(()) | Err(Error::ConsensusTaskStopped) | Err(Error::MinerTaskStopped) => { /* Do nothing */ }
                Err(e) => error!(target: "darkfid", "Failed starting consensus initialization task: {}", e),
            }
        },
        Error::ConsensusTaskStopped,
        ex.clone(),
    );

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!(target: "darkfid", "Caught termination signal, cleaning up and exiting...");

    info!(target: "darkfid", "Stopping dnet subs task...");
    dnet_task.stop().await;

    info!(target: "darkfid", "Stopping JSON-RPC server...");
    rpc_task.stop().await;

    info!(target: "darkfid", "Stopping Merge mining server...");
    mm_rpc_task.stop().await;

    info!(target: "darkfid", "Stopping P2P network...");
    p2p.stop().await;

    info!(target: "darkfid", "Stopping consensus task...");
    consensus_task.stop().await;

    info!(target: "darkfid", "Flushing sled database...");
    let flushed_bytes = sled_db.flush_async().await?;
    info!(target: "darkfid", "Flushed {} bytes", flushed_bytes);

    if let Some(ref rpc_client) = darkfid.rpc_client {
        info!(target: "darkfid", "Stopping JSON-RPC client...");
        rpc_client.lock().await.client.stop().await;
    };

    Ok(())
}
