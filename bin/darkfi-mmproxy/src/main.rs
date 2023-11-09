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
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResult, JsonSubscriber},
        server::{listen_and_serve, RequestHandler},
        util::JsonValue,
    },
    system::{StoppableTask, StoppableTaskPtr},
    Error, Result,
};
use darkfi_serial::async_trait;
use log::{error, info};
use serde::Deserialize;
use smol::{
    channel,
    lock::{Mutex, MutexGuard, RwLock},
    stream::StreamExt,
    Executor,
};
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use url::Url;
use uuid::Uuid;

mod error;
mod monero;
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
    monerod: Monerod,
}

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[structopt()]
struct Monerod {
    #[structopt(long, default_value = "")]
    /// Mining reward wallet address
    wallet_address: String,

    #[structopt(long, default_value = "http://127.0.0.1:28081/json_rpc")]
    /// monerod JSON-RPC server listen URL
    monerod_rpc: Url,
}

struct Worker {
    /// JSON-RPC notification subscriber, used to send job notifications
    job_sub: JsonSubscriber,
    /// Current job ID for the worker
    job_id: Uuid,
    /// Keepalive sender channel, pinged from stratum keepalived
    ka_send: channel::Sender<()>,
    /// Background keepalive task reference
    ka_task: StoppableTaskPtr,
}

impl Worker {
    fn new(
        job_sub: JsonSubscriber,
        ka_send: channel::Sender<()>,
        ka_task: StoppableTaskPtr,
    ) -> Self {
        Self { job_sub, job_id: Uuid::new_v4(), ka_send, ka_task }
    }

    async fn send_job(&mut self, blob: String, target: String) -> Result<()> {
        // Update job id
        self.job_id = Uuid::new_v4();

        let params: JsonValue = HashMap::from([
            ("blob".to_string(), blob.into()),
            ("job_id".to_string(), self.job_id.to_string().into()),
            ("target".to_string(), target.into()),
        ])
        .into();

        info!("Sending mining job notification to worker");
        self.job_sub.notify(params).await;

        Ok(())
    }
}

struct MiningProxy {
    /// monerod settings
    monerod: Monerod,
    /// Worker logins
    logins: HashMap<String, String>,
    /// Workers UUIDs
    workers: Arc<RwLock<HashMap<Uuid, Worker>>>,
    /// JSON-RPC connection tracker
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    /// Main async executor reference
    executor: Arc<Executor<'static>>,
}

impl MiningProxy {
    async fn new(
        monerod: Monerod,
        logins: HashMap<String, String>,
        executor: Arc<Executor<'static>>,
    ) -> Result<Self> {
        let self_ = Self {
            monerod,
            logins,
            workers: Arc::new(RwLock::new(HashMap::new())),
            rpc_connections: Mutex::new(HashSet::new()),
            executor,
        };

        // Test that monerod is reachable
        let req = JsonRequest::new("get_block_count", vec![].into());
        if let Err(e) = self_.oneshot_request(req).await {
            error!("Could not reach monerod: {}", e);
            return Err(Error::Custom("Could not reach monerod".to_string()))
        }

        Ok(self_)
    }
}

#[async_trait]
#[rustfmt::skip]
impl RequestHandler for MiningProxy {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        error!(target: "mmproxy::rpc", "--> {}", req.stringify().unwrap());

        match req.method.as_str() {
            "ping" => self.pong(req.id, req.params).await,

            // Stratum methods
            "login" => self.stratum_login(req.id, req.params).await,
            "submit" => self.stratum_submit(req.id, req.params).await,
            "keepalived" => self.stratum_keepalived(req.id, req.params).await,

            // Monero daemon methods
            "get_block_count" => self.monero_get_block_count(req.id, req.params).await,
            "getblockcount" => self.monero_get_block_count(req.id, req.params).await,
            "on_get_block_hash" => self.monero_on_get_block_hash(req.id, req.params).await,
            "on_getblockhash" => self.monero_on_get_block_hash(req.id, req.params).await,
            "get_block_template" => self.monero_get_block_template(req.id, req.params).await,
            "getblocktemplate" => self.monero_get_block_template(req.id, req.params).await,
            "submit_block" => self.monero_submit_block(req.id, req.params).await,
            "submitblock" => self.monero_submit_block(req.id, req.params).await,
            "generateblocks" => self.monero_generateblocks(req.id, req.params).await,

            /*
            "get_last_block_header" => self.monero_get_last_block_header(req.id, req.params).await,
            "get_block_header_by_hash" => self.monero_get_block_header_by_hash(req.id, req.params).await,
            "get_block_header_by_height" => self.monero_get_block_header_by_height(req.id, req.params).await,
            "get_block_headers_range" => self.monero_get_block_headers_range(req.id, req.params).await,
            "get_block" => self.monero_get_block(req.id, req.params).await,
            "get_connections" => self.monero_get_connections(req.id, req.params).await,
            "get_info" => self.monero_get_info(req.id, req.params).await,
            "hard_fork_info" => self.monero_hard_fork_info(req.id, req.params).await,
            "set_bans" => self.monero_set_bans(req.id, req.params).await,
            "get_bans" => self.monero_get_bans(req.id, req.params).await,
            "banned" => self.monero_banned(req.id, req.params).await,
            "flush_txpool" => self.monero_flush_txpool(req.id, req.params).await,
            "get_output_histogram" => self.monero_get_output_histogram(req.id, req.params).await,
            "get_version" => self.monero_get_version(req.id, req.params).await,
            "get_coinbase_tx_sum" => self.monero_get_coinbase_tx_sum(req.id, req.params).await,
            "get_fee_estimate" => self.monero_get_fee_estimate(req.id, req.params).await,
            "get_alternate_chains" => self.monero_get_alternate_chains(req.id, req.params).await,
            "relay_tx" => self.monero_relay_tx(req.id, req.params).await,
            "sync_info" => self.monero_sync_info(req.id, req.params).await,
            "get_txpool_backlog" => self.monero_get_txpool_backlog(req.id, req.params).await,
            "get_output_distribution" => self.monero_get_output_distribution(req.id, req.params).await,
            "get_miner_data" => self.monero_get_miner_data(req.id, req.params).await,
            "prune_blockchain" => self.monero_prune_blockchain(req.id, req.params).await,
            "calc_pow" => self.monero_calc_pow(req.id, req.params).await,
            "flush_cache" => self.monero_flush_cache(req.id, req.params).await,
            "add_aux_pow" => self.monero_add_aux_pow(req.id, req.params).await,
            */

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
    // Parse worker logins
    let mut logins = HashMap::new();
    for worker in args.workers {
        let mut split = worker.split(':');
        let user = split.next().unwrap().to_string();
        let pass = split.next().unwrap().to_string();
        info!("Whitelisting worker \"{}:{}\"", user, pass);
        logins.insert(user, pass);
    }

    if args.monerod.wallet_address.is_empty() {
        error!("Wallet address empty. Please set it in your config.");
        return Err(Error::Custom("Wallet address empty".to_string()))
    }

    info!("Instantiating MiningProxy with wallet: {}", args.monerod.wallet_address);
    let mmproxy = Arc::new(MiningProxy::new(args.monerod, logins, ex.clone()).await?);
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
