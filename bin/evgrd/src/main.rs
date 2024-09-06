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

use std::sync::{Arc, Mutex as SyncMutex};
use darkfi::{
    async_daemonize, cli_desc,
    event_graph::{proto::ProtocolEventGraph, EventGraph, EventGraphPtr},
    net::{session::SESSION_DEFAULT, settings::SettingsOpt as NetSettingsOpt, P2p, P2pPtr},
    rpc::{
        jsonrpc::JsonSubscriber,
        server::{listen_and_serve, RequestHandler},
    },
    system::{sleep, StoppableTask, StoppableTaskPtr},
    util::path::{expand_path, get_config_path},
    Error, Result,
};
use darkfi_serial::{
    async_trait, deserialize_async, serialize_async, Encodable, SerialDecodable, SerialEncodable,
};
use log::{debug, error, info};
use rand::rngs::OsRng;
use sled_overlay::sled;
use smol::{fs, lock::Mutex, stream::StreamExt, Executor};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use url::Url;

const CONFIG_FILE: &str = "evgrd.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../evgrd.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "evgrd", about = cli_desc!())]
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

    #[structopt(long, default_value = "tcp://127.0.0.1:26880")]
    /// RPC server listen address
    rpc_listen: Url,

    #[structopt(short, long, default_value = "~/.local/darkfi/evgrd_db")]
    /// Datastore (DB) path
    datastore: String,

    #[structopt(short, long, default_value = "~/.local/darkfi/replayed_evgrd_db")]
    /// Replay logs (DB) path
    replay_datastore: String,

    /// Flag to store Sled DB instructions
    #[structopt(long)]
    replay_mode: bool,

    /// Flag to skip syncing the DAG (no history).
    #[structopt(long)]
    skip_dag_sync: bool,

    /// Number of attempts to sync the DAG.
    #[structopt(long, default_value = "5")]
    sync_attempts: u8,

    /// Number of seconds to wait before trying again if sync fails.
    #[structopt(long, default_value = "10")]
    sync_timeout: u8,

    /// P2P network settings
    #[structopt(flatten)]
    net: NetSettingsOpt,
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<Executor<'static>>) -> Result<()> {
    info!("Starting evgrd node");

    // Create datastore path if not there already.
    let datastore = expand_path(&args.datastore)?;
    fs::create_dir_all(&datastore).await?;

    let replay_datastore = expand_path(&args.replay_datastore)?;
    let replay_mode = args.replay_mode;

    info!("Instantiating event DAG");
    let sled_db = sled::open(datastore)?;
    /*
    let mut p2p_settings: darkfi::net::Settings = args.net.into();
    p2p_settings.app_version = semver::Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
    let p2p = P2p::new(p2p_settings, ex.clone()).await?;
    let event_graph = EventGraph::new(
        p2p.clone(),
        sled_db.clone(),
        replay_datastore.clone(),
        replay_mode,
        "darkirc_dag",
        1,
        ex.clone(),
    )
    .await?;

    let prune_task = event_graph.prune_task.get().unwrap();

    info!("Registering EventGraph P2P protocol");
    let event_graph_ = Arc::clone(&event_graph);
    let registry = p2p.protocol_registry();
    registry
        .register(SESSION_DEFAULT, move |channel, _| {
            let event_graph_ = event_graph_.clone();
            async move { ProtocolEventGraph::init(event_graph_, channel).await.unwrap() }
        })
        .await;

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
                debug!("Got deg event: {:?}", event);
                deg_sub_.notify(vec![event.into()].into()).await;
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

    info!("Starting JSON-RPC server");
    let darkirc = Arc::new(DarkIrc::new(
        p2p.clone(),
        sled_db.clone(),
        event_graph.clone(),
        dnet_sub,
        deg_sub,
        replay_datastore.clone(),
    ));
    let darkirc_ = Arc::clone(&darkirc);
    let rpc_task = StoppableTask::new();
    rpc_task.clone().start(
        listen_and_serve(args.rpc_listen, darkirc.clone(), None, ex.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => darkirc_.stop_connections().await,
                Err(e) => error!("Failed stopping JSON-RPC server: {}", e),
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );

    info!("Starting IRC server");
    let password = args.password.unwrap_or_default();
    let config_path = get_config_path(args.config, CONFIG_FILE)?;
    let irc_server = IrcServer::new(
        darkirc.clone(),
        args.irc_listen,
        args.irc_tls_cert,
        args.irc_tls_secret,
        config_path,
        password,
    )
    .await?;

    let irc_task = StoppableTask::new();
    let ex_ = ex.clone();
    irc_task.clone().start(
        irc_server.clone().listen(ex_),
        |res| async move {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* TODO: */ }
                Err(e) => error!("Failed stopping IRC server: {}", e),
            }
        },
        Error::DetachedTaskStopped,
        ex.clone(),
    );

    info!("Starting P2P network");
    p2p.clone().start().await?;

    info!("Waiting for some P2P connections...");
    sleep(5).await;

    // We'll attempt to sync {sync_attempts} times
    if !args.skip_dag_sync {
        for i in 1..=args.sync_attempts {
            info!("Syncing event DAG (attempt #{})", i);
            match event_graph.dag_sync().await {
                Ok(()) => break,
                Err(e) => {
                    if i == args.sync_attempts {
                        error!("Failed syncing DAG. Exiting.");
                        p2p.stop().await;
                        return Err(Error::DagSyncFailed)
                    } else {
                        // TODO: Maybe at this point we should prune or something?
                        // TODO: Or maybe just tell the user to delete the DAG from FS.
                        error!("Failed syncing DAG ({}), retrying in {}s...", e, args.sync_timeout);
                        sleep(args.sync_timeout.into()).await;
                    }
                }
            }
        }
    } else {
        *event_graph.synced.write().await = true;
    }

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!("Caught termination signal, cleaning up and exiting...");

    info!("Stopping P2P network");
    p2p.stop().await;

    info!("Stopping JSON-RPC server");
    rpc_task.stop().await;
    dnet_task.stop().await;
    deg_task.stop().await;

    info!("Stopping IRC server");
    irc_task.stop().await;
    prune_task.stop().await;

    info!("Flushing sled database...");
    let flushed_bytes = sled_db.flush_async().await?;
    info!("Flushed {} bytes", flushed_bytes);

    info!("Shut down successfully");
    */
    Ok(())
}
