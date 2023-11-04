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
    ffi::CString,
    fs::{create_dir_all, remove_dir_all},
    io::{stdin, Write},
    path::Path,
    sync::{Arc, OnceLock},
};

use crypto_box::{
    aead::{Aead, AeadCore},
    ChaChaBox, SecretKey,
};
use darkfi_serial::{
    async_trait, deserialize, deserialize_async_partial, serialize, serialize_async,
    SerialDecodable, SerialEncodable,
};
use futures::{select, FutureExt};
use libc::mkfifo;
use log::{debug, error, info};
use rand::rngs::OsRng;
use smol::{fs, lock::RwLock, stream::StreamExt};
use structopt_toml::StructOptToml;
use tinyjson::JsonValue;

use darkfi::{
    async_daemonize,
    event_graph::{
        proto::{EventPut, ProtocolEventGraph},
        Event, EventGraph, EventGraphPtr, NULL_ID,
    },
    net::{P2p, P2pPtr, SESSION_ALL},
    rpc::{
        jsonrpc::JsonSubscriber,
        server::{listen_and_serve, RequestHandler},
    },
    system::{sleep, StoppableTask},
    util::path::expand_path,
    Error, Result,
};

mod jsonrpc;
mod settings;

use taud::{
    error::{TaudError, TaudResult},
    task_info::{TaskEvent, TaskInfo},
    util::pipe_write,
};

use crate::{
    jsonrpc::JsonRpcInterface,
    settings::{Args, CONFIG_FILE, CONFIG_FILE_CONTENTS},
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
    event_graph: EventGraphPtr,
    broadcast_rcv: smol::channel::Receiver<TaskInfo>,
    workspaces: Arc<HashMap<String, ChaChaBox>>,
    datastore_path: std::path::PathBuf,
    piped: bool,
    p2p: P2pPtr,
    last_sent: RwLock<blake3::Hash>,
    seen: OnceLock<sled::Tree>,
) -> TaudResult<()> {
    let incoming = event_graph.event_sub.clone().subscribe().await;
    let seen_events = seen.get().unwrap();

    loop {
        select! {
            task_event = broadcast_rcv.recv().fuse() => {
                let tk = task_event.map_err(Error::from)?;
                if workspaces.contains_key(&tk.workspace) {
                    let chacha_box = workspaces.get(&tk.workspace).unwrap();
                    let encrypted_task = encrypt_task(&tk, chacha_box, &mut OsRng)?;
                    info!(target: "tau", "Send the task: ref: {}", tk.ref_id);
                    // Build a DAG event and return it.
                    let event = Event::new(
                        serialize_async(&encrypted_task).await,
                        event_graph.clone(),
                    )
                    .await;
                    // Update the last sent event.
                    // let event_id = event.id();
                    // *last_sent.write().await = event_id;

                    // If it fails for some reason, for now, we just note it
                    // and pass.
                    if let Err(e) = event_graph.dag_insert(event.clone()).await {
                        error!("[IRC CLIENT] Failed inserting new event to DAG: {}", e);
                    } else {
                        // We sent this, so it should be considered seen.
                        // TODO: should we save task on send or on receive?
                        // on receive better because it's garanteed your event is out there
                        // debug!("Marking event {} as seen", event_id);
                        // seen.get().unwrap().insert(event_id.as_bytes(), &[]).unwrap();

                        // Otherwise, broadcast it
                        p2p.broadcast(&EventPut(event)).await;
                    }
                }
            }
            task_event = incoming.receive().fuse() => {
                let event_id = task_event.id();
                if *last_sent.read().await == event_id {
                    continue
                }

                if seen_events.contains_key(event_id.as_bytes()).unwrap() {
                    continue
                }

                // Try to deserialize the `Event`'s content into a `Privmsg`
                let enc_task: EncryptedTask = match deserialize_async_partial(task_event.content()).await {
                    Ok((v, _)) => v,
                    Err(e) => {
                        error!("[TAUD] Failed deserializing incoming EncryptedTask event: {}", e);
                        continue
                    }
                };
                on_receive_task(&enc_task, &datastore_path, &workspaces, piped)
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
                    let loaded_events = loaded_task.events;
                    let mut events = task.events.clone();
                    events.retain(|ev| !loaded_events.contains(ev));

                    let file = "/tmp/tau_pipe";
                    let mut pipe_write = pipe_write(file)?;
                    let mut task_clone = task.clone();
                    task_clone.events = events;

                    let json: JsonValue = (&task_clone).into();
                    pipe_write.write_all(json.stringify().unwrap().as_bytes())?;
                }
                Err(_) => {
                    let file = "/tmp/tau_pipe";
                    let mut pipe_write = pipe_write(file)?;
                    let mut task_clone = task.clone();

                    task_clone.events.push(TaskEvent::new(
                        "add_task".to_string(),
                        task_clone.owner.clone(),
                        "".to_string(),
                    ));

                    let json: JsonValue = (&task_clone).into();
                    pipe_write.write_all(json.stringify().unwrap().as_bytes())?;
                }
            }
        }
        task.save(datastore_path)?;
    }
    Ok(())
}

async_daemonize!(realmain);
async fn realmain(settings: Args, executor: Arc<smol::Executor<'static>>) -> Result<()> {
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

    info!("Initializing taud node");

    // Create datastore path if not there already.
    let datastore = expand_path(&settings.datastore)?;
    fs::create_dir_all(&datastore).await?;

    info!("Instantiating event DAG");
    let sled_db = sled::open(datastore)?;
    let p2p = P2p::new(settings.net.into(), executor.clone()).await;
    let event_graph =
        EventGraph::new(p2p.clone(), sled_db.clone(), "taud_dag", 0, executor.clone()).await?;

    info!("Registering EventGraph P2P protocol");
    let event_graph_ = Arc::clone(&event_graph);
    let registry = p2p.protocol_registry();
    registry
        .register(SESSION_ALL, move |channel, _| {
            let event_graph_ = event_graph_.clone();
            async move { ProtocolEventGraph::init(event_graph_, channel).await.unwrap() }
        })
        .await;

    let (broadcast_snd, broadcast_rcv) = smol::channel::unbounded::<TaskInfo>();

    info!(target: "taud", "Starting P2P network");
    p2p.clone().start().await?;

    info!(target: "taud", "Waiting for some P2P connections...");
    sleep(5).await;

    // We'll attempt to sync 5 times
    if !settings.skip_dag_sync {
        for i in 1..=6 {
            info!("Syncing event DAG (attempt #{})", i);
            match event_graph.dag_sync().await {
                Ok(()) => break,
                Err(e) => {
                    if i == 6 {
                        error!("Failed syncing DAG. Exiting.");
                        p2p.stop().await;
                        return Err(Error::DagSyncFailed)
                    } else {
                        // TODO: Maybe at this point we should prune or something?
                        // TODO: Or maybe just tell the user to delete the DAG from FS.
                        error!("Failed syncing DAG ({}), retrying in 10s...", e);
                        sleep(10).await;
                    }
                }
            }
        }
    }

    ////////////////////
    // Listner
    ////////////////////
    info!(target: "taud", "Starting sync loop task");
    let last_sent = RwLock::new(NULL_ID);
    let seen = OnceLock::new();
    seen.set(sled_db.open_tree("tau_db").unwrap()).unwrap();

    ////////////////////
    // get history
    ////////////////////
    let dag_events = event_graph.order_events().await;
    let seen_events = seen.get().unwrap();

    for event_id in dag_events.iter() {
        // If it was seen, skip
        if seen_events.contains_key(event_id.as_bytes()).unwrap() {
            continue
        }

        // Get the event from the DAG
        let event = event_graph.dag_get(event_id).await.unwrap().unwrap();

        // Try to deserialize it. (Here we skip errors)
        let Ok((enc_task, _)) = deserialize_async_partial(event.content()).await else { continue };

        // Potentially decrypt the privmsg
        on_receive_task(&enc_task, &datastore_path, &workspaces, false).await.unwrap();

        debug!("Marking event {} as seen", event_id);
        seen_events.insert(event_id.as_bytes(), &[]).unwrap();
    }

    let sync_loop_task = StoppableTask::new();
    sync_loop_task.clone().start(
        start_sync_loop(
            event_graph.clone(),
            broadcast_rcv,
            workspaces.clone(),
            datastore_path.clone(),
            settings.piped,
            p2p.clone(),
            last_sent,
            seen.clone(),
        ),
        |res| async {
            match res {
                Ok(()) | Err(TaudError::Darkfi(Error::DetachedTaskStopped)) => { /* Do nothing */ }
                Err(e) => error!(target: "taud", "Failed starting sync loop task: {}", e),
            }
        },
        TaudError::Darkfi(Error::DetachedTaskStopped),
        executor.clone(),
    );

    // ==============
    // p2p dnet setup
    // ==============
    info!(target: "taud", "Starting dnet subs task");
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
                json_sub_.notify(vec![event.into()].into()).await;
            }
        },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => {
                    error!(target: "taud", "Failed starting dnet subs task: {}", e)
                }
            }
        },
        Error::DetachedTaskStopped,
        executor.clone(),
    );

    //
    // RPC interface
    //
    let rpc_interface = Arc::new(JsonRpcInterface::new(
        datastore_path.clone(),
        broadcast_snd,
        nickname.unwrap(),
        workspaces.clone(),
        p2p.clone(),
        json_sub,
    ));
    let rpc_task = StoppableTask::new();
    rpc_task.clone().start(
        listen_and_serve(settings.rpc_listen, rpc_interface.clone(), None, executor.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => rpc_interface.stop_connections().await,
                Err(e) => error!(target: "taud", "Failed starting JSON-RPC server: {}", e),
            }
        },
        Error::RpcServerStopped,
        executor.clone(),
    );

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(executor)?;
    signals_handler.wait_termination(signals_task).await?;
    info!("Caught termination signal, cleaning up and exiting...");

    info!(target: "taud", "Stopping JSON-RPC server...");
    rpc_task.stop().await;

    info!(target: "taud", "Stopping sync loop task...");
    sync_loop_task.stop().await;

    p2p.stop().await;

    Ok(())
}
