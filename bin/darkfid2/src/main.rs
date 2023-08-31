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
    util::time::TimeKeeper,
    validator::{Validator, ValidatorConfig, ValidatorPtr},
    Error, Result,
};
use darkfi_contract_test_harness::vks;

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
use task::sync::sync_task;

/// P2P net protocols
mod proto;

/// Utility functions
mod utils;
use utils::{genesis_txs_total, spawn_consensus_p2p, spawn_sync_p2p};

const CONFIG_FILE: &str = "darkfid_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../darkfid_config.toml");

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

    #[structopt(long)]
    /// Participate in the consensus protocol
    consensus: bool,

    #[structopt(long)]
    /// Skip syncing process and start node right away
    skip_sync: bool,

    /// Syncing network settings
    #[structopt(flatten)]
    sync_net: SettingsOpt,

    /// Consensus network settings
    #[structopt(flatten)]
    consensus_net: SettingsOpt,

    #[structopt(long)]
    /// Enable testing mode for local testing
    testing_mode: bool,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
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

    if args.testing_mode {
        info!(target: "darkfid", "Node is configured to run in testing mode!");
    }

    // NOTE: everything is dummy for now
    // FIXME: The VKS should only ever have to be generated on initial run.
    //        Do not use the precompiles for actual production code.
    // Initialize or open sled database
    let sled_db = sled::Config::new().temporary(true).open()?;
    let (_, vks) = vks::read_or_gen_vks_and_pks()?;
    vks::inject(&sled_db, &vks)?;

    // Initialize validator configuration
    let genesis_block = BlockInfo::default();
    let genesis_txs_total = genesis_txs_total(&genesis_block.txs)?;
    let time_keeper = TimeKeeper::new(genesis_block.header.timestamp, 10, 90, 0);
    let config = ValidatorConfig::new(
        time_keeper,
        genesis_block,
        genesis_txs_total,
        vec![],
        args.testing_mode,
    );

    // Initialize validator
    let validator = Validator::new(&sled_db, config).await?;

    // Here we initialize various subscribers that can export live blockchain/consensus data.
    let mut subscribers = HashMap::new();
    subscribers.insert("blocks", JsonSubscriber::new("blockchain.subscribe_blocks"));
    subscribers.insert("txs", JsonSubscriber::new("blockchain.subscribe_txs"));
    if args.consensus {
        subscribers.insert("proposals", JsonSubscriber::new("blockchain.subscribe_proposals"));
    }

    // Initialize syncing P2P network
    let sync_p2p =
        spawn_sync_p2p(&args.sync_net.into(), &validator, &subscribers, ex.clone()).await;

    // Initialize consensus P2P network
    let consensus_p2p = if args.consensus {
        Some(
            spawn_consensus_p2p(&args.consensus_net.into(), &validator, &subscribers, ex.clone())
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
    if args.consensus {
        info!("Starting consensus P2P network");
        let consensus_p2p = consensus_p2p.clone().unwrap();
        consensus_p2p.clone().start().await?;
    } else {
        info!("Not starting consensus P2P network");
    }

    // Sync blockchain
    if !args.skip_sync {
        sync_task(&darkfid).await?;
    } else {
        darkfid.validator.write().await.synced = true;
    }

    // Clean node pending transactions
    darkfid.validator.write().await.purge_pending_txs().await?;

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!(target: "darkfid", "Caught termination signal, cleaning up and exiting...");

    info!(target: "darkfid", "Stopping JSON-RPC server...");
    rpc_task.stop().await;

    info!(target: "darkfid", "Stopping syncing P2P network...");
    sync_p2p.stop().await;

    if args.consensus {
        info!(target: "darkfid", "Stopping consensus P2P network...");
        consensus_p2p.unwrap().stop().await;
    }

    info!(target: "darkfid", "Flushing sled database...");
    let flushed_bytes = sled_db.flush_async().await?;
    info!(target: "darkfid", "Flushed {} bytes", flushed_bytes);

    Ok(())
}
