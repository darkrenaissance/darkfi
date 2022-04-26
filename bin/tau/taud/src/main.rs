use async_std::sync::Arc;
use std::fs::create_dir_all;

use async_executor::Executor;
use easy_parallel::Parallel;
use futures::{select, FutureExt};
use log::{info, warn};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use smol::future;
use structopt_toml::StructOptToml;

use darkfi::{
    async_daemonize,
    raft::Raft,
    rpc::rpcserver::{listen_and_serve, RpcServerConfig},
    util::{
        cli::{log_config, spawn_config},
        expand_path,
        path::get_config_path,
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
    month_tasks::MonthTasks,
    settings::{Args, CONFIG_FILE, CONFIG_FILE_CONTENTS},
    task_info::TaskInfo,
};

async_daemonize!(realmain);
async fn realmain(settings: Args, executor: Arc<Executor<'_>>) -> Result<()> {
    let datastore_path = expand_path(&settings.datastore)?;

    // mkdir datastore_path if not exists
    create_dir_all(datastore_path.join("month"))?;
    create_dir_all(datastore_path.join("task"))?;

    //
    // RPC
    //
    let server_config = RpcServerConfig {
        socket_addr: settings.rpc_listen,
        use_tls: false,
        // this is all random filler that is meaningless bc tls is disabled
        identity_path: Default::default(),
        identity_pass: Default::default(),
    };

    let (rpc_snd, rpc_rcv) = async_channel::unbounded::<Option<TaskInfo>>();

    let rpc_interface = Arc::new(JsonRpcInterface::new(rpc_snd, datastore_path.clone()));

    let executor_cloned = executor.clone();
    let rpc_listener_taks =
        executor_cloned.spawn(listen_and_serve(server_config, rpc_interface, executor.clone()));

    let net_settings = settings.net; 

    //
    //Raft
    //
    let datastore_raft = datastore_path.join("tau.db");
    let mut raft = Raft::<TaskInfo>::new(net_settings.inbound, datastore_raft)?;

    let raft_sender = raft.get_broadcast();
    let commits = raft.get_commits();
    let initial_sync_raft_sender = raft_sender.clone();

    let datastore_path_cloned = datastore_path.clone();
    let recv_update: smol::Task<TaudResult<()>> = executor.spawn(async move {
        info!(target: "tau", "Start initial sync");
        info!(target: "tau", "Upload local tasks");
        let tasks = MonthTasks::load_current_open_tasks(&datastore_path)?;

        for task in tasks {
            info!(target: "tau", "send local task {:?}", task);
            initial_sync_raft_sender.send(task).await.map_err(Error::from)?;
        }

        loop {
            select! {
                task = rpc_rcv.recv().fuse() => {
                    let task = task.map_err(Error::from)?;
                    if let Some(tk) = task {
                        info!(target: "tau", "save the received task {:?}", tk);
                        tk.save(&datastore_path_cloned)?;
                        raft_sender.send(tk).await.map_err(Error::from)?;
                    }
                }
                task = commits.recv().fuse() => {
                    let task = task.map_err(Error::from)?;
                    info!(target: "tau", "receive update from the commits {:?}", task);
                    task.save(&datastore_path_cloned)?;
                }

            }
        }
    });

    let (signal, shutdown) = async_channel::bounded::<()>(1);
    ctrlc_async::set_async_handler(async move {
        warn!(target: "tau", "taud start() Exit Signal");
        // cleaning up tasks running in the background
        signal.send(()).await.unwrap();
        rpc_listener_taks.cancel().await;
        recv_update.cancel().await;
    })
    .unwrap();

    // blocking
    raft.start(net_settings.into(), executor.clone(), shutdown.clone()).await?;

    Ok(())
}
