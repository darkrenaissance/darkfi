/* This file is part of Darkfi (https://dark.fi)
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

use easy_parallel::Parallel;
use log::{debug, error, info, warn};
use smol::{lock::Mutex, stream::StreamExt, Executor};
use std::{collections::HashSet, error, fs::File, io::stdin, sync::Arc};
use url::Url;

use darkfi::{
    async_daemonize, cli_desc, net,
    net::{settings::SettingsOpt, Settings},
    rpc::{
        jsonrpc::JsonSubscriber,
        server::{listen_and_serve, RequestHandler},
    },
    system::StoppableTask,
    util::path::get_config_path,
    Error, Result,
};

use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};

use crate::{
    dchat_error::ErrorMissingSpecifier,
    dchatmsg::{DchatMsg, DchatMsgsBuffer},
    protocol_dchat::ProtocolDchat,
    rpc::JsonRpcInterface,
};

pub mod dchat_error;
pub mod dchatmsg;
pub mod protocol_dchat;
pub mod rpc;

const CONFIG_FILE: &str = "dchat_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../dchat_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "dchat", about = cli_desc!())]
struct Args {
    #[structopt(long, default_value = "tcp://127.0.0.1:55054")]
    /// RPC server listen address
    rpc_listen: Url,

    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    /// P2P network settings
    #[structopt(flatten)]
    net: SettingsOpt,
}

// ANCHOR: dchat
struct Dchat {
    p2p: net::P2pPtr,
    recv_msgs: DchatMsgsBuffer,
}
// ANCHOR_END: dchat

impl Dchat {
    fn new(p2p: net::P2pPtr, recv_msgs: DchatMsgsBuffer) -> Self {
        Self { p2p, recv_msgs }
    }

    // ANCHOR: send
    async fn send(&self, msg: String) -> Result<()> {
        let dchatmsg = DchatMsg { msg };
        self.p2p.broadcast(&dchatmsg).await;
        Ok(())
    }
    // ANCHOR_END: send
}

// ANCHOR: app_settings
#[derive(Clone, Debug)]
struct AppSettings {
    accept_addr: Url,
    net: Settings,
}

impl AppSettings {
    pub fn new(accept_addr: Url, net: Settings) -> Self {
        Self { accept_addr, net }
    }
}
async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    let cfg_path = get_config_path(args.config, CONFIG_FILE)?;
    let toml_contents = std::fs::read_to_string(cfg_path)?;

    // ANCHOR: dchat
    let p2p = net::P2p::new(args.net.into(), ex.clone()).await;
    let msgs: DchatMsgsBuffer = Arc::new(Mutex::new(vec![DchatMsg { msg: String::new() }]));
    let dchat = Dchat::new(p2p.clone(), msgs.clone());

    // ANCHOR: register_protocol
    info!("Registering Dchat protocol");
    let registry = p2p.protocol_registry();
    registry
        .register(!net::session::SESSION_SEED, move |channel, _p2p| {
            let msgs_ = msgs.clone();
            async move { ProtocolDchat::init(channel, msgs_).await }
        })
        .await;
    // ANCHOR_END: register_protocol

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
    // ANCHOR_end: dnet

    // ANCHOR: rpc
    //info!("Starting JSON-RPC server");
    let rpc_connections = Mutex::new(HashSet::new());
    let rpc = Arc::new(JsonRpcInterface { p2p: p2p.clone(), rpc_connections, dnet_sub });
    let _ex = ex.clone();

    let rpc_task = StoppableTask::new();
    rpc_task.clone().start(
        listen_and_serve(args.rpc_listen, rpc.clone(), None, ex.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => rpc.stop_connections().await,
                Err(e) => error!("Failed stopping JSON-RPC server: {}", e),
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );
    // ANCHOR_end: rpc

    info!("Starting P2P network");
    p2p.clone().start().await?;

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!("Caught termination signal, cleaning up and exiting...");

    info!("Stopping P2P network");
    p2p.stop().await;

    info!("Stopping JSON-RPC server");
    rpc_task.stop().await;
    dnet_task.stop().await;

    info!("Shut down successfully");
    Ok(())
}
//// ANCHOR: main
