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

use async_std::{
    stream::StreamExt,
    sync::{Arc, Mutex},
    task,
};

use chrono::{Duration, Utc};
use irc::ClientSubMsg;
use log::{debug, info};
use rand::rngs::OsRng;
use structopt_toml::StructOptToml;

use darkfi::{
    async_daemonize,
    event_graph::{
        events_queue::EventsQueue,
        model::{Model, ModelPtr},
        protocol_event::{ProtocolEvent, Seen},
        view::View,
    },
    net,
    rpc::server::listen_and_serve,
    system::{Subscriber, SubscriberPtr},
    util::{async_util::sleep, file::save_json_file, path::expand_path, time::Timestamp},
    Result,
};

pub mod crypto;
pub mod irc;
pub mod privmsg;
pub mod rpc;
pub mod settings;

use crate::{
    crypto::KeyPair,
    irc::{IrcConfig, IrcServer},
    privmsg::PrivMsgEvent,
    rpc::JsonRpcInterface,
    settings::{Args, ChannelInfo, CONFIG_FILE, CONFIG_FILE_CONTENTS},
};

async_daemonize!(realmain);
async fn realmain(settings: Args, executor: Arc<smol::Executor<'_>>) -> Result<()> {
    // Signal handling for config reload and graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new()?;
    let client_sub = Subscriber::new();
    task::spawn(parse_signals(signals_handler.sighup_sub.clone(), client_sub.clone()));

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
    let events_queue = EventsQueue::<PrivMsgEvent>::new();
    let model = Arc::new(Mutex::new(Model::new(events_queue.clone())));
    let view = Arc::new(Mutex::new(View::new(events_queue)));
    let model_clone = model.clone();
    let model_clone2 = model.clone();

    ////////////////////
    // P2p setup
    ////////////////////
    // Buffers
    let seen_event = Seen::new();
    let seen_inv = Seen::new();

    // Check the version
    let net_settings = settings.net.clone();

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
            async move { ProtocolEvent::init(channel, p2p, model, seen_event, seen_inv).await }
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

    // New irc server
    let irc_server =
        IrcServer::new(settings.clone(), p2p.clone(), model_clone, view.clone(), client_sub)
            .await?;

    // Start the irc server and detach it
    let executor_cloned = executor.clone();
    executor.spawn(async move { irc_server.start(executor_cloned).await }).detach();

    // Reset root task
    executor.spawn(async move { reset_root(model_clone2).await }).detach();

    // Wait for termination signal
    signals_handler.wait_termination(signals_task).await?;
    info!("Caught termination signal, cleaning up and exiting...");

    // stop p2p
    p2p2.stop().await;
    Ok(())
}

async fn parse_signals(
    sighup_sub: SubscriberPtr<Args>,
    client_sub: SubscriberPtr<ClientSubMsg>,
) -> Result<()> {
    debug!("Started signal parsing handler");
    let subscription = sighup_sub.subscribe().await;
    loop {
        let args = subscription.receive().await;
        let new_config = IrcConfig::new(&args)?;
        client_sub.notify(ClientSubMsg::Config(new_config)).await;
    }
}

async fn reset_root(model: ModelPtr<PrivMsgEvent>) {
    loop {
        let now = Utc::now();

        // clocks are valid, safe to unwrap
        let next_midnight = (now + Duration::days(1)).date_naive().and_hms_opt(0, 0, 0).unwrap();

        let duration = next_midnight.signed_duration_since(now.naive_utc()).to_std().unwrap();

        // make sure the root is the same as everyone else's at
        // startup by passing today's date 00:00 AM UTC as
        // timestamp to root_event
        let now_datetime = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
        let timestamp = now_datetime.timestamp() as u64;

        model.lock().await.reset_root(Timestamp(timestamp));

        sleep(duration.as_secs()).await;
        info!("Resetting root");
    }
}
