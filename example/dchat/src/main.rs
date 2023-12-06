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

use std::{error, fs::File, io::stdin};

// ANCHOR: daemon_deps
use darkfi::system::StoppableTask;
use easy_parallel::Parallel;
use smol::{lock::Mutex, Executor};
use std::{collections::HashSet, sync::Arc};
// ANCHOR_END: daemon_deps

use log::{debug, error};
use simplelog::WriteLogger;
use url::Url;

use darkfi::{
    net,
    net::Settings,
    rpc::{
        jsonrpc::JsonSubscriber,
        server::{listen_and_serve, RequestHandler},
    },
};

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

// ANCHOR: error
pub type DarkfiError = darkfi::Error;
pub type Error = Box<dyn error::Error>;
pub type Result<T> = std::result::Result<T, Error>;
// ANCHOR_END: error

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

    // ANCHOR: menu
    async fn menu(&self) -> Result<()> {
        let mut buffer = String::new();
        let stdin = stdin();
        loop {
            println!(
                "Welcome to dchat.
    s: send message
    i: inbox
    q: quit "
            );
            stdin.read_line(&mut buffer)?;
            // Remove trailing \n
            buffer.pop();
            match buffer.as_str() {
                "q" => return Ok(()),
                "s" => {
                    // Remove trailing s
                    buffer.pop();
                    stdin.read_line(&mut buffer)?;
                    match self.send(buffer.clone()).await {
                        Ok(_) => {
                            println!("you sent: {}", buffer);
                        }
                        Err(e) => {
                            println!("send failed for reason: {}", e);
                        }
                    }
                    buffer.clear();
                }
                "i" => {
                    let msgs = self.recv_msgs.lock().await;
                    if msgs.is_empty() {
                        println!("inbox is empty")
                    } else {
                        println!("received:");
                        for i in msgs.iter() {
                            if !i.msg.is_empty() {
                                println!("{}", i.msg);
                            }
                        }
                    }
                    buffer.clear();
                }
                _ => {}
            }
        }
    }
    // ANCHOR_END: menu

    // ANCHOR: register_protocol
    async fn register_protocol(&self, msgs: DchatMsgsBuffer) -> Result<()> {
        debug!(target: "dchat", "Dchat::register_protocol() [START]");
        let registry = self.p2p.protocol_registry();
        registry
            .register(!net::session::SESSION_SEED, move |channel, _p2p| {
                let msgs2 = msgs.clone();
                async move { ProtocolDchat::init(channel, msgs2).await }
            })
            .await;
        debug!(target: "dchat", "Dchat::register_protocol() [STOP]");
        Ok(())
    }
    // ANCHOR_END: register_protocol

    // ANCHOR: start
    async fn start(&mut self) -> Result<()> {
        debug!(target: "dchat", "Dchat::start() [START]");

        self.register_protocol(self.recv_msgs.clone()).await?;
        self.p2p.clone().start().await?;

        self.menu().await?;

        self.p2p.stop().await;

        debug!(target: "dchat", "Dchat::start() [STOP]");
        Ok(())
    }
    // ANCHOR_END: start

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
// ANCHOR_END: app_settings

// ANCHOR: alice
fn alice() -> Result<AppSettings> {
    let log_level = simplelog::LevelFilter::Debug;
    let log_config = simplelog::Config::default();

    let log_path = "/tmp/alice.log";
    let file = File::create(log_path).unwrap();
    WriteLogger::init(log_level, log_config, file)?;

    let seed = Url::parse("tcp://127.0.0.1:50515").unwrap();
    let inbound = Url::parse("tcp://127.0.0.1:51554").unwrap();
    let ext_addr = Url::parse("tcp://127.0.0.1:51554").unwrap();

    let net = Settings {
        inbound_addrs: vec![inbound],
        external_addrs: vec![ext_addr],
        seeds: vec![seed],
        localnet: true,
        ..Default::default()
    };

    let accept_addr = Url::parse("tcp://127.0.0.1:55054").unwrap();
    let settings = AppSettings::new(accept_addr, net);

    Ok(settings)
}
// ANCHOR_END: alice

// ANCHOR: bob
fn bob() -> Result<AppSettings> {
    let log_level = simplelog::LevelFilter::Debug;
    let log_config = simplelog::Config::default();

    let log_path = "/tmp/bob.log";
    let file = File::create(log_path).unwrap();
    WriteLogger::init(log_level, log_config, file)?;

    let seed = Url::parse("tcp://127.0.0.1:50515").unwrap();

    let net = Settings {
        inbound_addrs: vec![],
        outbound_connections: 5,
        seeds: vec![seed],
        localnet: true,
        ..Default::default()
    };

    let accept_addr = Url::parse("tcp://127.0.0.1:51054").unwrap();
    let settings = AppSettings::new(accept_addr, net);

    Ok(settings)
}
// ANCHOR_END: bob

// ANCHOR: main
fn main() -> Result<()> {
    smol::block_on(async {
        let settings: Result<AppSettings> = match std::env::args().nth(1) {
            Some(id) => match id.as_str() {
                "a" => alice(),
                "b" => bob(),
                _ => Err(ErrorMissingSpecifier.into()),
            },
            None => Err(ErrorMissingSpecifier.into()),
        };

        let settings = settings?.clone();
        let ex = Arc::new(Executor::new());
        let p2p = net::P2p::new(settings.net, ex.clone()).await;
        let msgs: DchatMsgsBuffer = Arc::new(Mutex::new(vec![DchatMsg { msg: String::new() }]));
        let mut dchat = Dchat::new(p2p.clone(), msgs);

        // ANCHOR: dnet_sub
        let dnet_sub = JsonSubscriber::new("dnet.subscribe_events");
        let dnet_sub_ = dnet_sub.clone();
        let p2p_ = p2p.clone();
        let dnet_task = StoppableTask::new();
        dnet_task.clone().start(
            async move {
                let dnet_sub = p2p_.dnet_subscribe().await;
                loop {
                    let event = dnet_sub.receive().await;
                    //debug!("Got dnet event: {:?}", event);
                    dnet_sub_.notify(vec![event.into()].into()).await;
                }
            },
            |res| async {
                match res {
                    Ok(()) | Err(DarkfiError::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => panic!("{}", e),
                }
            },
            DarkfiError::DetachedTaskStopped,
            ex.clone(),
        );
        // ANCHOR_END: dnet_sub

        // ANCHOR: json_init
        let accept_addr = settings.accept_addr.clone();

        let rpc_connections = Mutex::new(HashSet::new());
        let rpc = Arc::new(JsonRpcInterface {
            addr: accept_addr.clone(),
            p2p,
            rpc_connections,
            dnet_sub,
        });
        let _ex = ex.clone();

        let rpc_task = StoppableTask::new();
        rpc_task.clone().start(
            listen_and_serve(accept_addr.clone(), rpc.clone(), None, ex.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(DarkfiError::RpcServerStopped) => rpc.stop_connections().await,
                    Err(e) => error!("Failed stopping JSON-RPC server: {}", e),
                }
            },
            DarkfiError::RpcServerStopped,
            ex.clone(),
        );
        // ANCHOR_END: json_init

        let nthreads = std::thread::available_parallelism().unwrap().get();
        let (signal, shutdown) = smol::channel::unbounded::<()>();

        let (_, result) = Parallel::new()
            .each(0..nthreads, |_| smol::future::block_on(ex.run(shutdown.recv())))
            .finish(|| {
                smol::future::block_on(async move {
                    dchat.start().await?;
                    drop(signal);
                    Ok(())
                })
            });

        result
    })
}
// ANCHOR_END: main
