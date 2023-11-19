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

use darkfi::{
    async_daemonize, cli_desc,
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResult},
        server::{listen_and_serve, RequestHandler},
    },
    system::{StoppableTask, StoppableTaskPtr},
    Error, Result,
};
use darkfi_serial::async_trait;
use log::{error, info};
use serde::Deserialize;
use smol::{
    lock::{Mutex, MutexGuard, RwLock},
    net::TcpStream,
    stream::StreamExt,
    Executor,
};
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use url::Url;
use uuid::Uuid;

mod error;
mod stratum;

const CONFIG_FILE: &str = "darkfi_mmproxy.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../darkfi_mmproxy.toml");

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

    #[structopt(long, default_value = "tcp://127.0.0.1:3333")]
    /// mmproxy JSON-RPC server listen URL
    rpc_listen: Url,

    #[structopt(long)]
    /// List of worker logins
    workers: Vec<String>,

    #[structopt(long)]
    /// Set log file output
    log: Option<String>,

    #[structopt(flatten)]
    monerod: MonerodArgs,
}

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[structopt()]
struct MonerodArgs {
    #[structopt(long, default_value = "mainnet")]
    /// Mining reward wallet address
    network: String,

    #[structopt(long, default_value = "http://127.0.0.1:28081/json_rpc")]
    /// monerod JSON-RPC server listen URL
    rpc: Url,
}

struct MiningProxy {
    /// monerod network type
    monerod_network: monero::Network,
    /// monerod RPC address
    monerod_rpc: Url,
    /// Workers UUIDs
    workers: Arc<RwLock<HashMap<Uuid, stratum::Worker>>>,
    /// JSON-RPC connection tracker
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    /// Main async executor reference
    executor: Arc<Executor<'static>>,
}

impl MiningProxy {
    async fn new(monerod: MonerodArgs, executor: Arc<Executor<'static>>) -> Result<Self> {
        let monerod_network = match monerod.network.as_str() {
            "mainnet" => monero::Network::Mainnet,
            "testnet" => monero::Network::Testnet,
            _ => {
                error!("Invalid Monero network \"{}\"", monerod.network);
                return Err(Error::Custom("Invalid Monero network".to_string()))
            }
        };

        // Test that monerod RPC is reachable
        if let Err(e) = TcpStream::connect(monerod.rpc.socket_addrs(|| None)?[0]).await {
            error!("Failed connecting to monerod RPC: {}", e);
            return Err(e.into())
        }

        let workers = Arc::new(RwLock::new(HashMap::new()));
        let rpc_connections = Mutex::new(HashSet::new());

        Ok(Self { monerod_network, monerod_rpc: monerod.rpc, workers, rpc_connections, executor })
    }
}

#[async_trait]
#[rustfmt::skip]
impl RequestHandler for MiningProxy {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        match req.method.as_str() {
            "ping" => self.pong(req.id, req.params).await,

            // Stratum methods
            "login" => self.stratum_login(req.id, req.params).await,
            "submit" => self.stratum_submit(req.id, req.params).await,
            "keepalived" => self.stratum_keepalived(req.id, req.params).await,

            _ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'_, HashSet<StoppableTaskPtr>> {
        self.rpc_connections.lock().await
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<Executor<'static>>) -> Result<()> {
    info!("Starting DarkFi x Monero merge mining proxy...");

    let mmproxy = Arc::new(MiningProxy::new(args.monerod, ex.clone()).await?);
    let mmproxy_ = Arc::clone(&mmproxy);

    info!("Starting JSON-RPC server");
    let rpc_task = StoppableTask::new();
    rpc_task.clone().start(
        listen_and_serve(args.rpc_listen, mmproxy.clone(), None, ex.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => mmproxy_.stop_connections().await,
                Err(e) => error!("Failed stopping JSON-RPC server: {}", e),
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );

    info!("Merge mining proxy ready, waiting for connections...");

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!("Caught termination signal, cleaning up and exiting...");

    Ok(())
}
