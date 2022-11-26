/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use std::fmt;

use async_std::sync::{Arc, Mutex};
use log::{info, warn};
use rand::rngs::OsRng;
use smol::channel::Receiver;
use structopt_toml::StructOptToml;

use darkfi::{
    async_daemonize, net,
    rpc::server::listen_and_serve,
    system::{Subscriber, SubscriberPtr},
    util::{file::save_json_file, path::expand_path},
    Result,
};

pub mod buffers;
pub mod crypto;
pub mod irc;
pub mod model;
pub mod privmsg;
pub mod protocol_privmsg;
pub mod protocol_privmsg2;
pub mod rpc;
pub mod settings;
pub mod view;

use crate::{
    buffers::SeenIds,
    irc::IrcServer,
    privmsg::Privmsg,
    protocol_privmsg::ProtocolPrivmsg,
    rpc::JsonRpcInterface,
    settings::{Args, ChannelInfo, CONFIG_FILE, CONFIG_FILE_CONTENTS},
};

#[derive(serde::Serialize)]
struct KeyPair {
    private_key: String,
    public_key: String,
}

impl fmt::Display for KeyPair {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Public key: {}\nPrivate key: {}", self.public_key, self.private_key)
    }
}

struct Ircd {
    notify_clients: SubscriberPtr<Privmsg>,
}

impl Ircd {
    fn new() -> Self {
        let notify_clients = Subscriber::new();
        Self { notify_clients }
    }

    async fn start(
        &self,
        settings: &Args,
        seen: Arc<Mutex<SeenIds>>,
        p2p: net::P2pPtr,
        p2p_receiver: Receiver<Privmsg>,
        executor: Arc<smol::Executor<'_>>,
    ) -> Result<()> {
        let notify_clients = self.notify_clients.clone();
        executor
            .spawn(async move {
                while let Ok(msg) = p2p_receiver.recv().await {
                    notify_clients.notify(msg).await;
                }
            })
            .detach();

        let irc_server = IrcServer::new(
            settings.clone(),
            seen.clone(),
            p2p.clone(),
            self.notify_clients.clone(),
        )
        .await?;

        let executor_cloned = executor.clone();
        executor
            .spawn(async move {
                irc_server.start(executor_cloned.clone()).await.unwrap();
            })
            .detach();
        Ok(())
    }
}

async_daemonize!(realmain);
async fn realmain(settings: Args, executor: Arc<smol::Executor<'_>>) -> Result<()> {
    let seen = Arc::new(Mutex::new(SeenIds::new()));

    if settings.gen_secret {
        let secret_key = crypto_box::SecretKey::generate(&mut OsRng);
        let encoded = bs58::encode(secret_key.as_bytes());
        println!("{}", encoded.into_string());
        return Ok(())
    }

    if settings.gen_keypair {
        let secret_key = crypto_box::SecretKey::generate(&mut OsRng);
        let pub_key = secret_key.public_key();
        let prv_encoded = bs58::encode(secret_key.as_bytes()).into_string();
        let pub_encoded = bs58::encode(pub_key.as_bytes()).into_string();

        let kp = KeyPair { private_key: prv_encoded, public_key: pub_encoded };

        if settings.output.is_some() {
            let datastore = expand_path(&settings.output.unwrap())?;
            save_json_file(&datastore, &kp)?;
        } else {
            println!("Generated KeyPair:\n{}", kp);
        }

        return Ok(())
    }

    //
    // P2p setup
    //
    let mut net_settings = settings.net.clone();
    net_settings.app_version = Some(option_env!("CARGO_PKG_VERSION").unwrap_or("").to_string());
    let (p2p_send_channel, p2p_recv_channel) = smol::channel::unbounded::<Privmsg>();

    let p2p = net::P2p::new(net_settings.into()).await;
    let p2p2 = p2p.clone();

    let registry = p2p.protocol_registry();

    let seen_c = seen.clone();
    registry
        .register(net::SESSION_ALL, move |channel, p2p| {
            let sender = p2p_send_channel.clone();
            let seen = seen_c.clone();
            async move { ProtocolPrivmsg::init(channel, sender, p2p, seen).await }
        })
        .await;

    p2p.clone().start(executor.clone()).await?;

    let executor_cloned = executor.clone();
    executor_cloned.spawn(p2p.clone().run(executor.clone())).detach();

    // RPC interface
    let rpc_listen_addr = settings.rpc_listen.clone();
    let rpc_interface =
        Arc::new(JsonRpcInterface { addr: rpc_listen_addr.clone(), p2p: p2p.clone() });
    let _ex = executor.clone();
    executor
        .spawn(async move { listen_and_serve(rpc_listen_addr, rpc_interface, _ex).await })
        .detach();

    //
    // IRC instance
    //

    let ircd = Ircd::new();

    ircd.start(&settings, seen, p2p, p2p_recv_channel, executor.clone()).await?;

    // Run once receive exit signal
    let (signal, shutdown) = smol::channel::bounded::<()>(1);
    ctrlc::set_handler(move || {
        warn!(target: "ircd", "ircd start Exit Signal");
        // cleaning up tasks running in the background
        async_std::task::block_on(signal.send(())).unwrap();
    })
    .unwrap();

    // Wait for SIGINT
    shutdown.recv().await?;
    print!("\r");
    info!("Caught termination signal, cleaning up and exiting...");

    p2p2.stop().await;

    Ok(())
}
