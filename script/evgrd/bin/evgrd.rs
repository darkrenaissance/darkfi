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

use std::{collections::HashSet, convert::TryInto, path::PathBuf, sync::Arc};

use darkfi::{
    async_daemonize, cli_desc,
    event_graph::{
        proto::{EventPut, ProtocolEventGraph},
        Event, EventGraph, EventGraphPtr,
    },
    net::{
        session::SESSION_DEFAULT,
        settings::SettingsOpt as NetSettingsOpt,
        transport::{Listener, PtListener, PtStream},
        P2p, P2pPtr,
    },
    rpc::{
        jsonrpc::JsonSubscriber,
        server::{listen_and_serve, RequestHandler},
        settings::RpcSettingsOpt,
    },
    system::{sleep, StoppableTask, StoppableTaskPtr},
    util::path::expand_path,
    Error, Result,
};
use darkfi_serial::{AsyncDecodable, AsyncEncodable};
use futures::{AsyncWriteExt, FutureExt};
use sled_overlay::sled;
use smol::{fs, lock::Mutex, stream::StreamExt, Executor};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use tracing::{debug, error, info};
use url::Url;

use evgrd::{FetchEventsMessage, VersionMessage, MSG_EVENT, MSG_FETCHEVENTS, MSG_SENDEVENT};

mod rpc;

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

    #[structopt(long, default_value = "tcp://127.0.0.1:5588")]
    /// RPC server listen address
    daemon_listen: Vec<Url>,

    #[structopt(short, long, default_value = "~/.local/share/darkfi/evgrd_db")]
    /// Datastore (DB) path
    datastore: String,

    #[structopt(short, long, default_value = "~/.local/share/darkfi/replayed_evgrd_db")]
    /// Replay logs (DB) path
    replay_datastore: String,

    #[structopt(long)]
    /// Flag to store Sled DB instructions
    replay_mode: bool,

    #[structopt(long)]
    /// Flag to skip syncing the DAG (no history)
    skip_dag_sync: bool,

    #[structopt(long, default_value = "5")]
    /// Number of attempts to sync the DAG
    sync_attempts: u8,

    #[structopt(long, default_value = "15")]
    /// Number of seconds to wait before trying again if sync fails
    sync_timeout: u8,

    #[structopt(flatten)]
    /// P2P network settings
    net: NetSettingsOpt,

    #[structopt(flatten)]
    /// JSON-RPC settings
    rpc: RpcSettingsOpt,
}

pub struct Daemon {
    /// P2P network pointer
    p2p: P2pPtr,
    ///// Sled DB (also used in event_graph and for RLN)
    //sled: sled::Db,
    /// Event Graph instance
    event_graph: EventGraphPtr,
    /// JSON-RPC connection tracker
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    /// dnet JSON-RPC subscriber
    dnet_sub: JsonSubscriber,
    /// deg JSON-RPC subscriber
    deg_sub: JsonSubscriber,
    /// Replay logs (DB) path
    replay_datastore: PathBuf,
}

impl Daemon {
    fn new(
        p2p: P2pPtr,
        //sled: sled::Db,
        event_graph: EventGraphPtr,
        dnet_sub: JsonSubscriber,
        deg_sub: JsonSubscriber,
        replay_datastore: PathBuf,
    ) -> Self {
        Self {
            p2p,
            //sled,
            event_graph,
            rpc_connections: Mutex::new(HashSet::new()),
            dnet_sub,
            deg_sub,
            replay_datastore,
        }
    }
}

async fn rpc_serve(
    listener: Box<dyn PtListener>,
    daemon: Arc<Daemon>,
    ex: Arc<Executor<'_>>,
) -> Result<()> {
    loop {
        match listener.next().await {
            Ok((stream, url)) => {
                info!(target: "evgrd", "Accepted connection from {url}");
                let daemon = daemon.clone();
                ex.spawn(async move {
                    if let Err(e) = handle_connect(stream, daemon).await {
                        error!(target: "evgrd", "Handle connect exited: {e}");
                    }
                })
                .detach();
            }

            // Errors we didn't handle above:
            Err(e) => {
                error!(
                    target: "evgrd",
                    "Unhandled listener.next() error: {}", e,
                );
                continue
            }
        }
    }
}

async fn handle_connect(mut stream: Box<dyn PtStream>, daemon: Arc<Daemon>) -> Result<()> {
    let client_version = VersionMessage::decode_async(&mut stream).await?;
    info!(target: "evgrd", "Client version: {}", client_version.protocol_version);

    let version = VersionMessage::new();
    version.encode_async(&mut stream).await?;
    stream.flush().await?;
    debug!(target: "darkirc", "Sent version: {version:?}");

    let event_sub = daemon.event_graph.event_pub.clone().subscribe().await;

    loop {
        futures::select! {
            ev = event_sub.receive().fuse() => {
                MSG_EVENT.encode_async(&mut stream).await?;
                stream.flush().await?;
                ev.encode_async(&mut stream).await?;
                stream.flush().await?;
            }
            msg_type = u8::decode_async(&mut stream).fuse() => {
                debug!(target: "evgrd", "Received msg_type: {msg_type:?}");
                let msg_type = msg_type?;
                match msg_type {
                    MSG_FETCHEVENTS => fetch_events(&mut stream, &daemon).await?,
                    MSG_SENDEVENT => send_event(&mut stream, &daemon).await?,
                    _ => error!(target: "evgrd", "Skipping unhandled msg_type: {msg_type}")
                }
            }
        }
    }
}

async fn fetch_events(stream: &mut Box<dyn PtStream>, daemon: &Daemon) -> Result<()> {
    let fetchevs = FetchEventsMessage::decode_async(stream).await?;
    info!(target: "evgrd", "Fetch events: {fetchevs:?}");
    let events = daemon.event_graph.fetch_successors_of(fetchevs.unref_tips).await?;

    let n_events = events.len();
    for event in events {
        MSG_EVENT.encode_async(stream).await?;
        stream.flush().await?;
        event.encode_async(stream).await?;
        stream.flush().await?;
    }
    debug!(target: "evgrd", "Sent {n_events} for fetch");
    Ok(())
}

async fn send_event(stream: &mut Box<dyn PtStream>, daemon: &Daemon) -> Result<()> {
    let timestamp = u64::decode_async(stream).await?;
    let content = Vec::<u8>::decode_async(stream).await?;
    info!(target: "evgrd", "send_event: {timestamp}, {content:?}");

    let event = Event::with_timestamp(timestamp, content, &daemon.event_graph).await;
    daemon.event_graph.dag_insert(&[event.clone()]).await.unwrap();

    info!(target: "evgrd", "Broadcasting event put: {event:?}");
    //daemon.p2p.broadcast(&EventPut(event)).await;

    let p2p = daemon.p2p.clone();
    let self_version = p2p.settings().read().await.app_version.clone();
    let connected_peers = p2p.hosts().peers();
    let mut peers_with_matched_version = vec![];
    let mut peers_with_different_version = vec![];
    for peer in connected_peers {
        let peer_version = peer.version.get();
        if let Some(peer_version) = peer_version {
            if self_version == peer_version.version {
                peers_with_matched_version.push(peer)
            } else {
                peers_with_different_version.push(peer)
            }
        }
    }

    if !peers_with_matched_version.is_empty() {
        p2p.broadcast_to(&EventPut(event.clone()), &peers_with_matched_version).await;
    }
    if !peers_with_different_version.is_empty() {
        let mut event = event;
        event.timestamp /= 1000;
        p2p.broadcast_to(&EventPut(event), &peers_with_different_version).await;
    }

    Ok(())
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<Executor<'static>>) -> Result<()> {
    info!(target: "evgrd", "Starting evgrd node");

    // Create datastore path if not there already.
    let datastore = expand_path(&args.datastore)?;
    fs::create_dir_all(&datastore).await?;

    let replay_datastore = expand_path(&args.replay_datastore)?;
    let replay_mode = args.replay_mode;

    info!(target: "evgrd", "Instantiating event DAG");
    let sled_db = sled::open(datastore)?;
    let mut p2p_settings: darkfi::net::Settings =
        (env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), args.net).try_into()?;
    p2p_settings.seeds.push(url::Url::parse("tcp+tls://lilith1.dark.fi:5262").unwrap());
    let p2p = P2p::new(p2p_settings, ex.clone()).await?;
    let event_graph = EventGraph::new(
        p2p.clone(),
        sled_db.clone(),
        replay_datastore.clone(),
        replay_mode,
        "evgrd_dag",
        1,
        ex.clone(),
    )
    .await?;

    // Adding some events
    // for i in 1..6 {
    //     let event = Event::new(vec![1, 2, 3, i], &event_graph).await;
    //     event_graph.dag_insert(&[event.clone()]).await.unwrap();
    // }

    let prune_task = event_graph.prune_task.get().unwrap();

    info!(target: "evgrd", "Registering EventGraph P2P protocol");
    let event_graph_ = Arc::clone(&event_graph);
    let registry = p2p.protocol_registry();
    registry
        .register(SESSION_DEFAULT, move |channel, _| {
            let event_graph_ = event_graph_.clone();
            async move { ProtocolEventGraph::init(event_graph_, channel).await.unwrap() }
        })
        .await;

    info!(target: "evgrd", "Starting dnet subs task");
    let dnet_sub = JsonSubscriber::new("dnet.subscribe_events");
    let dnet_sub_ = dnet_sub.clone();
    let p2p_ = p2p.clone();
    let dnet_task = StoppableTask::new();
    dnet_task.clone().start(
        async move {
            let dnet_sub = p2p_.dnet_subscribe().await;
            loop {
                let event = dnet_sub.receive().await;
                debug!(target: "evgrd", "Got dnet event: {:?}", event);
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

    info!(target: "evgrd", "Starting deg subs task");
    let deg_sub = JsonSubscriber::new("deg.subscribe_events");
    let deg_sub_ = deg_sub.clone();
    let event_graph_ = event_graph.clone();
    let deg_task = StoppableTask::new();
    deg_task.clone().start(
        async move {
            let deg_sub = event_graph_.deg_subscribe().await;
            loop {
                let event = deg_sub.receive().await;
                debug!(target: "evgrd", "Got deg event: {:?}", event);
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

    info!(target: "evgrd", "Starting JSON-RPC server");
    let daemon = Arc::new(Daemon::new(
        p2p.clone(),
        //sled_db.clone(),
        event_graph.clone(),
        dnet_sub,
        deg_sub,
        replay_datastore.clone(),
    ));

    // Used for deg and dnet
    let daemon_ = daemon.clone();
    let rpc_task = StoppableTask::new();
    rpc_task.clone().start(
        listen_and_serve(args.rpc.into(), daemon.clone(), None, ex.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => daemon_.stop_connections().await,
                Err(e) => error!(target: "evgrd", "Failed stopping JSON-RPC server: {}", e),
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );

    info!(target: "evgrd", "Starting evgrd server");
    let mut rpc_tasks = vec![];
    for listen_url in args.daemon_listen {
        let listener = Listener::new(listen_url, None).await?;
        let ptlistener = listener.listen().await?;

        let rpc_task = StoppableTask::new();
        rpc_task.clone().start(
            rpc_serve(ptlistener, daemon.clone(), ex.clone()),
            |res| async move {
                match res {
                    Ok(()) => panic!("Acceptor task should never complete without error status"),
                    //Err(Error::RpcServerStopped) => daemon_.stop_connections().await,
                    Err(e) => error!(target: "evgrd", "Failed stopping RPC server: {}", e),
                }
            },
            Error::RpcServerStopped,
            ex.clone(),
        );
        rpc_tasks.push(rpc_task);
    }

    info!(target: "evgrd", "Starting P2P network");
    p2p.clone().start().await?;

    info!(target: "evgrd", "Waiting for some P2P connections...");
    sleep(5).await;

    // We'll attempt to sync {sync_attempts} times
    if !args.skip_dag_sync {
        for i in 1..=args.sync_attempts {
            info!(target: "evgrd", "Syncing event DAG (attempt #{})", i);
            match event_graph.dag_sync().await {
                Ok(()) => break,
                Err(e) => {
                    if i == args.sync_attempts {
                        error!(target: "evgrd", "Failed syncing DAG. Exiting.");
                        p2p.stop().await;
                        return Err(Error::DagSyncFailed)
                    } else {
                        // TODO: Maybe at this point we should prune or something?
                        // TODO: Or maybe just tell the user to delete the DAG from FS.
                        error!(target: "evgrd", "Failed syncing DAG ({}), retrying in {}s...", e, args.sync_timeout);
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
    info!(target: "evgrd", "Caught termination signal, cleaning up and exiting...");

    info!(target: "evgrd", "Stopping P2P network");
    p2p.stop().await;

    info!(target: "evgrd", "Stopping RPC server");
    for rpc_task in rpc_tasks {
        rpc_task.stop().await;
    }
    dnet_task.stop().await;
    deg_task.stop().await;

    info!(target: "evgrd", "Stopping IRC server");
    prune_task.stop().await;

    info!(target: "evgrd", "Flushing sled database...");
    let flushed_bytes = sled_db.flush_async().await?;
    info!(target: "evgrd", "Flushed {} bytes", flushed_bytes);

    info!(target: "evgrd", "Shut down successfully");
    Ok(())
}
