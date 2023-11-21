/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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
        jsonrpc::JsonSubscriber,
        server::{listen_and_serve, RequestHandler},
    },
    system::{StoppableTask, StoppableTaskPtr},
    util::{path::expand_path, time::TimeKeeper},
    validator::{utils::genesis_txs_total, Validator, ValidatorConfig, ValidatorPtr},
    Error, Result,
};
use darkfi_sdk::crypto::PublicKey;
use darkfi_serial::deserialize;

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
use task::{miner_task, sync_task};

/// P2P net protocols
mod proto;

/// Utility functions
mod utils;
use utils::{spawn_consensus_p2p, spawn_sync_p2p};

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

    #[structopt(long, default_value = "tcp://127.0.0.1:8340")]
    /// JSON-RPC listen URL
    rpc_listen: Url,

    #[structopt(long, default_value = "testnet")]
    /// Blockchain network to use
    network: String,

    #[structopt(flatten)]
    /// Localnet blockchain network configuration
    localnet: BlockchainNetwork,

    #[structopt(flatten)]
    /// Testnet blockchain network configuration
    testnet: BlockchainNetwork,

    #[structopt(flatten)]
    /// Mainnet blockchain network configuration
    mainnet: BlockchainNetwork,

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

    #[structopt(long, default_value = "4")]
    /// PoW miner number of threads to use
    pub pow_threads: usize,

    #[structopt(long, default_value = "10")]
    /// PoW block production target, in seconds
    pub pow_target: usize,

    #[structopt(long)]
    /// Optional fixed PoW difficulty, used for testing
    pub pow_fixed_difficulty: Option<usize>,

    #[structopt(long, default_value = "10")]
    /// Epoch duration, denominated by number of blocks/slots
    pub epoch_length: u64,

    #[structopt(long, default_value = "10")]
    /// PoS slot duration, in seconds
    pub slot_time: u64,

    #[structopt(long)]
    /// Whitelisted faucet public key (repeatable flag)
    pub faucet_pub: Vec<String>,

    #[structopt(long)]
    /// Participate in the consensus protocol
    pub consensus: bool,

    #[structopt(long)]
    /// Wallet address to receive consensus rewards
    pub recipient: Option<String>,

    #[structopt(long)]
    /// Skip syncing process and start node right away
    pub skip_sync: bool,

    #[structopt(long)]
    /// Enable PoS testing mode for local testing
    pub pos_testing_mode: bool,

    /// Syncing network settings
    #[structopt(flatten)]
    pub sync_net: SettingsOpt,

    /// Consensus network settings
    #[structopt(flatten)]
    pub consensus_net: SettingsOpt,
}

/// Daemon structure
pub struct Darkfid {
    /// Syncing P2P network pointer
    sync_p2p: P2pPtr,
    /// Optional consensus P2P network pointer
    consensus_p2p: Option<P2pPtr>,
    /// Validator(node) pointer
    validator: ValidatorPtr,
    /// A map of various subscribers exporting live info from the blockchain
    subscribers: HashMap<&'static str, JsonSubscriber>,
    /// JSON-RPC connection tracker
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
}

impl Darkfid {
    pub async fn new(
        sync_p2p: P2pPtr,
        consensus_p2p: Option<P2pPtr>,
        validator: ValidatorPtr,
        subscribers: HashMap<&'static str, JsonSubscriber>,
    ) -> Self {
        Self {
            sync_p2p,
            consensus_p2p,
            validator,
            subscribers,
            rpc_connections: Mutex::new(HashSet::new()),
        }
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    info!(target: "darkfid", "Initializing DarkFi node...");

    // Grab blockchain network configuration
    let (blockchain_config, genesis_block) = match args.network.as_str() {
        "localnet" => (args.localnet, GENESIS_BLOCK_LOCALNET),
        "testnet" => (args.testnet, GENESIS_BLOCK_TESTNET),
        "mainnet" => (args.mainnet, GENESIS_BLOCK_MAINNET),
        _ => {
            error!("Unsupported chain `{}`", args.network);
            return Err(Error::UnsupportedChain)
        }
    };

    if blockchain_config.pos_testing_mode {
        info!(target: "darkfid", "Node is configured to run in PoS testing mode!");
    }

    // Parse the genesis block
    let bytes = bs58::decode(&genesis_block.trim()).into_vec()?;
    let genesis_block: BlockInfo = deserialize(&bytes)?;

    // Initialize or open sled database
    let db_path = expand_path(&blockchain_config.database)?;
    let sled_db = sled::open(&db_path)?;

    // Initialize validator configuration
    let genesis_txs_total = genesis_txs_total(&genesis_block.txs)?;
    let time_keeper = TimeKeeper::new(
        genesis_block.header.timestamp,
        blockchain_config.epoch_length,
        blockchain_config.slot_time,
        0,
    );
    let pow_fixed_difficulty = if let Some(diff) = blockchain_config.pow_fixed_difficulty {
        info!(target: "darkfid", "Node is configured to run with fixed PoW difficulty: {}", diff);
        Some(diff.into())
    } else {
        None
    };
    let config = ValidatorConfig::new(
        time_keeper,
        blockchain_config.threshold,
        blockchain_config.pow_threads,
        blockchain_config.pow_target,
        pow_fixed_difficulty,
        genesis_block,
        genesis_txs_total,
        vec![],
        blockchain_config.pos_testing_mode,
    );

    // Initialize validator
    let validator = Validator::new(&sled_db, config).await?;

    // Here we initialize various subscribers that can export live blockchain/consensus data.
    let mut subscribers = HashMap::new();
    subscribers.insert("blocks", JsonSubscriber::new("blockchain.subscribe_blocks"));
    subscribers.insert("txs", JsonSubscriber::new("blockchain.subscribe_txs"));
    if blockchain_config.consensus {
        subscribers.insert("proposals", JsonSubscriber::new("blockchain.subscribe_proposals"));
    }

    // Initialize syncing P2P network
    let sync_p2p =
        spawn_sync_p2p(&blockchain_config.sync_net.into(), &validator, &subscribers, ex.clone())
            .await;

    // Initialize consensus P2P network
    let consensus_p2p = if blockchain_config.consensus {
        Some(
            spawn_consensus_p2p(
                &blockchain_config.consensus_net.into(),
                &validator,
                &subscribers,
                ex.clone(),
            )
            .await,
        )
    } else {
        None
    };

    // Initialize node
    let darkfid =
        Darkfid::new(sync_p2p.clone(), consensus_p2p.clone(), validator.clone(), subscribers).await;
    let darkfid = Arc::new(darkfid);
    info!(target: "darkfid", "Node initialized successfully!");

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

    // Consensus protocol
    if blockchain_config.consensus {
        info!(target: "darkfid", "Starting consensus P2P network");
        let consensus_p2p = consensus_p2p.clone().unwrap();
        consensus_p2p.clone().start().await?;
    } else {
        info!(target: "darkfid", "Not starting consensus P2P network");
    }

    // Sync blockchain
    if !blockchain_config.skip_sync {
        sync_task(&darkfid).await?;
    } else {
        darkfid.validator.write().await.synced = true;
    }

    // Clean node pending transactions
    darkfid.validator.write().await.purge_pending_txs().await?;

    // Consensus protocol
    let (consensus_task, consensus_sender) = if blockchain_config.consensus {
        info!(target: "darkfid", "Starting consensus protocol task");
        // Grab rewards recipient public key(address)
        if blockchain_config.recipient.is_none() {
            return Err(Error::ParseFailed("Recipient address missing"))
        }
        let recipient = match PublicKey::from_str(&blockchain_config.recipient.unwrap()) {
            Ok(address) => address,
            Err(_) => return Err(Error::InvalidAddress),
        };

        let (sender, recvr) = smol::channel::bounded(1);
        let task = StoppableTask::new();
        task.clone().start(
            // Weird hack to prevent lifetimes hell
            async move { miner_task(&darkfid, &recipient, &recvr).await },
            |res| async {
                match res {
                    Ok(()) | Err(Error::MinerTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "darkfid", "Failed starting miner task: {}", e),
                }
            },
            Error::MinerTaskStopped,
            ex.clone(),
        );
        (Some(task), Some(sender))
    } else {
        info!(target: "darkfid", "Not participating in consensus");
        (None, None)
    };

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!(target: "darkfid", "Caught termination signal, cleaning up and exiting...");

    info!(target: "darkfid", "Stopping JSON-RPC server...");
    rpc_task.stop().await;

    info!(target: "darkfid", "Stopping syncing P2P network...");
    sync_p2p.stop().await;

    if blockchain_config.consensus {
        info!(target: "darkfid", "Stopping consensus P2P network...");
        consensus_p2p.unwrap().stop().await;

        info!(target: "darkfid", "Stopping consensus task...");
        // Send signal to spawned miner threads to stop
        consensus_sender.unwrap().send(()).await?;
        consensus_task.unwrap().stop().await;
    }

    info!(target: "darkfid", "Flushing sled database...");
    let flushed_bytes = sled_db.flush_async().await?;
    info!(target: "darkfid", "Flushed {} bytes", flushed_bytes);

    Ok(())
}
