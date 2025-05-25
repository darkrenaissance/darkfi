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

use log::{debug, error, info, warn};
use sled_overlay::sled;
use smol::{stream::StreamExt, Executor};
use std::sync::Arc;
use structopt_toml::{structopt::StructOpt, StructOptToml};

use darkfi::{
    async_daemonize, cli_desc,
    dht::{Dht, DhtHandler, DhtSettings, DhtSettingsOpt},
    geode::hash_to_string,
    net::{session::SESSION_DEFAULT, settings::SettingsOpt, P2p, Settings as NetSettings},
    rpc::{
        jsonrpc::JsonSubscriber,
        server::{listen_and_serve, RequestHandler},
        settings::{RpcSettings, RpcSettingsOpt},
    },
    system::{Publisher, StoppableTask},
    util::path::expand_path,
    Error, Result,
};

use fud::{
    get_node_id,
    proto::{FudFindNodesReply, ProtocolFud},
    rpc::JsonRpcInterface,
    tasks::{announce_seed_task, fetch_file_task, get_task},
    Fud,
};

const CONFIG_FILE: &str = "fud_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../fud_config.toml");
const NODE_ID_PATH: &str = "node_id";

#[derive(Clone, Debug, serde::Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "fud", about = cli_desc!())]
struct Args {
    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(long)]
    /// Set log file path to output daemon logs into
    log: Option<String>,

    #[structopt(long, default_value = "~/.local/share/darkfi/fud")]
    /// Base directory for filesystem storage
    base_dir: String,

    #[structopt(short, long)]
    /// Default path to store downloaded files (defaults to <base_dir>/downloads)
    downloads_path: Option<String>,

    #[structopt(long, default_value = "60")]
    /// Chunk transfer timeout in seconds
    chunk_timeout: u64,

    #[structopt(flatten)]
    /// Network settings
    net: SettingsOpt,

    #[structopt(flatten)]
    /// JSON-RPC settings
    rpc: RpcSettingsOpt,

    #[structopt(flatten)]
    /// DHT settings
    dht: DhtSettingsOpt,
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<Executor<'static>>) -> Result<()> {
    // The working directory for this daemon and geode.
    let basedir = expand_path(&args.base_dir)?;

    // The directory to store the downloaded files
    let downloads_path = match args.downloads_path {
        Some(downloads_path) => expand_path(&downloads_path)?,
        None => basedir.join("downloads"),
    };

    // Sled database init
    info!("Instantiating database");
    let sled_db = sled::open(basedir.join("db"))?;

    info!("Instantiating P2P network");
    let net_settings: NetSettings = args.net.into();
    let p2p = P2p::new(net_settings.clone(), ex.clone()).await?;

    info!("Starting dnet subs task");
    let dnet_sub = JsonSubscriber::new("dnet.subscribe_events");
    let dnet_sub_ = dnet_sub.clone();
    let p2p_ = p2p.clone();
    let dnet_task = StoppableTask::new();
    dnet_task.clone().start(
        async move {
            let dnet_sub = p2p_.dnet_subscribe().await;
            loop {
                let event = dnet_sub.receive().await;
                debug!("Got dnet event: {:?}", event);
                dnet_sub_.notify(vec![event.into()].into()).await;
            }
        },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => panic!("{}", e),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );

    let mut node_id_path = basedir.to_path_buf();
    node_id_path.push(NODE_ID_PATH);
    let node_id = get_node_id(&node_id_path).await?;

    info!(target: "fud", "Your node ID: {}", hash_to_string(&node_id));

    // Daemon instantiation
    let event_pub = Publisher::new();
    let dht_settings: DhtSettings = args.dht.into();
    let dht: Arc<Dht> = Arc::new(Dht::new(&node_id, &dht_settings, p2p.clone(), ex.clone()).await);
    let fud: Arc<Fud> = Arc::new(
        Fud::new(
            p2p.clone(),
            basedir,
            downloads_path,
            args.chunk_timeout,
            dht.clone(),
            sled_db.open_tree("path")?,
            event_pub.clone(),
        )
        .await?,
    );

    info!(target: "fud", "Starting download subs task");
    let event_sub = JsonSubscriber::new("event");
    let event_sub_ = event_sub.clone();
    let event_task = StoppableTask::new();
    event_task.clone().start(
        async move {
            let event_sub = event_pub.clone().subscribe().await;
            loop {
                let event = event_sub.receive().await;
                debug!(target: "fud", "Got event: {:?}", event);
                event_sub_.notify(event.into()).await;
            }
        },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => panic!("{}", e),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );

    info!(target: "fud", "Starting fetch file task");
    let file_task = StoppableTask::new();
    file_task.clone().start(
        fetch_file_task(fud.clone()),
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => error!(target: "fud", "Failed starting fetch file task: {}", e),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );

    info!(target: "fud", "Starting get task");
    let get_task_ = StoppableTask::new();
    get_task_.clone().start(
        get_task(fud.clone()),
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => error!(target: "fud", "Failed starting get task: {}", e),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );

    let rpc_settings: RpcSettings = args.rpc.into();
    info!(target: "fud", "Starting JSON-RPC server on {}", rpc_settings.listen);
    let rpc_interface = Arc::new(JsonRpcInterface::new(fud.clone(), dnet_sub, event_sub));
    let rpc_task = StoppableTask::new();
    let rpc_interface_ = rpc_interface.clone();
    rpc_task.clone().start(
        listen_and_serve(rpc_settings, rpc_interface, None, ex.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => rpc_interface_.stop_connections().await,
                Err(e) => error!(target: "fud", "Failed starting sync JSON-RPC server: {}", e),
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );

    info!("Starting P2P protocols");
    let registry = p2p.protocol_registry();
    let fud_ = fud.clone();
    registry
        .register(SESSION_DEFAULT, move |channel, p2p| {
            let fud_ = fud_.clone();
            async move { ProtocolFud::init(fud_, channel, p2p).await.unwrap() }
        })
        .await;
    p2p.clone().start().await?;

    let p2p_settings_lock = p2p.settings();
    let p2p_settings = p2p_settings_lock.read().await;
    if p2p_settings.external_addrs.is_empty() {
        warn!(target: "fud::realmain", "No external addresses, you won't be able to seed")
    }
    drop(p2p_settings);

    info!(target: "fud", "Starting DHT tasks");
    let dht_channel_task = StoppableTask::new();
    let fud_ = fud.clone();
    dht_channel_task.clone().start(
        async move { fud_.channel_task::<FudFindNodesReply>().await },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => error!(target: "fud", "Failed starting dht channel task: {}", e),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );
    let dht_disconnect_task = StoppableTask::new();
    let fud_ = fud.clone();
    dht_disconnect_task.clone().start(
        async move { fud_.disconnect_task().await },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => error!(target: "fud", "Failed starting dht disconnect task: {}", e),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );
    let announce_task = StoppableTask::new();
    let fud_ = fud.clone();
    announce_task.clone().start(
        async move { announce_seed_task(fud_.clone()).await },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => error!(target: "fud", "Failed starting announce task: {}", e),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!(target: "fud", "Caught termination signal, cleaning up and exiting...");

    info!(target: "fud", "Stopping fetch file task...");
    file_task.stop().await;

    info!(target: "fud", "Stopping get task...");
    get_task_.stop().await;

    info!(target: "fud", "Stopping JSON-RPC server...");
    rpc_task.stop().await;

    info!(target: "fud", "Stopping P2P network...");
    p2p.stop().await;

    info!(target: "fud", "Stopping DHT tasks");
    dht_channel_task.stop().await;
    dht_disconnect_task.stop().await;
    announce_task.stop().await;

    info!("Bye!");
    Ok(())
}
