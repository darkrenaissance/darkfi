use async_std::sync::{Arc, Mutex};
use std::{env, fs::create_dir_all};

use async_executor::Executor;
use crypto_box::{aead::Aead, Box, SecretKey, KEY_SIZE};
use futures::{select, FutureExt};
use log::{debug, error, info, warn};
use smol::future;
use structopt_toml::StructOptToml;

use darkfi::{
    async_daemonize, net,
    raft::{NetMsg, ProtocolRaft, Raft},
    rpc::server::listen_and_serve,
    util::{
        cli::{get_log_config, get_log_level, spawn_config},
        expand_path,
        path::get_config_path,
        serial::{deserialize, serialize, SerialDecodable, SerialEncodable},
    },
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
    util::{load, save},
};

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct EncryptedTask {
    nonce: Vec<u8>,
    payload: Vec<u8>,
}

fn encrypt_task(
    task: &TaskInfo,
    secret_key: &SecretKey,
    rng: &mut crypto_box::rand_core::OsRng,
) -> TaudResult<EncryptedTask> {
    debug!("start encrypting task");
    let public_key = secret_key.public_key();
    let msg_box = Box::new(&public_key, secret_key);

    let nonce = crypto_box::generate_nonce(rng);
    let payload = &serialize(task)[..];
    let payload = msg_box.encrypt(&nonce, payload)?;

    let nonce = nonce.to_vec();
    Ok(EncryptedTask { nonce, payload })
}

fn decrypt_task(encrypt_task: &EncryptedTask, secret_key: &SecretKey) -> TaudResult<TaskInfo> {
    debug!("start decrypting task");
    let public_key = secret_key.public_key();
    let msg_box = Box::new(&public_key, secret_key);

    let nonce = encrypt_task.nonce.as_slice();
    let decrypted_task = msg_box.decrypt(nonce.into(), &encrypt_task.payload[..])?;

    let task = deserialize(&decrypted_task)?;

    Ok(task)
}

async_daemonize!(realmain);
async fn realmain(settings: Args, executor: Arc<Executor<'_>>) -> Result<()> {
    let datastore_path = expand_path(&settings.datastore)?;

    let nickname =
        if settings.nickname.is_some() { settings.nickname } else { env::var("USER").ok() };

    if nickname.is_none() {
        error!("Provide a nickname in config file");
        return Ok(())
    }

    // mkdir datastore_path if not exists
    create_dir_all(datastore_path.join("month"))?;
    create_dir_all(datastore_path.join("task"))?;

    let mut rng = crypto_box::rand_core::OsRng;

    let secret_key = if settings.key_gen {
        info!(target: "tau", "generating a new secret key");
        let secret = SecretKey::generate(&mut rng);
        let sk_string = hex::encode(secret.as_bytes());
        save::<String>(&datastore_path.join("secret_key"), &sk_string)?;
        secret
    } else {
        let loaded_key = load::<String>(&datastore_path.join("secret_key"));

        if loaded_key.is_err() {
            error!(
                "Could not load secret key from file, \
                 please run \"taud --help\" for more information"
            );
            return Ok(())
        }

        let sk_bytes = hex::decode(loaded_key.unwrap())?;
        let sk_bytes: [u8; KEY_SIZE] = sk_bytes.as_slice().try_into()?;
        SecretKey::try_from(sk_bytes)?
    };

    //
    // RPC
    //

    let (rpc_snd, rpc_rcv) = async_channel::unbounded::<Option<TaskInfo>>();

    let rpc_interface =
        Arc::new(JsonRpcInterface::new(rpc_snd, datastore_path.clone(), nickname.unwrap()));

    let executor_cloned = executor.clone();
    let rpc_listener_task =
        executor_cloned.spawn(listen_and_serve(settings.rpc_listen.clone(), rpc_interface));

    let net_settings = settings.net;

    //
    //Raft
    //
    let datastore_raft = datastore_path.join("tau.db");
    let mut raft = Raft::<EncryptedTask>::new(net_settings.inbound.clone(), datastore_raft)?;

    let raft_sender = raft.get_broadcast();
    let commits = raft.get_commits();

    let datastore_path_cloned = datastore_path.clone();
    let recv_update: smol::Task<TaudResult<()>> = executor.spawn(async move {
        info!(target: "tau", "Start initial sync");
        loop {
            select! {
                task = rpc_rcv.recv().fuse() => {
                    let task = task.map_err(Error::from)?;
                    if let Some(tk) = task {
                        info!(target: "tau", "save the received task {:?}", tk);
                        let encrypted_task = encrypt_task(&tk, &secret_key,&mut rng)?;
                        tk.save(&datastore_path_cloned)?;
                        raft_sender.send(encrypted_task).await.map_err(Error::from)?;
                    }
                }
                task = commits.recv().fuse() => {
                    let recv = task.map_err(Error::from)?;
                    let task = decrypt_task(&recv, &secret_key);

                    if let Err(e) = task {
                        warn!("unable to decrypt the task: {}", e);
                        continue
                    }

                    let task = task.unwrap();
                    info!(target: "tau", "receive update from the commits {:?}", task);
                    task.save(&datastore_path_cloned)?;
                }
            }
        }
    });

    // P2p setup
    let (p2p_send_channel, p2p_recv_channel) = async_channel::unbounded::<NetMsg>();

    let p2p = net::P2p::new(net_settings.into()).await;
    let p2p = p2p.clone();

    let registry = p2p.protocol_registry();

    let seen_net_msg = Arc::new(Mutex::new(vec![]));
    let raft_node_id = raft.id.clone();
    registry
        .register(net::SESSION_ALL, move |channel, p2p| {
            let raft_node_id = raft_node_id.clone();
            let sender = p2p_send_channel.clone();
            let seen_net_msg_cloned = seen_net_msg.clone();
            async move {
                ProtocolRaft::init(raft_node_id, channel, sender, p2p, seen_net_msg_cloned).await
            }
        })
        .await;

    p2p.clone().start(executor.clone()).await?;

    let executor_cloned = executor.clone();
    let p2p_run_task = executor_cloned.spawn(p2p.clone().run(executor.clone()));

    let (signal, shutdown) = async_channel::bounded::<()>(1);
    ctrlc_async::set_async_handler(async move {
        warn!(target: "tau", "taud start() Exit Signal");
        // cleaning up tasks running in the background
        signal.send(()).await.unwrap();
        rpc_listener_task.cancel().await;
        recv_update.cancel().await;
        p2p_run_task.cancel().await;
    })
    .unwrap();

    // blocking
    raft.start(p2p.clone(), p2p_recv_channel.clone(), executor.clone(), shutdown.clone()).await?;

    Ok(())
}
