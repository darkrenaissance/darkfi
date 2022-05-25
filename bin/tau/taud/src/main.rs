use async_std::sync::{Arc, Mutex};
use std::{env, fs::create_dir_all, path::PathBuf};

use async_executor::Executor;
use async_trait::async_trait;
use crypto::EncryptedTask;
use crypto_box::{SecretKey, KEY_SIZE};
use easy_parallel::Parallel;
use futures::{select, FutureExt};
use log::{error, info, warn};
use serde_json::Value;
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use smol::future;
use structopt_toml::StructOptToml;
use url::Url;

use darkfi::{
    async_daemonize, net,
    raft::{NetMsg, ProtocolRaft, Raft},
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResult},
        server::{listen_and_serve, RequestHandler},
    },
    util::{
        cli::{log_config, spawn_config},
        expand_path,
        path::get_config_path,
    },
    Error, Result,
};

mod crypto;
mod error;
mod month_tasks;
mod rpc_add;
mod rpc_get;
mod rpc_update;
mod settings;
mod task_info;
mod util;

use crate::{
    crypto::{decrypt_task, encrypt_task},
    error::{to_json_result, TaudError, TaudResult},
    month_tasks::MonthTasks,
    settings::{Args, CONFIG_FILE, CONFIG_FILE_CONTENTS},
    task_info::TaskInfo,
    util::{load, save},
};

pub struct JsonRpcInterface {
    dataset_path: PathBuf,
    notify_queue_sender: async_channel::Sender<Option<TaskInfo>>,
    nickname: String,
}

#[async_trait]
impl RequestHandler for JsonRpcInterface {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if !req.params.is_array() {
            return JsonError::new(ErrorCode::InvalidParams, None, req.id).into()
        }

        if self.notify_queue_sender.send(None).await.is_err() {
            return JsonError::new(ErrorCode::InternalError, None, req.id).into()
        }

        let rep = match req.method.as_str() {
            Some("add") => self.add(req.params).await,
            Some("update") => self.update(req.params).await,
            Some("get_ids") => self.get_ids(req.params).await,
            Some("set_state") => self.set_state(req.params).await,
            Some("set_comment") => self.set_comment(req.params).await,
            Some("get_task_by_id") => self.get_task_by_id(req.params).await,
            Some(_) | None => return JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        };

        to_json_result(rep, req.id)
    }
}

impl JsonRpcInterface {
    pub fn new(
        notify_queue_sender: async_channel::Sender<Option<TaskInfo>>,
        dataset_path: PathBuf,
        nickname: String,
    ) -> Self {
        Self { notify_queue_sender, dataset_path, nickname }
    }

    pub fn load_task_by_id(&self, task_id: &Value) -> TaudResult<TaskInfo> {
        let task_id: u64 = serde_json::from_value(task_id.clone())?;

        let tasks = MonthTasks::load_current_open_tasks(&self.dataset_path)?;
        let task = tasks.into_iter().find(|t| (t.get_id() as u64) == task_id);

        task.ok_or(TaudError::InvalidId)
    }
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
    let rpc_listener_url = Url::parse(&settings.rpc_listen)?;
    let rpc_listener_task =
        executor_cloned.spawn(listen_and_serve(rpc_listener_url, rpc_interface));

    let net_settings = settings.net;

    //
    // Raft
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
                        let encrypted_task = encrypt_task(&tk, &secret_key)?;
                        tk.save(&datastore_path_cloned)?;
                        raft_sender.send(encrypted_task).await.map_err(Error::from)?;
                    }
                }
                task = commits.recv().fuse() => {
                    let recv = task.map_err(Error::from)?;
                    let task = decrypt_task(&recv, &secret_key);

                    if task.is_none() {
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
