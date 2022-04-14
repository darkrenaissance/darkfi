use async_std::sync::Arc;

use async_executor::Executor;
use clap::Parser;
use log::info;
use simplelog::{ColorChoice, TermLogger, TerminalMode};

use darkfi::{
    net::Settings as P2pSettings,
    raft::Raft,
    rpc::rpcserver::{listen_and_serve, RpcServerConfig},
    util::{
        cli::{log_config, spawn_config, Config},
        path::get_config_path,
    },
    Error,
};

mod error;
mod jsonrpc;
mod month_tasks;
mod task_info;
mod util;

use crate::{
    error::TaudResult,
    jsonrpc::JsonRpcInterface,
    month_tasks::MonthTasks,
    task_info::TaskInfo,
    util::{CliTaud, Settings, TauConfig, CONFIG_FILE_CONTENTS},
};

async fn start(settings: Settings, executor: Arc<Executor<'_>>) -> TaudResult<()> {
    let p2p_settings = P2pSettings {
        inbound: settings.accept_address,
        outbound_connections: settings.outbound_connections,
        external_addr: settings.accept_address,
        peers: settings.connect.clone(),
        seeds: settings.seeds.clone(),
        ..Default::default()
    };

    //
    //Raft
    //
    let mut raft = Raft::<TaskInfo>::new(settings.accept_address, settings.datastore_raft.clone())?;

    let raft_sender = raft.get_broadcast();
    let commits = raft.get_commits();
    let initial_sync_raft_sender = raft_sender.clone();

    //
    // RPC
    //
    let server_config = RpcServerConfig {
        socket_addr: settings.rpc_listener_url,
        use_tls: false,
        // this is all random filler that is meaningless bc tls is disabled
        identity_path: Default::default(),
        identity_pass: Default::default(),
    };

    let (rpc_snd, rpc_rcv) = async_channel::unbounded::<Option<TaskInfo>>();

    let rpc_interface = Arc::new(JsonRpcInterface::new(rpc_snd, settings.dataset_path.clone()));

    let dataset_path_cloned = settings.dataset_path.clone();
    let recv_update_from_rpc: smol::Task<TaudResult<()>> = executor.spawn(async move {
        loop {
            let task_info = rpc_rcv.recv().await.map_err(Error::from)?;
            if let Some(tk) = task_info {
                info!(target: "tau", "save the received task {:?}", tk);
                tk.save(&dataset_path_cloned)?;
                raft_sender.send(tk).await.map_err(Error::from)?;
            }
        }
    });

    let dataset_path_cloned = settings.dataset_path.clone();
    let recv_update_from_raft: smol::Task<TaudResult<()>> = executor.spawn(async move {
        loop {
            let task = commits.recv().await.map_err(Error::from)?;
            info!(target: "tau", "update from the commits");
            task.save(&dataset_path_cloned)?;
        }
    });

    let dataset_path_cloned = settings.dataset_path.clone();
    let initial_sync: smol::Task<TaudResult<()>> = executor.spawn(async move {
        info!(target: "tau", "Start initial sync");
        info!(target: "tau", "Upload local tasks");
        let tasks = MonthTasks::load_current_open_tasks(&dataset_path_cloned)?;

        for task in tasks {
            info!(target: "tau", "send local task {:?}", task);
            initial_sync_raft_sender.send(task).await.map_err(Error::from)?;
        }
        Ok(())
    });

    let executor_cloned = executor.clone();
    executor_cloned
        .spawn(listen_and_serve(server_config, rpc_interface, executor.clone()))
        .detach();

    // blocking
    raft.start(p2p_settings.clone(), executor.clone()).await?;

    recv_update_from_rpc.cancel().await;
    recv_update_from_raft.cancel().await;
    initial_sync.cancel().await;
    Ok(())
}

#[async_std::main]
async fn main() -> TaudResult<()> {
    let args = CliTaud::parse();

    let (lvl, conf) = log_config(args.verbose.into())?;
    TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto).map_err(Error::from)?;

    let config_path = get_config_path(args.config.clone(), "taud_config.toml")?;
    spawn_config(&config_path, CONFIG_FILE_CONTENTS)?;

    let config: TauConfig = Config::<TauConfig>::load(config_path)?;

    let settings = Settings::load(args, config)?;

    let ex = Arc::new(Executor::new());
    smol::block_on(ex.run(start(settings, ex.clone())))
}
