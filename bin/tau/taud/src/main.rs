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

use std::{
    collections::HashMap,
    env,
    fs::{create_dir_all, remove_dir_all},
    io::stdin,
    path::Path,
};

use async_std::sync::{Arc, Mutex};
use crypto_box::{
    aead::{Aead, AeadCore},
    SalsaBox, SecretKey,
};
use darkfi_serial::{deserialize, serialize, SerialDecodable, SerialEncodable};
use futures::{select, FutureExt};
use log::{debug, error, info, warn};
use structopt_toml::StructOptToml;

use darkfi::{
    async_daemonize,
    event_graph::{
        events_queue::EventsQueue,
        get_current_time,
        model::{Event, EventId, Model, ModelPtr},
        protocol_event::{ProtocolEvent, Seen, SeenPtr, UnreadEvents},
        view::{View, ViewPtr},
        EventMsg,
    },
    net::{self, P2pPtr},
    rpc::server::listen_and_serve,
    util::path::expand_path,
    Error, Result,
};

mod error;
mod jsonrpc;
mod month_tasks;
mod settings;
mod task_info;
mod util;

use crate::{
    error::TaudResult,
    jsonrpc::JsonRpcInterface,
    settings::{Args, CONFIG_FILE, CONFIG_FILE_CONTENTS},
    task_info::TaskInfo,
};

fn get_workspaces(settings: &Args) -> Result<HashMap<String, SalsaBox>> {
    let mut workspaces = HashMap::new();

    for workspace in settings.workspaces.iter() {
        let workspace: Vec<&str> = workspace.split(':').collect();
        let (workspace, secret) = (workspace[0], workspace[1]);

        let bytes: [u8; 32] = bs58::decode(secret)
            .into_vec()?
            .try_into()
            .map_err(|_| Error::ParseFailed("Parse secret key failed"))?;

        let secret = crypto_box::SecretKey::from(bytes);
        let public = secret.public_key();
        let salsa_box = crypto_box::SalsaBox::new(&public, &secret);
        workspaces.insert(workspace.to_string(), salsa_box);
    }

    Ok(workspaces)
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct EncryptedTask {
    nonce: Vec<u8>,
    payload: Vec<u8>,
}

impl EventMsg for EncryptedTask {
    fn new() -> Self {
        Self {
            nonce: [
                19, 40, 199, 87, 248, 23, 187, 11, 119, 237, 214, 65, 5, 206, 187, 33, 222, 107,
                140, 84, 114, 61, 205, 40,
            ]
            .to_vec(),
            payload: [
                30, 66, 74, 74, 65, 78, 80, 120, 85, 66, 106, 119, 119, 81, 66, 55, 112, 80, 88,
                85, 82, 97, 79, 108, 115, 83, 113, 78, 71, 116, 113, 4, 114, 111, 111, 116, 1, 0,
                0, 0, 5, 116, 105, 116, 108, 101, 0, 4, 100, 101, 115, 99, 6, 100, 97, 114, 107,
                102, 105, 0, 0, 0, 0, 42, 47, 14, 100, 0, 0, 0, 0, 4, 111, 112, 101, 110, 0, 0,
            ]
            .to_vec(),
        }
    }
}

fn encrypt_task(
    task: &TaskInfo,
    salsa_box: &SalsaBox,
    rng: &mut crypto_box::rand_core::OsRng,
) -> TaudResult<EncryptedTask> {
    debug!("start encrypting task");

    let nonce = SalsaBox::generate_nonce(rng);
    let payload = &serialize(task)[..];
    let payload = salsa_box.encrypt(&nonce, payload)?;

    let nonce = nonce.to_vec();
    Ok(EncryptedTask { nonce, payload })
}

fn decrypt_task(encrypt_task: &EncryptedTask, salsa_box: &SalsaBox) -> TaudResult<TaskInfo> {
    debug!("start decrypting task");

    let nonce = encrypt_task.nonce.as_slice();
    let decrypted_task = salsa_box.decrypt(nonce.into(), &encrypt_task.payload[..])?;

    let task = deserialize(&decrypted_task)?;

    Ok(task)
}

#[allow(clippy::too_many_arguments)]
async fn start_sync_loop(
    broadcast_rcv: smol::channel::Receiver<TaskInfo>,
    view: ViewPtr<EncryptedTask>,
    model: ModelPtr<EncryptedTask>,
    seen: SeenPtr<EventId>,
    workspaces: HashMap<String, SalsaBox>,
    datastore_path: std::path::PathBuf,
    missed_events: Arc<Mutex<Vec<Event<EncryptedTask>>>>,
    p2p: P2pPtr,
) -> TaudResult<()> {
    loop {
        let mut v = view.lock().await;
        select! {
            task_event = broadcast_rcv.recv().fuse() => {
                let tk = task_event.map_err(Error::from)?;
                if workspaces.contains_key(&tk.workspace) {
                    let salsa_box = workspaces.get(&tk.workspace).unwrap();
                    let encrypted_task = encrypt_task(&tk, salsa_box, &mut crypto_box::rand_core::OsRng)?;
                    info!(target: "tau", "Send the task: ref: {}", tk.ref_id);
                    // raft_msgs_sender.send(encrypted_task).await.map_err(Error::from)?;
                    let event = Event {
                        previous_event_hash: model.lock().await.get_head_hash(),
                        action: encrypted_task,
                        timestamp: get_current_time(),
                        read_confirms: 0,
                    };

                    p2p.broadcast(event).await?;

                }
            }
            task_event = v.process().fuse() => {
                let event = task_event.map_err(Error::from)?;
                if !seen.push(&event.hash()).await {
                    continue
                }

                info!("new event: {:?}", event);
                missed_events.lock().await.push(event.clone());

                on_receive_task(&event.action, &datastore_path, &workspaces)
                    .await?;
            }
        }
    }
}

// async fn start_sync_loop(
//     broadcast_rcv: smol::channel::Receiver<TaskInfo>,
//     raft_msgs_sender: smol::channel::Sender<EncryptedTask>,
//     commits_recv: smol::channel::Receiver<EncryptedTask>,
//     datastore_path: std::path::PathBuf,
//     workspaces: HashMap<String, SalsaBox>,
//     mut rng: crypto_box::rand_core::OsRng,
// ) -> TaudResult<()> {
//     loop {
//         select! {
//             task = broadcast_rcv.recv().fuse() => {
//                 let tk = task.map_err(Error::from)?;
//                 if workspaces.contains_key(&tk.workspace) {
//                     let salsa_box = workspaces.get(&tk.workspace).unwrap();
//                     let encrypted_task = encrypt_task(&tk, salsa_box, &mut rng)?;
//                     info!(target: "tau", "Send the task: ref: {}", tk.ref_id);
//                     raft_msgs_sender.send(encrypted_task).await.map_err(Error::from)?;
//                 }
//             }
//             task = commits_recv.recv().fuse() => {
//                 let task = task.map_err(Error::from)?;
//                 on_receive_task(&task,&datastore_path, &workspaces)
//                     .await?;
//             }
//         }
//     }
// }

async fn on_receive_task(
    task: &EncryptedTask,
    datastore_path: &Path,
    workspaces: &HashMap<String, SalsaBox>,
) -> TaudResult<()> {
    for (workspace, salsa_box) in workspaces.iter() {
        let task = decrypt_task(task, salsa_box);
        if let Err(e) = task {
            info!("unable to decrypt the task: {}", e);
            continue
        }

        let mut task = task.unwrap();
        info!(target: "tau", "Save the task: ref: {}", task.ref_id);
        task.workspace = workspace.clone();
        task.save(datastore_path)?;
    }
    Ok(())
}

async_daemonize!(realmain);
async fn realmain(settings: Args, executor: Arc<smol::Executor<'_>>) -> Result<()> {
    let datastore_path = expand_path(&settings.datastore)?;

    let nickname =
        if settings.nickname.is_some() { settings.nickname.clone() } else { env::var("USER").ok() };

    if settings.refresh {
        println!("Removing local data in: {:?} (yes/no)? ", datastore_path);
        let mut confirm = String::new();
        stdin().read_line(&mut confirm).expect("Failed to read line");

        let confirm = confirm.to_lowercase();
        let confirm = confirm.trim();

        if confirm == "yes" || confirm == "y" {
            remove_dir_all(datastore_path).unwrap_or(());
            println!("Local data removed successfully.");
        } else {
            error!("Unexpected Value: {}", confirm);
        }

        return Ok(())
    }

    if nickname.is_none() {
        error!("Provide a nickname in config file");
        return Ok(())
    }

    // mkdir datastore_path if not exists
    create_dir_all(datastore_path.clone())?;
    create_dir_all(datastore_path.join("month"))?;
    create_dir_all(datastore_path.join("task"))?;

    if settings.generate {
        println!("Generating a new workspace");

        loop {
            println!("Name for the new workspace: ");
            let mut workspace = String::new();
            stdin().read_line(&mut workspace).expect("Failed to read line");
            let workspace = workspace.to_lowercase();
            let workspace = workspace.trim();
            if workspace.is_empty() && workspace.len() < 3 {
                error!("Wrong workspace try again");
                continue
            }
            let mut rng = crypto_box::rand_core::OsRng;
            let secret_key = SecretKey::generate(&mut rng);
            let encoded = bs58::encode(secret_key.as_bytes());

            println!("workspace: {}:{}", workspace, encoded.into_string());
            println!("Please add it to the config file.");
            break
        }

        return Ok(())
    }

    let workspaces = get_workspaces(&settings)?;

    if workspaces.is_empty() {
        error!("Please add at least one workspace to the config file.");
        println!("Run `$ taud --generate` to generate new workspace.");
        return Ok(())
    }

    ////////////////////
    // Initialize the base structures
    ////////////////////
    let events_queue = EventsQueue::<EncryptedTask>::new();
    let model = Arc::new(Mutex::new(Model::new(events_queue.clone())));
    let view = Arc::new(Mutex::new(View::new(events_queue)));
    let model_clone = model.clone();

    ////////////////////
    // Buffers
    ////////////////////
    let seen_event = Seen::new();
    let seen_inv = Seen::new();
    let unread_events = UnreadEvents::new();

    // let datastore_raft = datastore_path.join("tau.db");

    let (broadcast_snd, broadcast_rcv) = smol::channel::unbounded::<TaskInfo>();

    //
    // P2p setup
    //
    let mut net_settings = settings.net.clone();
    net_settings.app_version = Some(option_env!("CARGO_PKG_VERSION").unwrap_or("").to_string());

    let p2p = net::P2p::new(net_settings.into()).await;
    // let p2p = p2p.clone();
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

    p2p.clone().start(executor.clone()).await?;

    executor.spawn(p2p.clone().run(executor.clone())).detach();

    ////////////////////
    // Listner
    ////////////////////
    let seen_ids = Seen::new();
    let missed_events = Arc::new(Mutex::new(vec![]));

    executor
        .spawn(start_sync_loop(
            broadcast_rcv,
            view,
            model_clone,
            seen_ids,
            workspaces.clone(),
            datastore_path.clone(),
            missed_events,
            p2p.clone(),
        ))
        .detach();

    //
    // RPC interface
    //
    let rpc_interface = Arc::new(JsonRpcInterface::new(
        datastore_path.clone(),
        broadcast_snd,
        nickname.unwrap(),
        workspaces.clone(),
        p2p.clone(),
    ));
    let _ex = executor.clone();
    executor.spawn(listen_and_serve(settings.rpc_listen.clone(), rpc_interface, _ex)).detach();

    //
    // Waiting Exit signal
    //
    let (signal, shutdown) = smol::channel::bounded::<()>(1);
    ctrlc::set_handler(move || {
        warn!(target: "tau", "Catch exit signal");
        // cleaning up tasks running in the background
        if let Err(e) = async_std::task::block_on(signal.send(())) {
            error!("Error on sending exit signal: {}", e);
        }
    })
    .unwrap();

    shutdown.recv().await?;
    print!("\r");
    info!("Caught termination signal, cleaning up and exiting...");

    Ok(())
}
