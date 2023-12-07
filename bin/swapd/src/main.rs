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

use std::{collections::HashSet, sync::Arc};

use darkfi::{
    async_daemonize, cli_desc,
    rpc::server::{listen_and_serve, RequestHandler},
    system::{StoppableTask, StoppableTaskPtr},
    util::path::expand_path,
    Error, Result,
};
use log::{error, info};
use serde::Deserialize;
use smol::{fs, lock::Mutex, stream::StreamExt, Executor};
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use url::Url;

const CONFIG_FILE: &str = "swapd.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../swapd.toml");

/// JSON-RPC server methods
mod rpc;

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "darkfi-mmproxy", about = cli_desc!())]
struct Args {
    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(long)]
    /// Set log file output
    log: Option<String>,

    #[structopt(flatten)]
    swapd: SwapdArgs,
}

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[structopt()]
struct SwapdArgs {
    #[structopt(long, default_value = "tcp://127.0.0.1:52821")]
    /// darkfi-swapd JSON-RPC listen URL
    swapd_rpc: Url,

    #[structopt(long, default_value = "~/.local/darkfi/swapd")]
    /// Path to swapd's filesystem database
    swapd_db: String,
}

/// Swapd daemon state
struct Swapd {
    /// Main reference to the swapd filesystem databaase
    _sled_db: sled::Db,
    /// JSON-RPC connection tracker
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
}

impl Swapd {
    /// Instantiate `Swapd` state
    async fn new(_swapd_args: &SwapdArgs, sled_db: sled::Db) -> Result<Self> {
        Ok(Self { _sled_db: sled_db, rpc_connections: Mutex::new(HashSet::new()) })
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<Executor<'static>>) -> Result<()> {
    info!("Starting DarkFi Atomic Swap Daemon...");

    // Create datastore path if not there already.
    let datastore = expand_path(&args.swapd.swapd_db)?;
    fs::create_dir_all(&datastore).await?;
    let sled_db = sled::open(datastore)?;

    info!("Initializing daemon state");
    let swapd = Arc::new(Swapd::new(&args.swapd, sled_db.clone()).await?);

    info!("Starting JSON-RPC server on {}", args.swapd.swapd_rpc);
    let swapd_ = Arc::clone(&swapd);
    let rpc_task = StoppableTask::new();
    rpc_task.clone().start(
        listen_and_serve(args.swapd.swapd_rpc, swapd.clone(), None, ex.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => swapd_.stop_connections().await,
                Err(e) => error!("Failed stopping JSON-RPC server: {}", e),
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );

    info!("Ready to operate");

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!("Caught termination signal, cleaning up and exiting");

    info!("Flushing sled database");
    sled_db.flush_async().await?;

    info!("Shut down successfully");
    Ok(())
}
