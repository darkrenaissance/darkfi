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
    collections::HashSet,
    fs,
    io::{stdin, stdout, Write},
    sync::Arc,
};

use log::{error, info};
use smol::{lock::Mutex, stream::StreamExt};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use url::Url;

use darkfi::{
    async_daemonize, cli_desc,
    rpc::{
        client::RpcClient,
        server::{listen_and_serve, RequestHandler},
    },
    system::{StoppableTask, StoppableTaskPtr},
    util::path::expand_path,
    Error, Result,
};
use drk::walletdb::{WalletDb, WalletPtr};

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

const CONFIG_FILE: &str = "blockchain_explorer_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../blockchain_explorer_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "blockcahin-explorer", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(short, long, default_value = "tcp://127.0.0.1:14567")]
    /// JSON-RPC listen URL
    rpc_listen: Url,

    #[structopt(long, default_value = "~/.local/darkfi/blockchain-explorer/daemon.db")]
    /// Path to daemon database
    db_path: String,

    #[structopt(long)]
    /// Password for the daemon database.
    /// If it's not present, daemon will prompt the user for it.
    db_pass: Option<String>,

    #[structopt(long)]
    /// Reset the databae and start syncing from first block
    reset: bool,

    #[structopt(short, long, default_value = "tcp://127.0.0.1:8340")]
    /// darkfid JSON-RPC endpoint
    endpoint: Url,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

/// Daemon structure
pub struct BlockchainExplorer {
    /// Daemon database operations handler
    pub database: WalletPtr,
    /// JSON-RPC connection tracker
    pub rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    /// JSON-RPC client to execute requests to darkfid daemon
    pub rpc_client: RpcClient,
}

impl BlockchainExplorer {
    async fn new(
        db_path: String,
        db_pass: Option<String>,
        endpoint: Url,
        ex: Arc<smol::Executor<'static>>,
    ) -> Result<Self> {
        // Grab password
        let db_pass = match db_pass {
            Some(pass) => pass,
            None => {
                let mut pass = String::new();
                while pass.trim().is_empty() {
                    info!(target: "blockchain-explorer", "Provide database passsword:");
                    stdout().flush()?;
                    stdin().read_line(&mut pass).unwrap_or(0);
                }
                pass.trim().to_string()
            }
        };

        // Script kiddies protection
        if db_pass == "changeme" {
            error!(target: "blockchain-explorer", "Please don't use default database password...");
            return Err(Error::ParseFailed("Default database password usage"))
        }

        // Initialize database
        let db_path = expand_path(&db_path)?;
        if !db_path.exists() {
            if let Some(parent) = db_path.parent() {
                fs::create_dir_all(parent)?;
            }
        }
        let database = match WalletDb::new(Some(db_path), Some(&db_pass)) {
            Ok(w) => w,
            Err(e) => {
                let err = format!("{e:?}");
                error!(target: "blockchain-explorer", "Error initializing database: {err}");
                return Err(Error::RusqliteError(err))
            }
        };

        // Initialize rpc client
        let rpc_client = RpcClient::new(endpoint, ex).await?;

        let explorer = Self { database, rpc_connections: Mutex::new(HashSet::new()), rpc_client };

        // Initialize all the database tables
        if let Err(e) = explorer.initialize_blocks().await {
            let err = format!("{e:?}");
            error!(target: "blockchain-explorer", "Error initializing blocks database table: {err}");
            return Err(Error::RusqliteError(err))
        }
        if let Err(e) = explorer.initialize_transactions().await {
            let err = format!("{e:?}");
            error!(target: "blockchain-explorer", "Error initializing transactions database table: {err}");
            return Err(Error::RusqliteError(err))
        }
        // TODO: Map deployed contracts to their corresponding files with sql table and retrieval methods

        Ok(explorer)
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    info!(target: "blockchain-explorer", "Initializing DarkFi blockchain explorer node...");
    let explorer =
        BlockchainExplorer::new(args.db_path, args.db_pass, args.endpoint.clone(), ex.clone())
            .await?;
    let explorer = Arc::new(explorer);
    info!(target: "blockchain-explorer", "Node initialized successfully!");

    // JSON-RPC server
    info!(target: "blockchain-explorer", "Starting JSON-RPC server");
    // Here we create a task variable so we can manually close the
    // task later.
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
        let err = format!("{e:?}");
        error!(target: "blockchain-explorer", "Error syncing blocks: {err}");
        return Err(Error::RusqliteError(err))
    }

    info!(target: "blockchain-explorer", "Subscribing to new blocks...");
    let (subscriber_task, listener_task) = match subscribe_blocks(
        explorer.clone(),
        args.endpoint,
        ex.clone(),
    )
    .await
    {
        Ok(pair) => pair,
        Err(e) => {
            let err = format!("{e:?}");
            error!(target: "blockchain-explorer", "Error while setting up blocks subscriber: {err}");
            return Err(Error::RusqliteError(err))
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
