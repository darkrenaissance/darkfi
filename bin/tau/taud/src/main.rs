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
    async_daemonize, net,
    raft::{NetMsg, ProtocolRaft, Raft, RaftSettings},
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

async fn start_sync_loop(
    broadcast_rcv: smol::channel::Receiver<TaskInfo>,
    raft_msgs_sender: smol::channel::Sender<EncryptedTask>,
    commits_recv: smol::channel::Receiver<EncryptedTask>,
    datastore_path: std::path::PathBuf,
    workspaces: HashMap<String, SalsaBox>,
    mut rng: crypto_box::rand_core::OsRng,
) -> TaudResult<()> {
    loop {
        select! {
            task = broadcast_rcv.recv().fuse() => {
                let tk = task.map_err(Error::from)?;
                if workspaces.contains_key(&tk.workspace) {
                    let salsa_box = workspaces.get(&tk.workspace).unwrap();
                    let encrypted_task = encrypt_task(&tk, salsa_box, &mut rng)?;
                    info!(target: "tau", "Send the task: ref: {}", tk.ref_id);
                    raft_msgs_sender.send(encrypted_task).await.map_err(Error::from)?;
                }
            }
            task = commits_recv.recv().fuse() => {
                let task = task.map_err(Error::from)?;
                on_receive_task(&task,&datastore_path, &workspaces)
                    .await?;
            }
        }
    }
}

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

    let rng = crypto_box::rand_core::OsRng;

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

    //
    // Raft
    //
    let seen_net_msgs = Arc::new(Mutex::new(HashMap::new()));

    let datastore_raft = datastore_path.join("tau.db");
    let raft_settings = RaftSettings { datastore_path: datastore_raft, ..RaftSettings::default() };

    let mut raft = Raft::<EncryptedTask>::new(raft_settings, seen_net_msgs.clone())?;
    let raft_id = raft.id();

    let (broadcast_snd, broadcast_rcv) = smol::channel::unbounded::<TaskInfo>();

    //
    // P2p setup
    //
    let mut net_settings = settings.net.clone();
    net_settings.app_version = Some(option_env!("CARGO_PKG_VERSION").unwrap_or("").to_string());
    let (p2p_send_channel, p2p_recv_channel) = smol::channel::unbounded::<NetMsg>();

    let p2p = net::P2p::new(net_settings.into()).await;
    let p2p = p2p.clone();
    let registry = p2p.protocol_registry();

    registry
        .register(net::SESSION_ALL, move |channel, p2p| {
            let raft_id = raft_id.clone();
            let sender = p2p_send_channel.clone();
            let seen_net_msgs_cloned = seen_net_msgs.clone();
            async move {
                ProtocolRaft::init(raft_id, channel, sender, p2p, seen_net_msgs_cloned).await
            }
        })
    .await;

    p2p.clone().start(executor.clone()).await?;

    executor.spawn(p2p.clone().run(executor.clone())).detach();

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

    executor
        .spawn(start_sync_loop(
            broadcast_rcv,
            raft.sender(),
            raft.receiver(),
            datastore_path,
            workspaces,
            rng,
        ))
        .detach();

    raft.run(p2p.clone(), p2p_recv_channel.clone(), executor.clone(), shutdown.clone()).await?;

    Ok(())
}
