/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::sync::{Arc, OnceLock};

use darkfi::{
    async_daemonize, cli_desc,
    event_graph::{proto::ProtocolEventGraph, EventGraph, EventGraphPtr, NULL_ID},
    net::{session::SESSION_DEFAULT, settings::SettingsOpt, P2p},
    rpc::{
        jsonrpc::JsonSubscriber,
        server::{listen_and_serve, RequestHandler},
        settings::RpcSettingsOpt,
    },
    system::{sleep, StoppableTask},
    util::path::expand_path,
    Error, Result,
};
use sled_overlay::sled;
use smol::{fs, lock::RwLock, stream::StreamExt};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use tracing::{debug, error, info};

mod rpc;
use rpc::JsonRpcInterface;

const CONFIG_FILE: &str = "genev_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../genev_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "genev", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(flatten)]
    /// JSON-RPC settings
    rpc: RpcSettingsOpt,

    #[structopt(flatten)]
    /// P2P network settings
    net: SettingsOpt,

    #[structopt(long, default_value = "~/.local/share/darkfi/genev_db")]
    /// Sets Datastore Path
    datastore: String,

    #[structopt(short, long, default_value = "~/.local/share/darkfi/replayed_genev_db")]
    /// Replay logs (DB) path
    replay_datastore: String,

    #[structopt(long)]
    /// Flag to store Sled DB instructions
    replay_mode: bool,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(long)]
    /// Flag to skip syncing the DAG (no history)
    skip_dag_sync: bool,

    #[structopt(long)]
    // Whether to sync headers only or full sync
    pub fast_mode: bool,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

async fn start_sync_loop(
    event_graph: EventGraphPtr,
    last_sent: RwLock<blake3::Hash>,
    seen: OnceLock<sled::Tree>,
) -> Result<()> {
    let incoming = event_graph.event_pub.clone().subscribe().await;
    let seen_events = seen.get().unwrap();
    loop {
        let event = incoming.receive().await;
        let event_id = event.header.id();
        if *last_sent.read().await == event_id {
            continue
        }

        if seen_events.contains_key(event_id.as_bytes()).unwrap() {
            continue
        }

        debug!("new event: {event:?}");
    }
}

async_daemonize!(realmain);
async fn realmain(settings: Args, executor: Arc<smol::Executor<'static>>) -> Result<()> {
    ////////////////////
    // Initialize the base structures
    ////////////////////
    info!("Instantiating event DAG");
    // Create datastore path if not there already.
    let datastore_path = expand_path(&settings.datastore)?;
    fs::create_dir_all(&datastore_path).await?;

    let replay_datastore = expand_path(&settings.replay_datastore)?;
    let replay_mode = settings.replay_mode;

    let sled_db = sled::open(datastore_path.clone())?;
    let p2p_settings: darkfi::net::Settings =
        (env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), settings.net).try_into()?;
    let p2p = P2p::new(p2p_settings, executor.clone()).await?;
    let event_graph = EventGraph::new(
        p2p.clone(),
        sled_db.clone(),
        replay_datastore,
        replay_mode,
        "genevd_dag",
        1,
        executor.clone(),
    )
    .await?;

    info!("Registering EventGraph P2P protocol");
    let event_graph_ = Arc::clone(&event_graph);
    let registry = p2p.protocol_registry();
    registry
        .register(SESSION_DEFAULT, move |channel, _| {
            let event_graph_ = event_graph_.clone();
            async move { ProtocolEventGraph::init(event_graph_, channel).await.unwrap() }
        })
        .await;

    // Run
    info!(target: "genevd", "Starting P2P network");
    p2p.clone().start().await?;

    info!(target: "genevd", "Waiting for some P2P connections...");
    sleep(5).await;

    // We'll attempt to sync 5 times
    if !settings.skip_dag_sync {
        for i in 1..=6 {
            info!("Syncing event DAG (attempt #{i})");
            match event_graph.dag_sync(settings.fast_mode).await {
                Ok(()) => break,
                Err(e) => {
                    if i == 6 {
                        error!("Failed syncing DAG. Exiting.");
                        p2p.stop().await;
                        return Err(Error::DagSyncFailed)
                    } else {
                        // TODO: Maybe at this point we should prune or something?
                        // TODO: Or maybe just tell the user to delete the DAG from FS.
                        error!("Failed syncing DAG ({e}), retrying in 10s...");
                        sleep(10).await;
                    }
                }
            }
        }
    } else {
        *event_graph.synced.write().await = true;
    }

    ////////////////////
    // Listner
    ////////////////////
    let last_sent = RwLock::new(NULL_ID);
    let seen = OnceLock::new();
    seen.set(sled_db.open_tree("genevdb").unwrap()).unwrap();

    info!(target: "genevd", "Starting sync loop task");
    let sync_loop_task = StoppableTask::new();
    sync_loop_task.clone().start(
        start_sync_loop(event_graph.clone(), last_sent, seen.clone()),
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => error!(target: "genevd", "Failed starting sync loop task: {e}"),
            }
        },
        Error::DetachedTaskStopped,
        executor.clone(),
    );

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
                debug!("Got dnet event: {event:?}");
                dnet_sub_.notify(vec![event.into()].into()).await;
            }
        },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => panic!("{e}"),
            }
        },
        Error::DetachedTaskStopped,
        executor.clone(),
    );

    info!("Starting deg subs task");
    let deg_sub = JsonSubscriber::new("deg.subscribe_events");
    let deg_sub_ = deg_sub.clone();
    let event_graph_ = event_graph.clone();
    let deg_task = StoppableTask::new();
    deg_task.clone().start(
        async move {
            let deg_sub = event_graph_.deg_subscribe().await;
            loop {
                let event = deg_sub.receive().await;
                debug!("Got deg event: {event:?}");
                deg_sub_.notify(vec![event.into()].into()).await;
            }
        },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => panic!("{e}"),
            }
        },
        Error::DetachedTaskStopped,
        executor.clone(),
    );

    //
    // RPC interface
    //
    let rpc_interface = Arc::new(JsonRpcInterface::new(
        "Alolymous".to_string(),
        event_graph.clone(),
        p2p.clone(),
        dnet_sub,
        deg_sub,
    ));
    let rpc_task = StoppableTask::new();
    let rpc_interface_ = rpc_interface.clone();
    rpc_task.clone().start(
        listen_and_serve(settings.rpc.into(), rpc_interface, None, executor.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => rpc_interface_.stop_connections().await,
                Err(e) => error!(target: "genevd", "Failed starting JSON-RPC server: {e}"),
            }
        },
        Error::RpcServerStopped,
        executor.clone(),
    );

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(executor)?;
    signals_handler.wait_termination(signals_task).await?;
    info!("Caught termination signal, cleaning up and exiting...");

    info!(target: "genevd", "Stopping JSON-RPC server...");
    rpc_task.stop().await;

    info!(target: "genevd", "Stopping Debugging tasks...");
    dnet_task.stop().await;
    deg_task.stop().await;

    info!(target: "genevd", "Stopping sync loop task...");
    sync_loop_task.stop().await;

    // stop p2p
    p2p.stop().await;

    Ok(())
}
