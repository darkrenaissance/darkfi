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

use std::{collections::HashSet, sync::Arc};

use log::{error, info};
use sled_overlay::sled;
use smol::{lock::Mutex, stream::StreamExt};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use url::Url;

use darkfi::{
    async_daemonize,
    blockchain::Blockchain,
    cli_desc,
    rpc::{
        client::RpcClient,
        server::{listen_and_serve, RequestHandler},
    },
    system::{StoppableTask, StoppableTaskPtr},
    util::path::expand_path,
    Error, Result,
};

/// Crate errors
mod error;

/// JSON-RPC requests handler and methods
mod rpc;
mod rpc_blocks;
use rpc_blocks::subscribe_blocks;
mod rpc_statistics;
mod rpc_transactions;

/// Database functionality related to blocks
mod blocks;

/// Database functionality related to transactions
mod transactions;

/// Database functionality related to statistics
mod statistics;

/// Test utilities used for unit and integration testing
mod test_utils;

const CONFIG_FILE: &str = "blockchain_explorer_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../blockchain_explorer_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "blockchain-explorer", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(short, long, default_value = "tcp://127.0.0.1:14567")]
    /// JSON-RPC listen URL
    rpc_listen: Url,

    #[structopt(long, default_value = "~/.local/share/darkfi/blockchain-explorer/daemon.db")]
    /// Path to daemon database
    db_path: String,

    #[structopt(long)]
    /// Reset the database and start syncing from first block
    reset: bool,

    #[structopt(short, long, default_value = "tcp://127.0.0.1:8340")]
    /// darkfid JSON-RPC endpoint
    endpoint: Url,

    #[structopt(short, long)]
    /// Set log file to output into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

/// Structure represents the explorer database backed by a sled DB connection.
pub struct ExplorerDb {
    /// Main pointer to the sled db connection
    pub sled_db: sled::Db,
    /// Explorer darkfid blockchain copy
    pub blockchain: Blockchain,
}

impl ExplorerDb {
    /// Creates a new `BlockExplorerDb` instance
    pub fn new(db_path: String) -> Result<ExplorerDb> {
        let db_path = expand_path(db_path.as_str())?;
        let sled_db = sled::open(&db_path)?;
        let blockchain = Blockchain::new(&sled_db)?;
        info!(target: "blockchain-explorer", "Initialized explorer database {}, block count: {}", db_path.display(), blockchain.len());
        Ok(ExplorerDb { sled_db, blockchain })
    }
}

/// Daemon structure
pub struct Explorerd {
    /// Explorer database instance
    pub db: ExplorerDb,
    /// JSON-RPC connection tracker
    pub rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    /// JSON-RPC client to execute requests to darkfid daemon
    pub rpc_client: RpcClient,
}

impl Explorerd {
    /// Creates a new `BlockchainExplorer` instance.
    async fn new(db_path: String, endpoint: Url, ex: Arc<smol::Executor<'static>>) -> Result<Self> {
        // Initialize rpc client
        let rpc_client = RpcClient::new(endpoint.clone(), ex).await?;
        info!(target: "explorerd", "Created rpc client: {:?}", endpoint);

        // Initialize explorer database
        let explorer_db = ExplorerDb::new(db_path)?;

        Ok(Self { rpc_connections: Mutex::new(HashSet::new()), rpc_client, db: explorer_db })
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    info!(target: "blockchain-explorer", "Initializing DarkFi blockchain explorer node...");
    let explorer = Explorerd::new(args.db_path, args.endpoint.clone(), ex.clone()).await?;
    let explorer = Arc::new(explorer);
    info!(target: "blockchain-explorer", "Node initialized successfully!");

    // JSON-RPC server
    info!(target: "blockchain-explorer", "Starting JSON-RPC server");
    // Here we create a task variable so we can manually close the task later.
    let rpc_task = StoppableTask::new();
    let explorer_ = explorer.clone();
    rpc_task.clone().start(
        listen_and_serve(args.rpc_listen, explorer.clone(), None, ex.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => explorer_.stop_connections().await,
                Err(e) => error!(target: "blockchain-explorer", "Failed starting sync JSON-RPC server: {}", e),
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );

    // Sync blocks
    info!(target: "blockchain-explorer", "Syncing blocks from darkfid...");
    if let Err(e) = explorer.sync_blocks(args.reset).await {
        let error_message = format!("Error syncing blocks: {:?}", e);
        error!(target: "blockchain-explorer", "{error_message}");
        return Err(Error::DatabaseError(error_message));
    }

    // Subscribe blocks
    info!(target: "blockchain-explorer", "Subscribing to new blocks...");
    let (subscriber_task, listener_task) =
        match subscribe_blocks(explorer.clone(), args.endpoint, ex.clone()).await {
            Ok(pair) => pair,
            Err(e) => {
                let error_message = format!("Error setting up blocks subscriber: {:?}", e);
                error!(target: "blockchain-explorer", "{error_message}");
                return Err(Error::DatabaseError(error_message));
            }
        };

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!(target: "blockchain-explorer", "Caught termination signal, cleaning up and exiting...");

    info!(target: "blockchain-explorer", "Stopping JSON-RPC server...");
    rpc_task.stop().await;

    info!(target: "blockchain-explorer", "Stopping darkfid listener...");
    listener_task.stop().await;

    info!(target: "blockchain-explorer", "Stopping darkfid subscriber...");
    subscriber_task.stop().await;

    info!(target: "blockchain-explorer", "Stopping JSON-RPC client...");
    explorer.rpc_client.stop().await;

    Ok(())
}
