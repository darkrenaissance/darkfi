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

// ANCHOR: imports
use smol::{lock::Mutex, stream::StreamExt};
use std::{collections::HashSet, sync::Arc};
use tracing::{debug, error, info};

use darkfi::{
    async_daemonize, cli_desc, net,
    net::settings::SettingsOpt,
    rpc::{
        jsonrpc::JsonSubscriber,
        server::{listen_and_serve, RequestHandler},
        settings::{RpcSettings, RpcSettingsOpt},
    },
    system::{StoppableTask, StoppableTaskPtr},
    Error, Result,
};

use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};

use crate::{
    dchatmsg::{DchatMsg, DchatMsgsBuffer},
    protocol_dchat::ProtocolDchat,
};
// ANCHOR_END: imports

pub mod dchat_error;
pub mod dchatmsg;
pub mod protocol_dchat;
pub mod rpc;

const CONFIG_FILE: &str = "dchatd_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../dchatd_config.toml");

// ANCHOR: args
#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "dchat", about = cli_desc!())]
struct Args {
    #[structopt(flatten)]
    /// JSON-RPC settings
    rpc: RpcSettingsOpt,

    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[structopt(flatten)]
    /// P2P network settings
    net: SettingsOpt,
}
// ANCHOR_END: args

// ANCHOR: dchat
struct Dchat {
    p2p: net::P2pPtr,
    recv_msgs: DchatMsgsBuffer,
    pub rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    pub dnet_sub: JsonSubscriber,
}

impl Dchat {
    fn new(
        p2p: net::P2pPtr,
        recv_msgs: DchatMsgsBuffer,
        rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
        dnet_sub: JsonSubscriber,
    ) -> Self {
        Self { p2p, recv_msgs, rpc_connections, dnet_sub }
    }
}
// ANCHOR_END: dchat

// ANCHOR: main
async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    let p2p_settings: net::Settings =
        (env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), args.net).try_into()?;
    let p2p = net::P2p::new(p2p_settings, ex.clone()).await?;

    // ANCHOR: dnet
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
        ex.clone(),
    );
    // ANCHOR_end: dnet

    // ANCHOR: rpc
    let rpc_settings: RpcSettings = args.rpc.into();
    info!("Starting JSON-RPC server on port {}", rpc_settings.listen);
    let msgs: DchatMsgsBuffer = Arc::new(Mutex::new(vec![DchatMsg { msg: String::new() }]));
    let rpc_connections = Mutex::new(HashSet::new());
    let dchat = Arc::new(Dchat::new(p2p.clone(), msgs.clone(), rpc_connections, dnet_sub));
    let _ex = ex.clone();

    let rpc_task = StoppableTask::new();
    rpc_task.clone().start(
        listen_and_serve(rpc_settings, dchat.clone(), None, ex.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => dchat.stop_connections().await,
                Err(e) => error!("Failed stopping JSON-RPC server: {e}"),
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );
    // ANCHOR_end: rpc

    // ANCHOR: register_protocol
    info!("Registering Dchat protocol");
    let registry = p2p.protocol_registry();
    registry
        .register(net::session::SESSION_DEFAULT, move |channel, _p2p| {
            let msgs_ = msgs.clone();
            async move { ProtocolDchat::init(channel, msgs_).await }
        })
        .await;
    // ANCHOR_END: register_protocol

    // ANCHOR: p2p_start
    info!("Starting P2P network");
    p2p.clone().start().await?;
    // ANCHOR_END: p2p_start

    // ANCHOR: shutdown
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!("Caught termination signal, cleaning up and exiting...");

    info!("Stopping JSON-RPC server");
    rpc_task.stop().await;

    info!("Stopping dnet tasks");
    dnet_task.stop().await;

    info!("Stopping P2P network");
    p2p.stop().await;

    info!("Shut down successfully");
    // ANCHOR_END: shutdown
    Ok(())
}
// ANCHOR_END: main
