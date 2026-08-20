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
    collections::{BTreeMap, HashMap},
    env,
    ffi::CString,
    fs::{create_dir_all, remove_dir_all, File},
    io::{stdin, Write},
    str::FromStr,
    sync::{atomic::Ordering, Arc, OnceLock},
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
use tracing::{debug, error, info, warn};

use darkfi::{
    async_daemonize,
    event_graph::{
        proto::{EventPut, ProtocolEventGraph},
        Event, EventGraph, EventGraphConfig, EventGraphPtr,
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
    pasta_prelude::PrimeField,
    schnorr::{SchnorrPublic, SchnorrSecret, Signature},
    Keypair, PublicKey,
};

// =====================================================================
// Taud consensus parameters.
//
// These define the EventGraph configuration that EVERY Taud node
// in the network must agree on. Changing any of them is a hard fork.
// They are passed verbatim to `EventGraph::new` at startup.
// =====================================================================

/// Epoch origin for DAG rotation (UTC midnight, 1 March 2025).
/// Rotation boundaries are computed as offsets from this point.
const TAUD_INITIAL_GENESIS: u64 = 1_740_787_200_000;

/// DAG rotation period, in hours.
const TAUD_HOURS_ROTATION: u64 = 0;

/// Genesis payload. Two protocols MUST use distinct values; this
/// also feeds into `RlnAppId::from_genesis` so RLN signals from one
/// deployment never appear valid on another.
const TAUD_GENESIS_CONTENTS: &[u8] = b"taud-v1";

/// How many rotation periods to keep in the rolling DAG window.
/// With `hours_rotation = 1` and `max_dags = 24`, this gives a
/// 24-hour history window. Older events are evicted from sled.
const TAUD_MAX_DAGS: usize = 1;

/// Sled cache capacity multiplier.
const BYTES_PER_MIB: u64 = 1024 * 1024;

/// Per-epoch limit printed by `--gen-rln-identity`.
fn generated_rln_identity_user_msg_limit() -> u64 {
    darkfi::event_graph::rln::GENESIS_USER_MSG_LIMIT
}

fn sled_cache_capacity_bytes(name: &str, cache_mb: u64) -> Result<u64> {
    if cache_mb == 0 {
        return Err(Error::Custom(format!("{name} must be greater than 0")))
    }

    cache_mb
        .checked_mul(BYTES_PER_MIB)
        .ok_or_else(|| Error::Custom(format!("{name} overflows bytes")))
}

mod jsonrpc;
mod settings;

use taud::{
    error::{TaudError, TaudResult},
    rln::{
        load_default_rln_identity, reserve_rln_message_id_in_store, RlnIdentity,
        RlnMessageReservation,
    },
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

fn parse_configured_workspaces(data: &toml::Value) -> Result<BTreeMap<String, Workspace>> {
    let mut ret = BTreeMap::new();

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

async fn get_workspaces(settings: &Args) -> Result<BTreeMap<String, Workspace>> {
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
    workspaces: Arc<BTreeMap<String, Workspace>>,
    sled_db: sled::Db,
    settings: Args,
    p2p: P2pPtr,
    seen: OnceLock<sled::Tree>,
    rln_identity: Arc<smol::lock::RwLock<Option<RlnIdentity>>>,
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
                    let event = match Event::new(serialize_async(&encrypted_task).await, &event_graph).await {
                        Ok(event) => event,
                        Err(e) => {
                            error!(target: "taud", "Failed creating new DAG event: {e}");
                            continue
                        }
                    };

                    let current_genesis = event_graph.current_genesis.read().await;
                    let dag_name = current_genesis.header.timestamp.to_string();
                    drop(current_genesis);

                    // Build the RLN signal blob before touching the local DAG when RLN
                    // is enabled. With RLN disabled, outbound events deliberately carry
                    // no proof blob.
                    let blob = if event_graph.rln_enabled() {
                        let (rln_identity, mid) = {
                            let mut active = rln_identity.write().await;
                            match reserve_rln_message_id_in_store(
                                &sled_db,
                                &mut active,
                                event.header.timestamp,
                            )
                            .await?
                            {
                                RlnMessageReservation::Reserved { identity, message_id } => {
                                    (identity, message_id)
                                }
                                RlnMessageReservation::MissingIdentity => {
                                    warn!(target: "taud", "No RLN identity registered; refusing to send. Run `tau rln register ...` to register.");
                                    continue
                                }
                                RlnMessageReservation::BudgetExhausted => {
                                    warn!(target: "taud", "RLN message budget exhausted for this epoch; dropping message to avoid slash");
                                    continue
                                }
                            }
                        };
                        match rln_identity.create_signal(&event, mid, &event_graph).await {
                            Ok(blob) => serialize_async(&blob).await,
                            Err(e) => {
                                error!(target: "taud", "Failed creating RLN signal proof: {e}");
                                continue
                            }
                        }
                    } else {
                        Vec::new()
                    };

                    if let Err(e) = event_graph.insert_signal_with_blob(&event, &blob, &dag_name).await {
                        error!(target: "taud", "Failed inserting new event to DAG: {e}");
                    } else {
                        // Otherwise, broadcast it. Taud runs EventGraph with RLN disabled
                        // by default, so the blob is empty unless RLN was enabled.
                        if let Err(e) = p2p.broadcast(&EventPut(event, blob)).await {
                            error!(target: "taud", "Event broadcast was not admitted: {e}");
                        }
                    }
                }
            }
            // Process message from the network. These should only be EncryptedTask.
            task_event = incoming.receive().fuse() => {
                let event_id = task_event.header.id();
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
    workspaces: &BTreeMap<String, Workspace>,
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

        break
    }
    Ok(())
}

async_daemonize!(realmain);
async fn realmain(settings: Args, executor: Arc<smol::Executor<'static>>) -> Result<()> {
    let datastore_path = expand_path(&settings.datastore)?;

    let nickname =
        if settings.nickname.is_some() { settings.nickname.clone() } else { env::var("USER").ok() };

    if settings.gen_rln_identity {
        let identity = RlnIdentity::new(&mut OsRng);
        let nullifier = bs58::encode(identity.nullifier.to_repr()).into_string();
        let trapdoor = bs58::encode(identity.trapdoor.to_repr()).into_string();
        // This value is part of the RLN commitment. It must match
        // the genesis budget used for pregenerated identities.
        let user_msg_limit = generated_rln_identity_user_msg_limit();

        println!("Generated a fresh RLN identity.\n");
        println!(
            "Current Taud registration accepts only identities whose commitments are in \
             the configured pregenerated set. Use this output for a genesis bundle or future \
             staked-registration testing; it will not register unless its commitment is \
             pregenerated.\n"
        );
        println!("Local account import command:\n");
        println!("  tau rln register <account_name> {nullifier} {trapdoor} {user_msg_limit}\n");
        println!(
            "Replace <account_name> with any local label you like (\"alice\", \"throwaway\", etc)."
        );
        println!(
            "Do not change user_msg_limit: it is part of the RLN commitment and must be \
             GENESIS_USER_MSG_LIMIT ({user_msg_limit}) for pregenerated genesis identities."
        );
        println!(
            "Keep the nullifier and trapdoor secret - they ARE the identity. \
             A `taud --gen-rln-identity` run is NOT idempotent; treat the \
             output like a freshly-minted password."
        );
        return Ok(())
    }

    if let Some(n_identities) = settings.gen_genesis_rln_identities {
        // We'll generate n_identities and hold them in a map
        // `k=commitment, v=(nullifier, trapdoor, used)`
        // We'll export the commitments to be used in the genesis event,
        // and the rest as a JSON file.
        let mut identities_map = HashMap::new();
        for _ in 0..n_identities {
            let identity = RlnIdentity::new(&mut OsRng);
            let commitment = identity.commitment();
            identities_map.insert(
                commitment.to_repr(),
                (identity.nullifier.to_repr(), identity.trapdoor.to_repr(), false),
            );
        }

        let mut commits = String::from(
            r#"
use darkfi_sdk::{crypto::pasta_prelude::PrimeField, pasta::pallas};

/// Return Taud's configured pregenerated RLN commitment set.
pub fn pregenerated_identity_commitments() -> Vec<[u8; 32]> {
    TAUD_GENESIS_COMMITMENTS_REPR.to_vec()
}

/// Check whether an RLN commitment belongs to Taud's pregenerated set.
pub fn is_pregenerated_commitment(commitment: &pallas::Base) -> bool {
    TAUD_GENESIS_COMMITMENTS_REPR.contains(&commitment.to_repr())
}

pub const TAUD_GENESIS_COMMITMENTS_REPR: &[[u8; 32]] = &[
"#,
        );

        for commitment in identities_map.keys() {
            commits.push_str(&format!("{:?},\n", commitment));
        }

        commits.push_str("];\n");

        let mut file = File::create("genesis_commits.rs")?;
        file.write_all(commits.as_bytes())?;

        let mut file = File::create("taud_rln_commits.bin")?;
        let buf = serialize(&identities_map);
        file.write_all(&buf)?;

        return Ok(())
    }

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
    let (workspace, _) = workspaces.first_key_value().unwrap();
    // let verified = Arc::new(Mutex::new(false));

    if workspaces.is_empty() {
        error!(target: "taud", "Please add at least one workspace to the config file.");
        println!("Run `$ taud --generate` to generate new workspace.");
        return Ok(())
    }

    info!(target: "taud", "Initializing taud node");

    let rln_enabled = settings.rln_enabled.unwrap_or(false);

    // Create datastore path if not there already.
    let datastore = expand_path(&settings.datastore)?;
    fs::create_dir_all(&datastore).await?;

    let zk_key_datastore = if rln_enabled {
        let zk_key_datastore = expand_path(&settings.zk_key_datastore)?;
        fs::create_dir_all(&zk_key_datastore).await?;
        Some(zk_key_datastore)
    } else {
        info!(target: "taud", "RLN disabled; skipping RLN key datastore setup");
        None
    };

    let replay_datastore = expand_path(&settings.replay_datastore)?;
    let replay_mode = settings.replay_mode;
    // let fast_mode = settings.fast_mode;

    info!(target: "taud", "Instantiating event DAG");
    let sled_db = sled::open(datastore)?;

    let zk_key_db = if let Some(zk_key_datastore) = zk_key_datastore.as_ref() {
        let zk_key_sled_cache_capacity =
            sled_cache_capacity_bytes("zk_key_sled_cache_mb", settings.zk_key_sled_cache_mb)?;
        info!(target: "taud", "Opening RLN key datastore with {} MiB sled cache", settings.zk_key_sled_cache_mb);
        Some(
            match sled::Config::new()
                .path(zk_key_datastore.clone())
                .cache_capacity(zk_key_sled_cache_capacity)
                .open()
            {
                Ok(v) => v,
                Err(e) => {
                    error!(target: "taud", "Failed to open RLN key datastore `{zk_key_datastore:?}`: {e}");
                    return Err(e.into());
                }
            },
        )
    } else {
        None
    };

    let p2p_settings: darkfi::net::Settings =
        (env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), settings.net.clone()).try_into()?;
    let comms_timeout = p2p_settings.outbound_connect_timeout_max();
    let p2p = match P2p::new(p2p_settings, executor.clone()).await {
        Ok(p2p) => p2p,
        Err(e) => {
            error!("Unable to create P2P network: {e}");
            return Err(e);
        }
    };
    // Consensus config. Every node must use exactly these values.
    let eg_config = EventGraphConfig {
        initial_genesis: TAUD_INITIAL_GENESIS,
        hours_rotation: TAUD_HOURS_ROTATION,
        genesis_contents: TAUD_GENESIS_CONTENTS.to_vec(),
        rln_enabled,
        pregenerated_identity_commitments: if rln_enabled {
            taud::genesis_commits::pregenerated_identity_commitments()
        } else {
            Vec::new()
        },
        max_dags: Some(TAUD_MAX_DAGS),
    };
    let event_graph = match if let Some(zk_key_db) = zk_key_db.clone() {
        EventGraph::new_with_zk_key_db(
            p2p.clone(),
            sled_db.clone(),
            zk_key_db,
            replay_datastore.clone(),
            replay_mode,
            eg_config,
            executor.clone(),
        )
        .await
    } else {
        EventGraph::new(
            p2p.clone(),
            sled_db.clone(),
            replay_datastore.clone(),
            replay_mode,
            eg_config,
            executor.clone(),
        )
        .await
    } {
        Ok(v) => v,
        Err(e) => {
            error!("Event graph failed to start: {e}");
            return Err(e);
        }
    };

    // Set the active RLN account if any. When RLN is disabled, avoid
    // loading account state that cannot affect outbound messages.
    let rln_identity: Arc<smol::lock::RwLock<Option<RlnIdentity>>> =
        Arc::new(smol::lock::RwLock::new(if event_graph.rln_enabled() {
            let rln_identity = load_default_rln_identity(&sled_db).await?;
            if rln_identity.is_some() {
                info!(target: "taud", "Default RLN account set");
            }
            rln_identity
        } else {
            info!(target: "taud", "RLN disabled; skipping default RLN account load");
            None
        }));

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
                info!(target: "taud", "Syncing static DAG");
                match event_graph.static_sync().await {
                    Ok(()) => info!(target: "taud", "Static synced successfully"),
                    Err(e) => {
                        error!(target: "taud", "Failed syncing static graph: {e}");
                        sleep(comms_timeout).await;
                        continue
                    }
                }
                info!(target: "taud", "Syncing event DAG");
                match event_graph.sync_selected(1).await {
                    Ok(()) => {
                        info!(target: "taud", "Event DAG synced successfully!");
                        break
                    }
                    Err(e) => {
                        // TODO: Maybe at this point we should prune or something?
                        // TODO: Or maybe just tell the user to delete the DAG from FS.
                        error!(target: "taud", "Failed syncing DAG ({e}), retrying in {comms_timeout}s...");
                        sleep(comms_timeout).await;
                    }
                }
            } else {
                event_graph.synced.store(true, Ordering::Release);
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
    let dag_events = event_graph.order_events().await?;

    for event in dag_events.iter() {
        let event_id = event.header.id();
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
            rln_identity.clone(),
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
        workspace.to_string(),
        workspaces.clone(),
        p2p.clone(),
        event_graph.clone(),
        json_sub,
        deg_sub,
        sled_db.clone(),
        rln_identity.clone(),
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

    if let Some(zk_key_db) = zk_key_db {
        info!(target: "taud", "Flushing RLN key sled database...");
        let flushed_key_bytes = zk_key_db.flush_async().await?;
        info!(target: "taud", "Flushed {flushed_key_bytes} RLN key bytes");
    }

    info!(target: "taud", "Shut down successfully");
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use structopt::StructOpt;

    use darkfi::util::time::Timestamp;
    use taud::task_info::TaskInfo;

    use super::*;

    const TEST_DATA_PATH: &str = "/tmp/test_tau_ws_claim";

    /// Two workspaces that share the same read/write key, as is common in
    /// localnet testing where one workspace block is duplicated.
    fn shared_key_workspaces() -> BTreeMap<String, Workspace> {
        let read_key = SecretKey::generate(&mut OsRng);
        let chacha = ChaChaBox::new(&read_key.public_key(), &read_key);

        let write_key = darkfi_sdk::crypto::SecretKey::random(&mut OsRng);
        let write_pubkey = PublicKey::from_secret(write_key);

        let mut map = BTreeMap::new();
        map.insert(
            "darkfi-dev".to_string(),
            Workspace {
                read_key: ChaChaBox::new(&read_key.public_key(), &read_key),
                write_key: Some(write_key),
                write_pubkey,
            },
        );
        map.insert(
            "test".to_string(),
            Workspace { read_key: chacha, write_key: Some(write_key), write_pubkey },
        );
        map
    }

    #[test]
    fn shared_keys_task_is_claimed_by_first_workspace() -> TaudResult<()> {
        remove_dir_all(TEST_DATA_PATH).ok();
        create_dir_all(TEST_DATA_PATH).unwrap();
        create_dir_all(Path::new(TEST_DATA_PATH).join("task")).unwrap();
        create_dir_all(Path::new(TEST_DATA_PATH).join("month")).unwrap();

        let workspaces = shared_key_workspaces();

        let mut args = Args::from_iter_safe(vec!["taud".to_string()]).unwrap();
        args.datastore = TEST_DATA_PATH.to_string();

        let task = TaskInfo::new(
            "darkfi-dev".to_string(),
            "test_title",
            "test_desc",
            "NICK",
            None,
            None,
            Timestamp::current_time(),
            None,
        )?;

        let enc = encrypt_sign_task(&task, workspaces.get("darkfi-dev").unwrap())?;

        smol::block_on(async { on_receive_task(&enc, &workspaces, &args).await })?;

        // Even though both workspaces can decrypt the task, it must be
        // claimed exactly once and keep the originating workspace label,
        // not be overwritten by the later `test` workspace.
        let loaded = TaskInfo::load(&task.ref_id, Path::new(TEST_DATA_PATH))?;
        assert_eq!(loaded.workspace, "darkfi-dev");

        Ok(())
    }
}
