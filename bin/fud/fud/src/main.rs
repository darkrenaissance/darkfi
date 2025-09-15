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
use structopt_toml::StructOptToml;

use darkfi::{
    async_daemonize,
    net::{session::SESSION_DEFAULT, P2p, Settings as NetSettings},
    rpc::{
        jsonrpc::JsonSubscriber,
        server::{listen_and_serve, RequestHandler},
        settings::RpcSettings,
    },
    system::{Publisher, StoppableTask},
    util::path::expand_path,
    Error, Result,
};
use fud::{
    proto::ProtocolFud,
    rpc::JsonRpcInterface,
    settings::{Args, CONFIG_FILE, CONFIG_FILE_CONTENTS},
    Fud,
};

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<Executor<'static>>) -> Result<()> {
    // The working directory for this daemon and geode.
    let basedir = expand_path(&args.base_dir)?;

    // Cloned args
    let args_ = args.clone();

    // Sled database init
    info!(target: "fud", "Instantiating database");
    let sled_db = sled::open(basedir.join("db"))?;

    info!(target: "fud", "Instantiating P2P network");
    let net_settings: NetSettings = args.net.into();
    let p2p = P2p::new(net_settings.clone(), ex.clone()).await?;

    info!(target: "fud", "Starting dnet subs task");
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
        ex.clone(),
    );

    // Daemon instantiation
    let event_pub = Publisher::new();

    let fud: Arc<Fud> =
        Fud::new(args_, p2p.clone(), &sled_db, event_pub.clone(), ex.clone()).await?;

    fud.start_tasks().await;

    info!(target: "fud", "Starting event subs task");
    let event_sub = JsonSubscriber::new("event");
    let event_sub_ = event_sub.clone();
    let event_task = StoppableTask::new();
    event_task.clone().start(
        async move {
            let event_sub = event_pub.clone().subscribe().await;
            loop {
                let event = event_sub.receive().await;
                debug!(target: "fud", "Got event: {event:?}");
                event_sub_.notify(event.into()).await;
            }
        },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => panic!("{e}"),
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
                Err(e) => error!(target: "fud", "Failed starting sync JSON-RPC server: {e}"),
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );

    info!(target: "fud", "Starting P2P protocols");
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

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!(target: "fud", "Caught termination signal, cleaning up and exiting...");

    fud.stop().await;

    info!(target: "fud", "Stopping JSON-RPC server...");
    rpc_task.stop().await;

    info!(target: "fud", "Stopping P2P network...");
    p2p.stop().await;

    info!(target: "fud", "Flushing sled database...");
    let flushed_bytes = sled_db.flush_async().await?;
    info!(target: "fud", "Flushed {flushed_bytes} bytes");

    info!(target: "fud", "Shut down successfully");
    Ok(())
}
