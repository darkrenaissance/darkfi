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
use smol::{channel::Receiver, lock::Mutex, stream::StreamExt, Executor};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use url::Url;

use darkfi::{
    async_daemonize, cli_desc,
    rpc::server::{listen_and_serve, RequestHandler},
    system::{StoppableTask, StoppableTaskPtr},
    Error, Result,
};

const CONFIG_FILE: &str = "minerd.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../minerd.toml");

/// Daemon error codes
mod error;

/// JSON-RPC server methods
mod rpc;

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "minerd", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(short, long, default_value = "tcp://127.0.0.1:28467")]
    /// JSON-RPC listen URL
    rpc_listen: Url,

    #[structopt(short, long, default_value = "4")]
    /// PoW miner number of threads to use
    threads: usize,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

/// Daemon structure
pub struct Minerd {
    /// PoW miner number of threads to use
    threads: usize,
    // Receiver to stop miner threads
    stop_signal: Receiver<()>,
    /// JSON-RPC connection tracker
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
}

impl Minerd {
    pub fn new(threads: usize, stop_signal: Receiver<()>) -> Self {
        Self { threads, stop_signal, rpc_connections: Mutex::new(HashSet::new()) }
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<Executor<'static>>) -> Result<()> {
    info!(target: "minerd", "Starting DarkFi Mining Daemon...");
    let (sender, recvr) = smol::channel::bounded(1);
    let minerd = Arc::new(Minerd::new(args.threads, recvr));

    info!(target: "minerd", "Starting JSON-RPC server on {}", args.rpc_listen);
    let minerd_ = Arc::clone(&minerd);
    let rpc_task = StoppableTask::new();
    rpc_task.clone().start(
        listen_and_serve(args.rpc_listen, minerd.clone(), None, ex.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => minerd_.stop_connections().await,
                Err(e) => error!(target: "minerd", "Failed stopping JSON-RPC server: {}", e),
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!(target: "minerd", "Caught termination signal, cleaning up and exiting");

    info!(target: "minerd", "Stopping miner threads...");
    sender.send(()).await?;

    info!(target: "minerd", "Stopping JSON-RPC server...");
    rpc_task.stop().await;

    info!(target: "minerd", "Shut down successfully");
    Ok(())
}
