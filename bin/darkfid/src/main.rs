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
    str::FromStr,
    sync::Arc,
};

use log::{error, info};
use smol::{lock::Mutex, stream::StreamExt};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use url::Url;

use darkfi::{
    async_daemonize,
    blockchain::BlockInfo,
    cli_desc,
    net::{settings::SettingsOpt, P2pPtr},
    rpc::{
        client::RpcClient,
        jsonrpc::JsonSubscriber,
        server::{listen_and_serve, RequestHandler},
    },
    system::{StoppableTask, StoppableTaskPtr},
    util::path::expand_path,
    validator::{Validator, ValidatorConfig, ValidatorPtr},
    Error, Result,
};
use darkfi_sdk::crypto::PublicKey;
use darkfi_serial::deserialize_async;

#[cfg(test)]
mod tests;

mod error;
use error::{server_error, RpcError};

/// JSON-RPC requests handler and methods
mod rpc;
mod rpc_blockchain;
mod rpc_tx;

/// Validator async tasks
mod task;
use task::{consensus_task, miner_task, sync_task};

/// P2P net protocols
mod proto;

/// Utility functions
mod utils;
use utils::{parse_blockchain_config, spawn_miners_p2p, spawn_sync_p2p};

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

    #[structopt(short, long, default_value = "tcp://127.0.0.1:8340")]
    /// JSON-RPC listen URL
    rpc_listen: Url,

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
    #[structopt(long, default_value = "~/.local/darkfi/darkfid_blockchain_localnet")]
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
    pub pow_target: usize,

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
    /// Skip syncing process and start node right away
    pub skip_sync: bool,

    /// Syncing network settings
    #[structopt(flatten)]
    pub sync_net: SettingsOpt,

    /// Miners network settings
    #[structopt(flatten)]
    pub miners_net: SettingsOpt,
}

/// Daemon structure
pub struct Darkfid {
    /// Syncing P2P network pointer
    sync_p2p: P2pPtr,
    /// Optional miners P2P network pointer
    miners_p2p: Option<P2pPtr>,
    /// Validator(node) pointer
    validator: ValidatorPtr,
    /// A map of various subscribers exporting live info from the blockchain
    subscribers: HashMap<&'static str, JsonSubscriber>,
    /// JSON-RPC connection tracker
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    /// JSON-RPC client to execute requests to the miner daemon
    rpc_client: Option<RpcClient>,
}

impl Darkfid {
    pub async fn new(
        sync_p2p: P2pPtr,
        miners_p2p: Option<P2pPtr>,
        validator: ValidatorPtr,
        subscribers: HashMap<&'static str, JsonSubscriber>,
        rpc_client: Option<RpcClient>,
    ) -> Self {
        Self {
            sync_p2p,
            miners_p2p,
            validator,
            subscribers,
            rpc_connections: Mutex::new(HashSet::new()),
            rpc_client,
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
    let bytes = bs58::decode(&genesis_block.trim()).into_vec()?;
    let genesis_block: BlockInfo = deserialize_async(&bytes).await?;

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
        verify_fees: false, // TODO: Make configurable
    };

    // Initialize validator
    let validator = Validator::new(&sled_db, config).await?;

    // Here we initialize various subscribers that can export live blockchain/consensus data.
    let mut subscribers = HashMap::new();
    subscribers.insert("blocks", JsonSubscriber::new("blockchain.subscribe_blocks"));
    subscribers.insert("txs", JsonSubscriber::new("blockchain.subscribe_txs"));
    subscribers.insert("proposals", JsonSubscriber::new("blockchain.subscribe_proposals"));

    // Initialize syncing P2P network
    let sync_p2p =
        spawn_sync_p2p(&blockchain_config.sync_net.into(), &validator, &subscribers, ex.clone())
            .await;

    // Initialize miners P2P network
    let (miners_p2p, rpc_client) = if blockchain_config.miner {
        let Ok(rpc_client) = RpcClient::new(blockchain_config.minerd_endpoint, ex.clone()).await
        else {
            error!(target: "darkfid", "Failed to initialize miner daemon rpc client, check if minerd is running");
            return Err(Error::RpcClientStopped)
        };
        (
            Some(
                spawn_miners_p2p(
                    &blockchain_config.miners_net.into(),
                    &validator,
                    &subscribers,
                    ex.clone(),
                )
                .await,
            ),
            Some(rpc_client),
        )
    } else {
        (None, None)
    };

    // Initialize node
    let darkfid = Darkfid::new(
        sync_p2p.clone(),
        miners_p2p.clone(),
        validator.clone(),
        subscribers,
        rpc_client,
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
    info!(target: "darkfid", "Starting JSON-RPC server");
    // Here we create a task variable so we can manually close the
    // task later. P2P tasks don't need this since it has its own
    // stop() function to shut down, also terminating the task we
    // created for it.
    let rpc_task = StoppableTask::new();
    let darkfid_ = darkfid.clone();
    rpc_task.clone().start(
        listen_and_serve(args.rpc_listen, darkfid.clone(), None, ex.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => darkfid_.stop_connections().await,
                Err(e) => error!(target: "darkfid", "Failed starting sync JSON-RPC server: {}", e),
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );

    info!(target: "darkfid", "Starting sync P2P network");
    sync_p2p.clone().start().await?;

    // Start miners P2P network
    if blockchain_config.miner {
        info!(target: "darkfid", "Starting miners P2P network");
        let miners_p2p = miners_p2p.clone().unwrap();
        miners_p2p.clone().start().await?;
    } else {
        info!(target: "darkfid", "Not starting miners P2P network");
    }

    // Sync blockchain
    if !blockchain_config.skip_sync {
        sync_task(&darkfid).await?;
    } else {
        *darkfid.validator.synced.write().await = true;
    }

    // Clean node pending transactions
    darkfid.validator.purge_pending_txs().await?;

    // Consensus protocol
    info!(target: "darkfid", "Starting consensus protocol task");
    let consensus_task = if blockchain_config.miner {
        // Grab rewards recipient public key(address)
        if blockchain_config.recipient.is_none() {
            return Err(Error::ParseFailed("Recipient address missing"))
        }
        let recipient = match PublicKey::from_str(&blockchain_config.recipient.unwrap()) {
            Ok(address) => address,
            Err(_) => return Err(Error::InvalidAddress),
        };

        let task = StoppableTask::new();
        task.clone().start(
            // Weird hack to prevent lifetimes hell
            async move { miner_task(&darkfid, &recipient, blockchain_config.skip_sync).await },
            |res| async {
                match res {
                    Ok(()) | Err(Error::MinerTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "darkfid", "Failed starting miner task: {}", e),
                }
            },
            Error::MinerTaskStopped,
            ex.clone(),
        );

        task
    } else {
        let task = StoppableTask::new();
        task.clone().start(
            // Weird hack to prevent lifetimes hell
            async move { consensus_task(&darkfid).await },
            |res| async {
                match res {
                    Ok(()) | Err(Error::ConsensusTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "darkfid", "Failed starting consensus task: {}", e),
                }
            },
            Error::ConsensusTaskStopped,
            ex.clone(),
        );

        task
    };

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!(target: "darkfid", "Caught termination signal, cleaning up and exiting...");

    info!(target: "darkfid", "Stopping JSON-RPC server...");
    rpc_task.stop().await;

    info!(target: "darkfid", "Stopping syncing P2P network...");
    sync_p2p.stop().await;

    if blockchain_config.miner {
        info!(target: "darkfid", "Stopping miners P2P network...");
        miners_p2p.unwrap().stop().await;
    }

    info!(target: "darkfid", "Stopping consensus task...");
    consensus_task.stop().await;

    info!(target: "darkfid", "Flushing sled database...");
    let flushed_bytes = sled_db.flush_async().await?;
    info!(target: "darkfid", "Flushed {} bytes", flushed_bytes);

    Ok(())
}
