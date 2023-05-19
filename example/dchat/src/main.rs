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

use std::{error, fs::File, io::stdin};

// ANCHOR: daemon_deps
use async_std::sync::{Arc, Mutex};
use easy_parallel::Parallel;
use smol::Executor;
// ANCHOR_END: daemon_deps
use log::debug;
use simplelog::WriteLogger;
use url::Url;

use darkfi::{net, net::Settings, rpc::server::listen_and_serve};

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
            .register(!net::SESSION_SEED, move |channel, _p2p| {
                let msgs2 = msgs.clone();
                async move { ProtocolDchat::init(channel, msgs2).await }
            })
            .await;
        debug!(target: "dchat", "Dchat::register_protocol() [STOP]");
        Ok(())
    }
    // ANCHOR_END: register_protocol

    // ANCHOR: start
    async fn start(&mut self, ex: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "dchat", "Dchat::start() [START]");

        let ex2 = ex.clone();

        self.register_protocol(self.recv_msgs.clone()).await?;
        self.p2p.clone().start(ex.clone()).await?;
        ex2.spawn(self.p2p.clone().run(ex.clone())).detach();

        self.menu().await?;

        self.p2p.stop().await;

        debug!(target: "dchat", "Dchat::start() [STOP]");
        Ok(())
    }
    // ANCHOR_END: start

    // ANCHOR: send
    async fn send(&self, msg: String) -> Result<()> {
        let dchatmsg = DchatMsg { msg };
        self.p2p.broadcast(dchatmsg).await?;
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
        inbound: vec![inbound],
        external_addr: vec![ext_addr],
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
        inbound: vec![],
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
#[async_std::main]
async fn main() -> Result<()> {
    let settings: Result<AppSettings> = match std::env::args().nth(1) {
        Some(id) => match id.as_str() {
            "a" => alice(),
            "b" => bob(),
            _ => Err(ErrorMissingSpecifier.into()),
        },
        None => Err(ErrorMissingSpecifier.into()),
    };

    let settings = settings?.clone();

    let p2p = net::P2p::new(settings.net).await;

    let ex = Arc::new(Executor::new());
    let ex2 = ex.clone();
    let ex3 = ex2.clone();

    let msgs: DchatMsgsBuffer = Arc::new(Mutex::new(vec![DchatMsg { msg: String::new() }]));

    let mut dchat = Dchat::new(p2p.clone(), msgs);

    // ANCHOR: json_init
    let accept_addr = settings.accept_addr.clone();
    let rpc = Arc::new(JsonRpcInterface { addr: accept_addr.clone(), p2p });
    let _ex = ex.clone();
    ex.spawn(async move { listen_and_serve(accept_addr.clone(), rpc, _ex).await }).detach();
    // ANCHOR_END: json_init

    let nthreads = std::thread::available_parallelism().unwrap().get();
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    let (_, result) = Parallel::new()
        .each(0..nthreads, |_| smol::future::block_on(ex2.run(shutdown.recv())))
        .finish(|| {
            smol::future::block_on(async move {
                dchat.start(ex3).await?;
                drop(signal);
                Ok(())
            })
        });

    result
}
// ANCHOR_END: main
