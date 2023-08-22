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

use std::{collections::HashMap, sync::Arc};

use chrono::{Duration, Utc};
use irc::ClientSubMsg;
use log::{debug, error, info};
use rand::rngs::OsRng;
use smol::{fs::create_dir_all, lock::Mutex, stream::StreamExt};
use structopt_toml::StructOptToml;
use tinyjson::JsonValue;

use darkfi::{
    async_daemonize,
    event_graph::{
        events_queue::EventsQueue,
        model::{Model, ModelPtr},
        protocol_event::{ProtocolEvent, Seen},
        view::View,
    },
    net,
    rpc::{jsonrpc::JsonSubscriber, server::listen_and_serve},
    system::{sleep, StoppableTask, Subscriber, SubscriberPtr},
    util::{file::save_json_file, path::expand_path, time::Timestamp},
    Error, Result,
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

// Removes events older than one week ,then sleeps untill next midnight
async fn remove_old_events(model: ModelPtr<PrivMsgEvent>) -> Result<()> {
    loop {
        let now = Utc::now();

        // clocks are valid, safe to unwrap
        let next_midnight = (now + Duration::days(1)).date_naive().and_hms_opt(0, 0, 0).unwrap();

        let duration = next_midnight.signed_duration_since(now.naive_utc()).to_std().unwrap();

        let week_old_datetime =
            (now - Duration::weeks(1)).date_naive().and_hms_opt(0, 0, 0).unwrap();
        let timestamp = week_old_datetime.timestamp() as u64;

        model.lock().await.remove_old_events(Timestamp(timestamp))?;
        info!("Removing old events");

        sleep(duration.as_secs() + 1).await;
    }
}

async_daemonize!(realmain);
async fn realmain(settings: Args, executor: Arc<smol::Executor<'static>>) -> Result<()> {
    let datastore_path = expand_path(&settings.datastore)?;

    // mkdir datastore_path if not exists
    create_dir_all(datastore_path.clone()).await?;

    // Signal handling for config reload and graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(executor.clone())?;
    let client_sub = Subscriber::new();
    executor.spawn(parse_signals(signals_handler.sighup_sub.clone(), client_sub.clone())).detach();

    ////////////////////
    // Generate new keypair and exit
    ////////////////////
    if settings.gen_keypair {
        let secret_key = crypto_box::SecretKey::generate(&mut OsRng);
        let public_key = secret_key.public_key();
        let secret = bs58::encode(secret_key.to_bytes()).into_string();
        let public = bs58::encode(public_key.as_bytes()).into_string();

        let kp = KeyPair { secret, public };

        if settings.output.is_some() {
            let datastore = expand_path(&settings.output.unwrap())?;
            let kp_enc = JsonValue::Object(HashMap::from([
                ("public".to_string(), JsonValue::String(kp.public)),
                ("secret".to_string(), JsonValue::String(kp.secret)),
            ]));
            save_json_file(&datastore, &kp_enc, false)?;
        } else {
            println!("Generated keypair:\n{}", kp);
        }

        return Ok(())
    }

    if settings.secret.is_some() {
        let secret = settings.secret.clone().unwrap();
        let bytes: [u8; 32] = bs58::decode(secret).into_vec()?.try_into().unwrap();
        let secret = crypto_box::SecretKey::from(bytes);
        let pubkey = secret.public_key();
        let pub_encoded = bs58::encode(pubkey.as_bytes()).into_string();

        if settings.output.is_some() {
            let datastore = expand_path(&settings.output.unwrap())?;
            save_json_file(&datastore, &JsonValue::String(pub_encoded), false)?;
        } else {
            println!("Public key recoverd: {}", pub_encoded);
        }

        return Ok(())
    }

    if settings.gen_secret {
        let secret_key = crypto_box::SecretKey::generate(&mut OsRng);
        let encoded = bs58::encode(secret_key.to_bytes());
        println!("{}", encoded.into_string());
        return Ok(())
    }

    ////////////////////
    // Initialize the base structures
    ////////////////////
    let events_queue = EventsQueue::<PrivMsgEvent>::new();
    let model = Arc::new(Mutex::new(Model::new(events_queue.clone())));
    let view = Arc::new(Mutex::new(View::new(events_queue.clone())));
    let model_clone = model.clone();
    let model_clone2 = model.clone();

    {
        // Temporarly load model and check if the loaded head is not
        // older than one week (already removed from other node's tree)
        let now = Utc::now();

        let now_datetime = (now - Duration::weeks(1)).date_naive().and_hms_opt(0, 0, 0).unwrap();
        let timestamp = Timestamp(now_datetime.timestamp() as u64);

        let mut loaded_model = Model::new(events_queue.clone());
        loaded_model.load_tree(&datastore_path)?;

        if loaded_model
            .get_event(&loaded_model.get_head_hash())
            .is_some_and(|event| event.timestamp >= timestamp)
        {
            model.lock().await.load_tree(&datastore_path)?;
        }
    }

    ////////////////////
    // P2p setup
    ////////////////////
    // Buffers
    let seen_event = Seen::new();
    let seen_inv = Seen::new();

    // Check the version
    let net_settings = settings.net.clone();

    // New p2p
    let p2p = net::P2p::new(net_settings.into(), executor.clone()).await;

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

    // ==============
    // p2p dnet setup
    // ==============
    info!(target: "darkirc", "Starting dnet subs task");
    let json_sub = JsonSubscriber::new("dnet.subscribe_events");
    let json_sub_ = json_sub.clone();
    let p2p_ = p2p.clone();
    let dnet_task = StoppableTask::new();
    dnet_task.clone().start(
        async move {
            let dnet_sub = p2p_.dnet_subscribe().await;
            loop {
                let event = dnet_sub.receive().await;
                debug!("Got dnet event: {:?}", event);
                json_sub_.notify(vec![event.into()]).await;
            }
        },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => {
                    error!(target: "darkirc", "Failed starting remove old events task: {}", e)
                }
            }
        },
        Error::DetachedTaskStopped,
        executor.clone(),
    );

    ////////////////////
    // RPC interface setup
    ////////////////////
    let rpc_listen_addr = settings.rpc_listen.clone();
    info!(target: "darkirc", "Starting JSON-RPC server on {}", rpc_listen_addr);
    let rpc_interface = Arc::new(JsonRpcInterface {
        addr: rpc_listen_addr.clone(),
        p2p: p2p.clone(),
        dnet_sub: json_sub,
    });
    let rpc_task = StoppableTask::new();
    rpc_task.clone().start(
        listen_and_serve(rpc_listen_addr, rpc_interface, executor.clone()),
        |res| async {
            match res {
                Ok(()) | Err(Error::RPCServerStopped) => { /* Do nothing */ }
                Err(e) => error!(target: "darkirc", "Failed starting JSON-RPC server: {}", e),
            }
        },
        Error::RPCServerStopped,
        executor.clone(),
    );

    ////////////////////
    // Start P2P network
    ////////////////////
    info!(target: "darkirc", "Starting P2P network");
    p2p.clone().start().await?;
    StoppableTask::new().start(
        p2p.clone().run(),
        |res| async {
            match res {
                Ok(()) | Err(Error::P2PNetworkStopped) => { /* Do nothing */ }
                Err(e) => error!(target: "darkirc", "Failed starting P2P network: {}", e),
            }
        },
        Error::P2PNetworkStopped,
        executor.clone(),
    );

    ////////////////////
    // IRC server
    ////////////////////
    info!(target: "darkirc", "Starting IRC server");
    let irc_server = IrcServer::new(
        settings.clone(),
        p2p.clone(),
        model_clone.clone(),
        view.clone(),
        client_sub,
    )
    .await?;
    let irc_server_task = StoppableTask::new();
    let executor_ = executor.clone();
    irc_server_task.clone().start(
        // Weird hack to prevent lifetimes hell
        async move { irc_server.start(executor_).await },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => error!(target: "darkirc", "Failed starting IRC server: {}", e),
            }
        },
        Error::DetachedTaskStopped,
        executor.clone(),
    );

    // Reset root task
    info!(target: "darkirc", "Starting remove old events task");
    let remove_old_events_task = StoppableTask::new();
    remove_old_events_task.clone().start(
        remove_old_events(model_clone2),
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => {
                    error!(target: "darkirc", "Failed starting remove old events task: {}", e)
                }
            }
        },
        Error::DetachedTaskStopped,
        executor,
    );

    // Wait for termination signal
    signals_handler.wait_termination(signals_task).await?;
    info!("Caught termination signal, cleaning up and exiting...");

    model_clone.lock().await.save_tree(&datastore_path)?;

    info!(target: "darkirc", "Stopping dnet subs task...");
    dnet_task.stop().await;

    info!(target: "darkirc", "Stopping JSON-RPC server...");
    rpc_task.stop().await;

    info!(target: "darkirc", "Stopping P2P network");
    p2p.stop().await;

    info!(target: "darkirc", "Stopping IRC server...");
    irc_server_task.stop().await;

    info!(target: "darkirc", "Stopping remove old events task...");
    remove_old_events_task.stop().await;

    Ok(())
}
