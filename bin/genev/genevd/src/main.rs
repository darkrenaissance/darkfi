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

use std::sync::{Arc, OnceLock};

use darkfi::{
    async_daemonize, cli_desc,
    event_graph::{proto::ProtocolEventGraph, EventGraph, EventGraphPtr, NULL_ID},
    net::{settings::SettingsOpt, P2p, SESSION_ALL},
    rpc::server::{listen_and_serve, RequestHandler},
    system::{sleep, StoppableTask},
    util::path::expand_path,
    Error, Result,
};
use log::{debug, error, info};
use smol::{lock::RwLock, stream::StreamExt};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use url::Url;

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

    /// JSON-RPC listen URL
    #[structopt(long = "rpc", default_value = "tcp://127.0.0.1:28880")]
    pub rpc_listen: Url,

    #[structopt(flatten)]
    pub net: SettingsOpt,

    /// Sets Datastore Path
    #[structopt(long, default_value = "~/.local/darkfi/genev_db")]
    pub datastore: String,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(long)]
    pub skip_dag_sync: bool,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

async fn start_sync_loop(
    event_graph: EventGraphPtr,
    last_sent: RwLock<blake3::Hash>,
    seen: OnceLock<sled::Tree>,
) -> Result<()> {
    let incoming = event_graph.event_sub.clone().subscribe().await;
    let seen_events = seen.get().unwrap();
    loop {
        let event = incoming.receive().await;
        let event_id = event.id();
        if *last_sent.read().await == event_id {
            continue
        }

        if seen_events.contains_key(event_id.as_bytes()).unwrap() {
            continue
        }

        debug!("new event: {:?}", event);
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

    let sled_db = sled::open(datastore_path.clone())?;
    let p2p = P2p::new(settings.net.into(), executor.clone()).await;
    let event_graph =
        EventGraph::new(p2p.clone(), sled_db.clone(), "genevd_dag", 1, executor.clone()).await?;

    info!("Registering EventGraph P2P protocol");
    let event_graph_ = Arc::clone(&event_graph);
    let registry = p2p.protocol_registry();
    registry
        .register(SESSION_ALL, move |channel, _| {
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
            info!("Syncing event DAG (attempt #{})", i);
            match event_graph.dag_sync().await {
                Ok(()) => break,
                Err(e) => {
                    if i == 6 {
                        error!("Failed syncing DAG. Exiting.");
                        p2p.stop().await;
                        return Err(Error::DagSyncFailed)
                    } else {
                        // TODO: Maybe at this point we should prune or something?
                        // TODO: Or maybe just tell the user to delete the DAG from FS.
                        error!("Failed syncing DAG ({}), retrying in 10s...", e);
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
                Err(e) => error!(target: "genevd", "Failed starting sync loop task: {}", e),
            }
        },
        Error::DetachedTaskStopped,
        executor.clone(),
    );

    //
    // RPC interface
    //
    let rpc_interface =
        Arc::new(JsonRpcInterface::new("Alolymous".to_string(), event_graph.clone(), p2p.clone()));
    let rpc_task = StoppableTask::new();
    let rpc_interface_ = rpc_interface.clone();
    rpc_task.clone().start(
        listen_and_serve(settings.rpc_listen, rpc_interface, None, executor.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => rpc_interface_.stop_connections().await,
                Err(e) => error!(target: "genevd", "Failed starting JSON-RPC server: {}", e),
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

    info!(target: "genevd", "Stopping sync loop task...");
    sync_loop_task.stop().await;

    // stop p2p
    p2p.stop().await;

    Ok(())
}
