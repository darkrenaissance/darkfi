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
};
use libc::mkfifo;
use std::{
    collections::HashMap,
    env,
    ffi::CString,
    fs::{create_dir_all, remove_dir_all},
    io::{stdin, Write},
    path::Path,
};

use crypto_box::{
    aead::{Aead, AeadCore},
    ChaChaBox, SecretKey,
};
use darkfi_serial::{deserialize, serialize, SerialDecodable, SerialEncodable};
use futures::{select, FutureExt};
use log::{debug, error, info};
use rand::rngs::OsRng;
use structopt_toml::StructOptToml;

use darkfi::{
    async_daemonize,
    event_graph::{
        events_queue::EventsQueue,
        model::{Event, EventId, Model, ModelPtr},
        protocol_event::{ProtocolEvent, Seen, SeenPtr},
        view::{View, ViewPtr},
        EventMsg,
    },
    net::{self, P2pPtr},
    rpc::server::listen_and_serve,
    util::{path::expand_path, time::Timestamp},
    Error, Result,
};

mod error;
mod jsonrpc;
mod month_tasks;
mod settings;
mod task_info;
mod util;

use crate::{
    error::{TaudError, TaudResult},
    jsonrpc::JsonRpcInterface,
    settings::{Args, CONFIG_FILE, CONFIG_FILE_CONTENTS},
    task_info::{TaskEvent, TaskInfo},
    util::pipe_write,
};

fn get_workspaces(settings: &Args) -> Result<HashMap<String, ChaChaBox>> {
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
        let chacha_box = crypto_box::ChaChaBox::new(&public, &secret);
        workspaces.insert(workspace.to_string(), chacha_box);
    }

    Ok(workspaces)
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct EncryptedTask {
    payload: String,
}

impl EventMsg for EncryptedTask {
    fn new() -> Self {
        Self { payload: String::from("root") }
    }
}

fn encrypt_task(
    task: &TaskInfo,
    chacha_box: &ChaChaBox,
    rng: &mut OsRng,
) -> TaudResult<EncryptedTask> {
    debug!("start encrypting task");

    let nonce = ChaChaBox::generate_nonce(rng);
    let payload = &serialize(task)[..];
    let mut payload = chacha_box.encrypt(&nonce, payload)?;

    let mut concat = vec![];
    concat.append(&mut nonce.as_slice().to_vec());
    concat.append(&mut payload);

    let payload = bs58::encode(concat.clone()).into_string();

    Ok(EncryptedTask { payload })
}

fn try_decrypt_task(encrypt_task: &EncryptedTask, chacha_box: &ChaChaBox) -> TaudResult<TaskInfo> {
    debug!("start decrypting task");

    let bytes = match bs58::decode(&encrypt_task.payload).into_vec() {
        Ok(v) => v,
        Err(_) => return Err(TaudError::DecryptionError("Error decoding payload".to_string())),
    };

    if bytes.len() < 25 {
        return Err(TaudError::DecryptionError("Invalid bytes length".to_string()))
    }

    // Try extracting the nonce
    let nonce = match bytes[0..24].try_into() {
        Ok(v) => v,
        Err(_) => return Err(TaudError::DecryptionError("Invalid nonce".to_string())),
    };

    // Take the remaining ciphertext
    let message = &bytes[24..];

    // let nonce = encrypt_task.nonce.as_slice();
    let decrypted_task = chacha_box.decrypt(nonce, message)?;

    let task = deserialize(&decrypted_task)?;

    Ok(task)
}

#[allow(clippy::too_many_arguments)]
async fn start_sync_loop(
    broadcast_rcv: smol::channel::Receiver<TaskInfo>,
    view: ViewPtr<EncryptedTask>,
    model: ModelPtr<EncryptedTask>,
    seen: SeenPtr<EventId>,
    workspaces: Arc<HashMap<String, ChaChaBox>>,
    datastore_path: std::path::PathBuf,
    missed_events: Arc<Mutex<Vec<Event<EncryptedTask>>>>,
    piped: bool,
    p2p: P2pPtr,
) -> TaudResult<()> {
    loop {
        let mut v = view.lock().await;
        select! {
            task_event = broadcast_rcv.recv().fuse() => {
                let tk = task_event.map_err(Error::from)?;
                if workspaces.contains_key(&tk.workspace) {
                    let chacha_box = workspaces.get(&tk.workspace).unwrap();
                    let encrypted_task = encrypt_task(&tk, chacha_box, &mut OsRng)?;
                    info!(target: "tau", "Send the task: ref: {}", tk.ref_id);
                    let event = Event {
                        previous_event_hash: model.lock().await.get_head_hash(),
                        action: encrypted_task,
                        timestamp: Timestamp::current_time(),
                    };

                    p2p.broadcast(&event).await;

                }
            }
            task_event = v.process().fuse() => {
                let event = task_event.map_err(Error::from)?;
                if !seen.push(&event.hash()).await {
                    continue
                }

                missed_events.lock().await.push(event.clone());

                on_receive_task(&event.action, &datastore_path, &workspaces, piped)
                    .await?;
            }
        }
    }
}

async fn on_receive_task(
    task: &EncryptedTask,
    datastore_path: &Path,
    workspaces: &HashMap<String, ChaChaBox>,
    piped: bool,
) -> TaudResult<()> {
    for (workspace, chacha_box) in workspaces.iter() {
        let task = try_decrypt_task(task, chacha_box);
        if let Err(e) = task {
            debug!("unable to decrypt the task: {}", e);
            continue
        }

        let mut task = task.unwrap();
        info!(target: "tau", "Save the task: ref: {}", task.ref_id);
        task.workspace = workspace.clone();
        if piped {
            // if we can't load the task then it's a new task.
            // otherwise it's a modification.
            match TaskInfo::load(&task.ref_id, datastore_path) {
                Ok(loaded_task) => {
                    let loaded_events = loaded_task.events.0;
                    let mut events = task.events.0.clone();
                    events.retain(|ev| !loaded_events.contains(ev));

                    let file = "/tmp/tau_pipe";
                    let mut pipe_write = pipe_write(file)?;
                    let mut task_clone = task.clone();
                    task_clone.events.0 = events;

                    let json = serde_json::to_string(&task_clone).unwrap();
                    pipe_write.write_all(json.as_bytes())?;
                }
                Err(_) => {
                    let file = "/tmp/tau_pipe";
                    let mut pipe_write = pipe_write(file)?;
                    let mut task_clone = task.clone();

                    task_clone.events.0.push(TaskEvent::new(
                        "add_task".to_string(),
                        task_clone.owner.clone(),
                        "".to_string(),
                    ));

                    let json = serde_json::to_string(&task_clone).unwrap();
                    pipe_write.write_all(json.as_bytes())?;
                }
            }
        }
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

    if settings.piped {
        let file = "/tmp/tau_pipe";
        let path = CString::new(file).unwrap();
        unsafe { mkfifo(path.as_ptr(), 0o644) };
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
            let secret_key = SecretKey::generate(&mut OsRng);
            let encoded = bs58::encode(secret_key.to_bytes());

            println!("workspace: {}:{}", workspace, encoded.into_string());
            println!("Please add it to the config file.");
            break
        }

        return Ok(())
    }

    let workspaces = Arc::new(get_workspaces(&settings)?);

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

    model.lock().await.load_tree(&datastore_path)?;

    ////////////////////
    // Buffers
    ////////////////////
    let seen_event = Seen::new();
    let seen_inv = Seen::new();

    let (broadcast_snd, broadcast_rcv) = smol::channel::unbounded::<TaskInfo>();

    //
    // P2p setup
    //
    let net_settings = settings.net.clone();

    let p2p = net::P2p::new(net_settings.into()).await;
    let registry = p2p.protocol_registry();

    registry
        .register(net::SESSION_ALL, move |channel, p2p| {
            let seen_event = seen_event.clone();
            let seen_inv = seen_inv.clone();
            let model = model.clone();
            async move { ProtocolEvent::init(channel, p2p, model, seen_event, seen_inv).await }
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
            model_clone.clone(),
            seen_ids,
            workspaces.clone(),
            datastore_path.clone(),
            missed_events,
            settings.piped,
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

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new()?;
    signals_handler.wait_termination(signals_task).await?;
    info!("Caught termination signal, cleaning up and exiting...");

    model_clone.lock().await.save_tree(&datastore_path)?;

    p2p.stop().await;

    Ok(())
}
