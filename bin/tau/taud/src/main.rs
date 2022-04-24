use async_std::sync::Arc;
use std::path::PathBuf;

use async_executor::Executor;
use easy_parallel::Parallel;
use futures_lite::future;
use log::{info, warn};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use structopt_toml::StructOptToml;

use darkfi::{
    async_daemonize,
    net::Settings as P2pSettings,
    raft::Raft,
    rpc::rpcserver::{listen_and_serve, RpcServerConfig},
    util::{
        cli::{log_config, spawn_config},
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
    let p2p_settings = P2pSettings {
        inbound: settings.accept,
        outbound_connections: settings.slots,
        external_addr: settings.accept,
        peers: settings.connect.clone(),
        seeds: settings.seeds.clone(),
        ..Default::default()
    };

    let datastore_path = PathBuf::from(&settings.datastore);

    //
    //Raft
    //
    let datastore_raft = datastore_path.join("tau.db");
    let mut raft = Raft::<TaskInfo>::new(settings.accept, datastore_raft)?;

    let raft_sender = raft.get_broadcast();
    let commits = raft.get_commits();
    let initial_sync_raft_sender = raft_sender.clone();

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

    let datastore_path_cloned = datastore_path.clone();
    let recv_update_from_rpc: smol::Task<TaudResult<()>> = executor.spawn(async move {
        loop {
            let task_info = rpc_rcv.recv().await.map_err(Error::from)?;
            if let Some(tk) = task_info {
                info!(target: "tau", "save the received task {:?}", tk);
                tk.save(&datastore_path_cloned)?;
                raft_sender.send(tk).await.map_err(Error::from)?;
            }
        }
    });

    let datastore_path_cloned = datastore_path.clone();
    let recv_update_from_raft: smol::Task<TaudResult<()>> = executor.spawn(async move {
        loop {
            let task = commits.recv().await.map_err(Error::from)?;
            info!(target: "tau", "receive update from the commits {:?}", task);
            task.save(&datastore_path_cloned)?;
        }
    });

    let initial_sync: smol::Task<TaudResult<()>> = executor.spawn(async move {
        info!(target: "tau", "Start initial sync");
        info!(target: "tau", "Upload local tasks");
        let tasks = MonthTasks::load_current_open_tasks(&datastore_path)?;

        for task in tasks {
            info!(target: "tau", "send local task {:?}", task);
            initial_sync_raft_sender.send(task).await.map_err(Error::from)?;
        }
        Ok(())
    });

    let executor_cloned = executor.clone();
    let rpc_listener_taks =
        executor_cloned.spawn(listen_and_serve(server_config, rpc_interface, executor.clone()));

    let (signal, shutdown) = async_channel::bounded::<()>(1);
    ctrlc_async::set_async_handler(async move {
        warn!(target: "tau", "taud start() Exit Signal");
        // cleaning up tasks running in the background
        signal.send(()).await.unwrap();
        rpc_listener_taks.cancel().await;
        recv_update_from_rpc.cancel().await;
        recv_update_from_raft.cancel().await;
        initial_sync.cancel().await;
    })
    .unwrap();

    // blocking
    raft.start(p2p_settings.clone(), executor.clone(), shutdown.clone()).await?;

    Ok(())
}
