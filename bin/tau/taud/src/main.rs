/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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
    slice,
    str::FromStr,
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
use rand::rngs::OsRng;
use sled_overlay::sled;
use smol::{fs, stream::StreamExt};
use structopt_toml::StructOptToml;
use tinyjson::JsonValue;
use tracing::{debug, error, info};

use darkfi::{
    async_daemonize,
    event_graph::{
        proto::{EventPut, ProtocolEventGraph},
        Event, EventGraph, EventGraphPtr,
    },
    net::{session::SESSION_DEFAULT, P2p, P2pPtr},
    rpc::{
        jsonrpc::JsonSubscriber,
        server::{listen_and_serve, RequestHandler},
    },
    system::{sleep, StoppableTask},
    util::path::{expand_path, get_config_path},
    Error, Result,
};

use darkfi_sdk::crypto::{
    schnorr::{SchnorrPublic, SchnorrSecret, Signature},
    Keypair, PublicKey,
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

struct Workspace {
    read_key: ChaChaBox,
    write_key: Option<darkfi_sdk::crypto::SecretKey>,
    write_pubkey: PublicKey,
}

impl Workspace {
    fn new() -> Self {
        let secret_key = SecretKey::generate(&mut OsRng);
        let keypair = Keypair::default();
        Self {
            read_key: ChaChaBox::new(&secret_key.public_key(), &secret_key),
            write_key: None,
            write_pubkey: keypair.public,
        }
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct EncryptedTask {
    payload: String,
}

#[derive(SerialEncodable, SerialDecodable)]
struct SignedTask {
    task: Vec<u8>,
    signature: Signature,
}

impl SignedTask {
    fn new(task: &TaskInfo, signature: Signature) -> Self {
        Self { task: serialize(task), signature }
    }
}

/// Sign then encrypt a task
fn encrypt_sign_task(task: &TaskInfo, workspace: &Workspace) -> TaudResult<EncryptedTask> {
    debug!(target: "taud", "start encrypting task");
    if workspace.write_key.is_none() {
        error!(target: "taud", "You don't have write access")
    }
    let signature: Signature = workspace.write_key.as_ref().unwrap().sign(&serialize(task)[..]);
    let signed_task = SignedTask::new(task, signature);

    let nonce = ChaChaBox::generate_nonce(&mut OsRng);
    let payload = &serialize(&signed_task)[..];
    let mut payload = workspace.read_key.encrypt(&nonce, payload)?;

    let mut concat = vec![];
    concat.append(&mut nonce.as_slice().to_vec());
    concat.append(&mut payload);

    let payload = bs58::encode(concat.clone()).into_string();

    Ok(EncryptedTask { payload })
}

fn try_decrypt_task(
    encrypt_task: &EncryptedTask,
    chacha_box: &ChaChaBox,
) -> TaudResult<SignedTask> {
    debug!(target: "taud", "start decrypting task");

    let bytes = match bs58::decode(&encrypt_task.payload).into_vec() {
        Ok(v) => v,
        Err(_) => return Err(TaudError::DecryptionError("Error decoding payload".to_string())),
    };

    if bytes.len() < 25 {
        return Err(TaudError::DecryptionError("Invalid bytes length".to_string()))
    }

    // Try extracting the nonce
    let nonce = bytes[0..24].into();

    // Take the remaining ciphertext
    let message = &bytes[24..];

    // let nonce = encrypt_task.nonce.as_slice();
    let decrypted_task = chacha_box.decrypt(nonce, message)?;

    let signed_task = deserialize(&decrypted_task)?;

    Ok(signed_task)
}

fn parse_configured_workspaces(data: &toml::Value) -> Result<HashMap<String, Workspace>> {
    let mut ret = HashMap::new();

    let Some(table) = data.as_table() else { return Err(Error::ParseFailed("TOML not a map")) };
    let Some(workspace) = table.get("workspace") else { return Ok(ret) };
    let Some(workspace) = workspace.as_table() else {
        return Err(Error::ParseFailed("`workspace` not a map"))
    };

    for (name, items) in workspace {
        let mut ws = Workspace::new();

        if let Some(read_key) = items.get("read_key") {
            if let Some(read_key) = read_key.as_str() {
                let Ok(read_key_bytes) = bs58::decode(read_key).into_vec() else {
                    return Err(Error::ParseFailed("Workspace secret not valid base58"))
                };

                if read_key_bytes.len() != 32 {
                    return Err(Error::ParseFailed("Workspace read_key not 32 bytes long"))
                }

                let read_key_bytes: [u8; 32] = read_key_bytes.try_into().unwrap();
                let read_key = crypto_box::SecretKey::from(read_key_bytes);
                let public = read_key.public_key();
                ws.read_key = ChaChaBox::new(&public, &read_key);
            } else {
                return Err(Error::ParseFailed("Workspace read_key not a string"))
            }
        } else {
            return Err(Error::ParseFailed("Workspace read_key is not set"))
        }

        if let Some(write_pubkey) = items.get("write_public_key") {
            if let Some(write_pubkey) = write_pubkey.as_str() {
                if !write_pubkey.is_empty() {
                    info!(target: "taud", "Found configured write_public_key for {name} workspace");
                    let write_key = PublicKey::from_str(write_pubkey).unwrap();
                    // let write_pubkey = write_pubkey.to_string();
                    // let decoded_write_pubkey = bs58::decode(write_pubkey).into_vec().unwrap();
                    ws.write_pubkey = write_key;
                }
            } else {
                return Err(Error::ParseFailed("Workspace write_public_key not a string"))
            }
        } else {
            return Err(Error::ParseFailed("Workspace write_public_key is not set"))
        }

        if let Some(write_key) = items.get("write_key") {
            if let Some(write_key) = write_key.as_str() {
                if !write_key.is_empty() {
                    info!(target: "taud", "Found configured write_key for {name} workspace");
                    let write_key = write_key.to_string();
                    let write_key_bytes = bs58::decode(write_key).into_vec().unwrap();
                    let secret = match darkfi_sdk::crypto::SecretKey::from_bytes(
                        write_key_bytes.try_into().unwrap(),
                    ) {
                        Ok(key) => key,
                        Err(e) => {
                            error!(target: "taud", "Failed parsing write_key: {e}");
                            return Err(Error::ParseFailed("Failed parsing write_key"))
                        }
                    };
                    ws.write_key = Some(secret);
                }
            } else {
                return Err(Error::ParseFailed("Workspace write_key not a string"))
            }
        }

        if let Some(wrt_key) = ws.write_key.as_ref() {
            let pk = PublicKey::from_secret(*wrt_key);
            if pk != ws.write_pubkey {
                error!(target: "taud", "Wrong keypair for {name} workspace, the workspace is not added!");
                continue
            }
        }

        info!(target: "taud", "Configured NaCl box for workspace {name}");
        ret.insert(name.to_string(), ws);
    }

    Ok(ret)
}

async fn get_workspaces(settings: &Args) -> Result<HashMap<String, Workspace>> {
    let config_path = get_config_path(settings.config.clone(), CONFIG_FILE)?;
    let contents = fs::read_to_string(config_path).await?;
    let contents = match toml::from_str(&contents) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "taud", "Failed parsing TOML config: {e}");
            return Err(Error::ParseFailed("Failed parsing TOML config"))
        }
    };

    let workspaces = parse_configured_workspaces(&contents)?;

    Ok(workspaces)
}

/// Atomically mark a message as seen.
pub async fn mark_seen(
    sled_db: sled::Db,
    seen: OnceLock<sled::Tree>,
    event_id: &blake3::Hash,
) -> Result<()> {
    let db = seen.get_or_init(|| sled_db.open_tree("tau_seen").unwrap());

    debug!(target: "taud", "Marking event {event_id} as seen");
    let mut batch = sled::Batch::default();
    batch.insert(event_id.as_bytes(), &[]);
    Ok(db.apply_batch(batch)?)
}

/// Check if a message was already marked seen.
pub async fn is_seen(
    sled_db: sled::Db,
    seen: OnceLock<sled::Tree>,
    event_id: &blake3::Hash,
) -> Result<bool> {
    let db = seen.get_or_init(|| sled_db.open_tree("tau_seen").unwrap());

    Ok(db.contains_key(event_id.as_bytes())?)
}

#[allow(clippy::too_many_arguments)]
async fn start_sync_loop(
    event_graph: EventGraphPtr,
    broadcast_rcv: smol::channel::Receiver<TaskInfo>,
    workspaces: Arc<HashMap<String, Workspace>>,
    sled_db: sled::Db,
    settings: Args,
    p2p: P2pPtr,
    seen: OnceLock<sled::Tree>,
) -> TaudResult<()> {
    let incoming = event_graph.event_pub.clone().subscribe().await;

    loop {
        select! {
            // Process message from Tau client
            task_event = broadcast_rcv.recv().fuse() => {
                let tk = task_event.map_err(Error::from)?;
                if workspaces.contains_key(&tk.workspace) {
                    let ws = workspaces.get(&tk.workspace).unwrap();
                    let encrypted_task = encrypt_sign_task(&tk, ws)?;
                    info!(target: "taud", "Send the task: ref: {}", tk.ref_id);
                    // Build a DAG event and return it.
                    let event = Event::new(
                        serialize_async(&encrypted_task).await,
                        &event_graph,
                    )
                    .await;

                    // If it fails for some reason, for now, we just note it
                    // and pass.
                    if let Err(e) = event_graph.dag_insert(slice::from_ref(&event)).await {
                        error!(target: "taud", "Failed inserting new event to DAG: {e}");
                    } else {
                        // Otherwise, broadcast it
                        p2p.broadcast(&EventPut(event)).await;
                    }
                }
            }
            // Process message from the network. These should only be EncryptedTask.
            task_event = incoming.receive().fuse() => {
                let event_id = task_event.id();
                if is_seen(sled_db.clone(), seen.clone(), &event_id).await? {
                    continue
                }
                mark_seen(sled_db.clone(), seen.clone(), &event_id).await?;

                // Try to deserialize the `Event`'s content into a `EncryptedTask`
                let enc_task: EncryptedTask = match deserialize_async_partial(task_event.content()).await {
                    Ok((v, _)) => v,
                    Err(e) => {
                        error!(target: "taud", "[TAUD] Failed deserializing incoming EncryptedTask event: {e}");
                        continue
                    }
                };
                on_receive_task(&enc_task, &workspaces, &settings)
                    .await?;
            }
        }
    }
}

/// Handle a received task, decrypt it, verify it, optionally write it
/// to a named pipe and save it on disk.
async fn on_receive_task(
    enc_task: &EncryptedTask,
    workspaces: &HashMap<String, Workspace>,
    settings: &Args,
) -> TaudResult<()> {
    for (ws_name, workspace) in workspaces.iter() {
        let signed_task = try_decrypt_task(enc_task, &workspace.read_key);
        if let Err(e) = signed_task {
            debug!(target: "taud", "Unable to decrypt the task: {e}");
            continue
        }

        if !workspace
            .write_pubkey
            .verify(&signed_task.as_ref().unwrap().task, &signed_task.as_ref().unwrap().signature)
        {
            error!(target: "taud", "Task is not verified: wrong write_public_key");
            error!(target: "taud", "Task is not saved");
            continue
        }

        let mut task: TaskInfo = deserialize(&signed_task.unwrap().task)?;
        info!(target: "taud", "Save the task: ref: {}", task.ref_id);
        task.workspace.clone_from(ws_name);
        let datastore_path = expand_path(&settings.datastore)?;

        // Push a notification to a fifo if set
        if settings.piped {
            // if we can't load the task then it's a new task.
            // otherwise it's a modification.
            match TaskInfo::load(&task.ref_id, &datastore_path) {
                Ok(loaded_task) => {
                    let loaded_events = loaded_task.events;
                    let mut events = task.events.clone();
                    events.retain(|ev| !loaded_events.contains(ev));

                    let file = settings.pipe_path.clone();
                    let mut pipe_write = pipe_write(file)?;
                    let mut task_clone = task.clone();
                    task_clone.events = events;

                    let json: JsonValue = (&task_clone).into();
                    pipe_write.write_all(json.stringify().unwrap().as_bytes())?;
                }
                Err(_) => {
                    let file = settings.pipe_path.clone();
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

        task.save(&datastore_path)?;
    }
    Ok(())
}

async_daemonize!(realmain);
async fn realmain(settings: Args, executor: Arc<smol::Executor<'static>>) -> Result<()> {
    let datastore_path = expand_path(&settings.datastore)?;

    let nickname =
        if settings.nickname.is_some() { settings.nickname.clone() } else { env::var("USER").ok() };

    if settings.refresh {
        println!("Removing local data in: {datastore_path:?} (yes/no)? ");
        let mut confirm = String::new();
        stdin().read_line(&mut confirm).expect("Failed to read line");

        let confirm = confirm.to_lowercase();
        let confirm = confirm.trim();

        if confirm == "yes" || confirm == "y" {
            remove_dir_all(datastore_path).unwrap_or(());
            println!("Local data removed successfully.");
        } else {
            error!(target: "taud", "Unexpected Value: {confirm}");
        }

        return Ok(())
    }

    if nickname.is_none() {
        error!(target: "taud", "Provide a nickname in config file");
        return Ok(())
    }

    if settings.piped {
        let file = settings.pipe_path.clone();
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
                error!(target: "taud", "Wrong workspace try again");
                continue
            }

            // Encryption
            // Chachabox secret key (read_key) used for encrypting tasks.
            let secret_key = SecretKey::generate(&mut OsRng);
            let encoded = bs58::encode(secret_key.to_bytes());

            // Signature
            // Secret key (write_key) used for signing tasks.
            let keypair = Keypair::random(&mut OsRng);
            let sk = format!("{}", keypair.secret);
            // Public key (write_public_key) used for verifying tasks.
            let pk = format!("{}", keypair.public);

            println!("Please add the following to the config file:");
            println!("[workspace.\"{workspace}\"]");
            println!("read_key = \"{}\"", encoded.into_string());
            println!("write_key = \"{sk}\"");
            println!("write_public_key = \"{pk}\"");
            break
        }

        return Ok(())
    }

    let workspaces = Arc::new(get_workspaces(&settings).await?);
    // let verified = Arc::new(Mutex::new(false));

    if workspaces.is_empty() {
        error!(target: "taud", "Please add at least one workspace to the config file.");
        println!("Run `$ taud --generate` to generate new workspace.");
        return Ok(())
    }

    info!(target: "taud", "Initializing taud node");

    // Create datastore path if not there already.
    let datastore = expand_path(&settings.datastore)?;
    fs::create_dir_all(&datastore).await?;

    let replay_datastore = expand_path(&settings.replay_datastore)?;
    let replay_mode = settings.replay_mode;

    info!(target: "taud", "Instantiating event DAG");
    let sled_db = sled::open(datastore)?;

    let p2p_settings: darkfi::net::Settings =
        (env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), settings.net.clone()).try_into()?;
    let comms_timeout = p2p_settings.outbound_connect_timeout_max();

    let p2p = P2p::new(p2p_settings, executor.clone()).await?;
    let event_graph = EventGraph::new(
        p2p.clone(),
        sled_db.clone(),
        replay_datastore,
        replay_mode,
        "taud_dag",
        0,
        executor.clone(),
    )
    .await?;

    info!(target: "taud", "Registering EventGraph P2P protocol");
    let event_graph_ = Arc::clone(&event_graph);
    let registry = p2p.protocol_registry();
    registry
        .register(SESSION_DEFAULT, move |channel, _| {
            let event_graph_ = event_graph_.clone();
            async move { ProtocolEventGraph::init(event_graph_, channel).await.unwrap() }
        })
        .await;

    let (broadcast_snd, broadcast_rcv) = smol::channel::unbounded::<TaskInfo>();

    info!(target: "taud", "Starting P2P network");
    p2p.clone().start().await?;

    loop {
        if p2p.is_connected() {
            info!(target: "taud", "Got peer connection");
            // We'll attempt to sync for ever
            if !settings.skip_dag_sync {
                info!(target: "taud", "Syncing event DAG");
                match event_graph.dag_sync().await {
                    Ok(()) => break,
                    Err(e) => {
                        // TODO: Maybe at this point we should prune or something?
                        // TODO: Or maybe just tell the user to delete the DAG from FS.
                        error!(target: "taud", "Failed syncing DAG ({e}), retrying in {comms_timeout}s...");
                        sleep(comms_timeout).await;
                    }
                }
            } else {
                *event_graph.synced.write().await = true;
                break
            }
        } else {
            info!(target: "taud", "Waiting for some P2P connections...");
            sleep(comms_timeout).await;
        }
    }

    let seen = OnceLock::new();
    seen.set(sled_db.open_tree("tau_seen").unwrap()).unwrap();

    ////////////////////
    // get history
    ////////////////////
    let dag_events = event_graph.order_events().await;

    for event in dag_events.iter() {
        let event_id = event.id();
        // If it was seen, skip
        if is_seen(sled_db.clone(), seen.clone(), &event_id).await? {
            continue
        }
        mark_seen(sled_db.clone(), seen.clone(), &event_id).await?;

        // Try to deserialize it. (Here we skip errors)
        let Ok((enc_task, _)) = deserialize_async_partial(event.content()).await else { continue };

        // Potentially decrypt the privmsg
        on_receive_task(&enc_task, &workspaces, &settings).await.unwrap();
    }

    ////////////////////
    // Listner
    ////////////////////
    info!(target: "taud", "Starting sync loop task");

    let sync_loop_task = StoppableTask::new();
    sync_loop_task.clone().start(
        start_sync_loop(
            event_graph.clone(),
            broadcast_rcv,
            workspaces.clone(),
            sled_db.clone(),
            settings.clone(),
            p2p.clone(),
            seen.clone(),
        ),
        |res| async {
            match res {
                Ok(()) | Err(TaudError::Darkfi(Error::DetachedTaskStopped)) => { /* Do nothing */ }
                Err(e) => error!(target: "taud", "Failed stopping sync loop task: {e}"),
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
                debug!(target: "taud", "Got dnet event: {event:?}");
                json_sub_.notify(vec![event.into()].into()).await;
            }
        },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => {
                    error!(target: "taud", "Failed stopping dnet subs task: {e}")
                }
            }
        },
        Error::DetachedTaskStopped,
        executor.clone(),
    );

    info!("Starting deg subs task");
    let deg_sub = JsonSubscriber::new("deg.subscribe_events");
    let deg_sub_ = deg_sub.clone();
    let event_graph_ = event_graph.clone();
    let deg_task = StoppableTask::new();
    deg_task.clone().start(
        async move {
            let deg_sub = event_graph_.deg_subscribe().await;
            loop {
                let event = deg_sub.receive().await;
                debug!(target: "taud", "Got deg event: {event:?}");
                deg_sub_.notify(vec![event.into()].into()).await;
            }
        },
        |res| async {
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                Err(e) => panic!("{e}"),
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
        event_graph.clone(),
        json_sub,
        deg_sub,
    ));
    let rpc_task = StoppableTask::new();
    rpc_task.clone().start(
        listen_and_serve(settings.rpc.into(), rpc_interface.clone(), None, executor.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => rpc_interface.stop_connections().await,
                Err(e) => error!(target: "taud", "Failed stopping JSON-RPC server: {e}"),
            }
        },
        Error::RpcServerStopped,
        executor.clone(),
    );

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(executor)?;
    signals_handler.wait_termination(signals_task).await?;
    info!(target: "taud", "Caught termination signal, cleaning up and exiting...");

    info!(target: "taud", "Stopping P2P network");
    p2p.stop().await;

    info!(target: "taud", "Stopping sync loop task...");
    sync_loop_task.stop().await;

    info!(target: "taud", "Stopping JSON-RPC server...");
    rpc_task.stop().await;
    dnet_task.stop().await;
    deg_task.stop().await;

    info!(target: "taud", "Flushing sled database...");
    let flushed_bytes = sled_db.flush_async().await?;
    info!(target: "taud", "Flushed {flushed_bytes} bytes");

    info!(target: "taud", "Shut down successfully");
    Ok(())
}
