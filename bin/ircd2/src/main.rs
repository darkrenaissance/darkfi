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

use async_std::sync::{Arc, Mutex};
use log::{info, warn};
use rand::rngs::OsRng;
use structopt_toml::StructOptToml;

use darkfi::{
    async_daemonize, net,
    rpc::server::listen_and_serve,
    system::Subscriber,
    util::{file::save_json_file, path::expand_path},
    Result,
};

pub mod crypto;
pub mod events_queue;
pub mod irc;
pub mod model;
pub mod privmsg;
pub mod protocol_event;
pub mod rpc;
pub mod settings;
pub mod view;

use crate::{
    crypto::KeyPair,
    events_queue::EventsQueue,
    irc::IrcServer,
    model::Model,
    protocol_event::{ProtocolEvent, Seen, UnreadEvents},
    rpc::JsonRpcInterface,
    settings::{Args, ChannelInfo, CONFIG_FILE, CONFIG_FILE_CONTENTS},
    view::View,
};

async_daemonize!(realmain);
async fn realmain(settings: Args, executor: Arc<smol::Executor<'_>>) -> Result<()> {
    ////////////////////
    // Generate new keypair and exit
    ////////////////////
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

    ////////////////////
    // Initialize the base structures
    ////////////////////
    let events_queue = EventsQueue::new();
    let model = Arc::new(Mutex::new(Model::new(events_queue.clone())));
    let _view = Arc::new(Mutex::new(View::new(events_queue)));

    ////////////////////
    // P2p setup
    ////////////////////
    // Buffers
    let seen_event = Seen::new();
    let seen_inv = Seen::new();
    let unread_events = UnreadEvents::new();

    // Check the version
    let mut net_settings = settings.net.clone();
    net_settings.app_version = Some(option_env!("CARGO_PKG_VERSION").unwrap_or("").to_string());

    // New p2p
    let p2p = net::P2p::new(net_settings.into()).await;
    let p2p2 = p2p.clone();

    // Register the protocol_event
    let registry = p2p.protocol_registry();
    registry
        .register(net::SESSION_ALL, move |channel, p2p| {
            let seen_event = seen_event.clone();
            let seen_inv = seen_inv.clone();
            let model = model.clone();
            let unread_events = unread_events.clone();
            async move {
                ProtocolEvent::init(channel, p2p, model, seen_event, seen_inv, unread_events).await
            }
        })
        .await;

    // Start
    p2p.clone().start(executor.clone()).await?;

    // Run
    let executor_cloned = executor.clone();
    executor_cloned.spawn(p2p.clone().run(executor.clone())).detach();

    ////////////////////
    // RPC interface setup
    ////////////////////
    let rpc_listen_addr = settings.rpc_listen.clone();
    let rpc_interface =
        Arc::new(JsonRpcInterface { addr: rpc_listen_addr.clone(), p2p: p2p.clone() });
    let _ex = executor.clone();
    executor
        .spawn(async move { listen_and_serve(rpc_listen_addr, rpc_interface, _ex).await })
        .detach();

    ////////////////////
    // IRC server
    ////////////////////
    let clients_subscriptions = Subscriber::new();

    // New irc server
    let irc_server = IrcServer::new(settings.clone(), clients_subscriptions).await?;

    // Start the irc server and detach it
    let executor_cloned = executor.clone();
    executor_cloned.spawn(async move { irc_server.start(executor.clone()).await }).detach();

    ////////////////////
    // Wait for SIGINT
    ////////////////////
    let (signal, shutdown) = smol::channel::bounded::<()>(1);
    ctrlc::set_handler(move || {
        warn!(target: "ircd", "ircd start Exit Signal");
        // cleaning up tasks running in the background
        async_std::task::block_on(signal.send(())).unwrap();
    })
    .unwrap();

    shutdown.recv().await?;
    print!("\r");
    info!("Caught termination signal, cleaning up and exiting...");

    // stop p2p
    p2p2.stop().await;
    Ok(())
}
