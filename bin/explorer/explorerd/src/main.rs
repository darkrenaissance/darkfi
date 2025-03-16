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

use std::{collections::HashSet, path::Path, sync::Arc};

use log::{error, info};
use smol::{lock::Mutex, stream::StreamExt};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use url::Url;

use darkfi::{
    async_daemonize, cli_desc,
    rpc::server::{listen_and_serve, RequestHandler},
    system::{StoppableTask, StoppableTaskPtr},
    util::path::get_config_path,
    Error, Result,
};

use crate::{
    config::ExplorerNetworkConfig,
    rpc::{blocks::subscribe_blocks, DarkfidRpcClient},
    service::ExplorerService,
};

/// Configuration management across multiple networks (localnet, testnet, mainnet)
mod config;

/// Manages JSON-RPC interactions for the explorer
mod rpc;

/// Core logic for block synchronization, chain data access, metadata storage/retrieval,
/// and statistics computation
mod service;

/// Manages persistent storage for blockchain, contracts, metrics, and metadata
mod store;

/// Crate errors
mod error;

/// Test utilities used for unit and integration testing
mod test_utils;

const CONFIG_FILE: &str = "explorerd_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../explorerd_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "explorerd", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(short, long, default_value = "testnet")]
    /// Explorer network (localnet, testnet, mainnet)
    network: String,

    #[structopt(long)]
    /// Reset the database and start syncing from first block
    reset: bool,

    #[structopt(short, long)]
    /// Set log file to output to
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[structopt(short, long)]
    /// Disable synchronization and connections to `darkfid`, operating solely
    /// on the local explorer database without attempting to connect or sync.
    /// If not specified, the application will attempt to connect and sync by default.
    no_sync: bool,
}

/// Defines a daemon structure responsible for handling incoming JSON-RPC requests and delegating them
/// to the backend layer for processing. It provides a JSON-RPC interface for managing operations related to
/// blocks, transactions, contracts, and metrics.
///
/// Upon startup, the daemon initializes a background task to handle incoming JSON-RPC requests.
/// This includes processing operations related to blocks, transactions, contracts, and metrics by
/// delegating them to the backend and returning appropriate RPC responses. Additionally, the daemon
/// synchronizes blocks from the `darkfid` daemon into the explorer database and subscribes
/// to new blocks, ensuring that the local database remains updated in real-time.
pub struct Explorerd {
    /// Explorer service instance
    pub service: ExplorerService,
    /// JSON-RPC connection tracker
    pub rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    /// JSON-RPC client to execute requests to darkfid daemon
    pub darkfid_client: Arc<DarkfidRpcClient>,
    /// Darkfi blockchain node endpoint to sync with when not in no-sync mode
    darkfid_endpoint: Url,
    /// A asynchronous executor used to create an RPC client when not in no-sync mode
    executor: Arc<smol::Executor<'static>>,
}

impl Explorerd {
    /// Creates a new `BlockchainExplorer` instance.
    async fn new(
        db_path: String,
        darkfid_endpoint: Url,
        ex: Arc<smol::Executor<'static>>,
    ) -> Result<Self> {
        // Initialize darkfid rpc client
        let darkfid_client = Arc::new(DarkfidRpcClient::new());

        // Create explorer service
        let service = ExplorerService::new(db_path)?;

        // Initialize the explorer service
        service.init().await?;

        Ok(Self {
            service,
            rpc_connections: Mutex::new(HashSet::new()),
            darkfid_client,
            darkfid_endpoint,
            executor: ex,
        })
    }

    /// Establishes a connection to the configured darkfid endpoint, returning a successful
    /// result if the connection is successful, or an error otherwise.
    async fn connect(&self) -> Result<()> {
        self.darkfid_client.connect(self.darkfid_endpoint.clone(), self.executor.clone()).await
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    info!(target: "explorerd", "Initializing DarkFi blockchain explorer node...");

    // Resolve the configuration path
    let config_path = get_config_path(args.config.clone(), CONFIG_FILE)?;

    // Get explorer network configuration
    let config: ExplorerNetworkConfig = (&config_path, &args.network).try_into()?;

    // Initialize the explorer daemon instance
    let explorer =
        Explorerd::new(config.database.clone(), config.endpoint.clone(), ex.clone()).await?;
    let explorer = Arc::new(explorer);
    info!(target: "explorerd", "Node initialized successfully!");

    // JSON-RPC server
    // Here we create a task variable so we can manually close the task later.
    let rpc_task = StoppableTask::new();
    let explorer_ = explorer.clone();
    rpc_task.clone().start(
        listen_and_serve(config.rpc.clone().into(), explorer.clone(), None, ex.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => explorer_.stop_connections().await,
                Err(e) => {
                    error!(target: "explorerd", "Failed starting sync JSON-RPC server: {}", e)
                }
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );
    info!(target: "explorerd", "Started JSON-RPC server: {}", config.rpc.rpc_listen.to_string().trim_end_matches("/"));

    // Declare task variables optional in case we are in no-sync mode
    let mut subscriber_task = None;
    let mut listener_task = None;

    // Do not sync when in no-sync mode
    if !args.no_sync {
        explorer.connect().await?;

        // Sync blocks
        info!(target: "explorerd", "Syncing blocks from darkfid...");
        if let Err(e) = explorer.sync_blocks(args.reset).await {
            let error_message = format!("Error syncing blocks: {:?}", e);
            error!(target: "explorerd", "{error_message}");
            return Err(Error::DatabaseError(error_message));
        }

        // Subscribe blocks
        info!(target: "explorerd", "Subscribing to new blocks...");
        match subscribe_blocks(explorer.clone(), config.endpoint.clone(), ex.clone()).await {
            Ok((sub_task, lst_task)) => {
                subscriber_task = Some(sub_task);
                listener_task = Some(lst_task);
            }
            Err(e) => {
                let error_message = format!("Error setting up blocks subscriber: {:?}", e);
                error!(target: "explorerd", "{error_message}");
                return Err(Error::DatabaseError(error_message));
            }
        };
    }

    log_started_banner(explorer.clone(), &config, &args, &config_path, args.no_sync);
    info!(target: "explorerd::", "All is good. Waiting for block notifications...");

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!(target: "explorerd", "Caught termination signal, cleaning up and exiting...");

    info!(target: "explorerd", "Stopping JSON-RPC server...");
    rpc_task.stop().await;

    // Stop darkfid listener task if it exists
    if let Some(task) = listener_task {
        info!(target: "explorerd", "Stopping darkfid listener...");
        task.stop().await;
    }

    // Stop darkfid subscribe task if it exists
    if let Some(task) = subscriber_task {
        info!(target: "explorerd", "Stopping darkfid subscriber...");
        task.stop().await;
    }

    info!(target: "explorerd", "Stopping JSON-RPC client...");
    let _ = explorer.darkfid_client.stop().await;

    Ok(())
}

/// Logs a banner displaying the startup details of the DarkFi Explorer Node.
fn log_started_banner(
    explorer: Arc<Explorerd>,
    config: &ExplorerNetworkConfig,
    args: &Args,
    config_path: &Path,
    no_sync: bool,
) {
    // Generate the `connected_node` string based on sync mode
    let connected_node = if no_sync {
        "Not connected".to_string()
    } else {
        config.endpoint.to_string().trim_end_matches('/').to_string()
    };

    // Log the banner
    info!(target: "explorerd", "========================================================================================");
    info!(target: "explorerd", "                   Started DarkFi Explorer Node{}                                        ",
        if no_sync { " (No-Sync Mode)" } else { "" });
    info!(target: "explorerd", "========================================================================================");
    info!(target: "explorerd", "  - Network: {}", args.network);
    info!(target: "explorerd", "  - JSON-RPC Endpoint: {}", config.rpc.rpc_listen.to_string().trim_end_matches('/'));
    info!(target: "explorerd", "  - Database: {}", config.database);
    info!(target: "explorerd", "  - Configuration: {}", config_path.to_str().unwrap_or("Error: configuration path not found!"));
    info!(target: "explorerd", "  - Reset Blocks: {}", if args.reset { "Yes" } else { "No" });
    info!(target: "explorerd", "~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~");
    info!(target: "explorerd", "  - Synced Blocks: {}", explorer.service.db.blockchain.len());
    info!(target: "explorerd", "  - Synced Transactions: {}", explorer.service.db.blockchain.len());
    info!(target: "explorerd", "  - Connected Darkfi Node: {}", connected_node);
    info!(target: "explorerd", "========================================================================================");
}
