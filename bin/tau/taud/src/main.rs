use std::sync::Arc;

use async_executor::Executor;
use clap::Parser;
use simplelog::{ColorChoice, TermLogger, TerminalMode};

use darkfi::{
    net::Settings as P2pSettings,
    raft::Raft,
    rpc::rpcserver::{listen_and_serve, RpcServerConfig},
    util::{
        cli::{log_config, spawn_config, Config},
        path::get_config_path,
        sleep,
    },
    Error, Result,
};

mod error;
mod jsonrpc;
mod month_tasks;
mod task_info;
mod util;

use crate::{
    error::TaudResult,
    jsonrpc::JsonRpcInterface,
    task_info::TaskInfo,
    util::{CliTaud, Settings, TauConfig, CONFIG_FILE_CONTENTS},
};

async fn start(settings: Settings, executor: Arc<Executor<'_>>) -> Result<()> {
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

    let rpc_interface = Arc::new(JsonRpcInterface::new(rpc_snd, settings.dataset_path));

    let recv_update_from_raft: smol::Task<TaudResult<()>> = executor.spawn(async move {
        loop {
            let task_info = rpc_rcv.recv().await.map_err(Error::from)?;

            if let Some(tk) = task_info {
                raft_sender.send(tk).await.map_err(Error::from)?;
            }

            // XXX THIS FOR DEBUGING
            sleep(1).await;

            let recv_commits = commits.lock().await;

            for task_info in recv_commits.iter() {
                task_info.save()?;
                if task_info.get_state() == "open" {
                    task_info.activate()?;
                } else {
                    let mut mt = task_info.get_month_task()?;
                    mt.remove(&task_info.get_ref_id());
                }
            }
        }
    });

    let ex2 = executor.clone();
    ex2.spawn(listen_and_serve(server_config, rpc_interface, executor.clone())).detach();

    raft.start(p2p_settings.clone(), executor.clone()).await?;

    recv_update_from_raft.cancel().await;
    Ok(())
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = CliTaud::parse();

    let (lvl, conf) = log_config(args.verbose.into())?;
    TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    let config_path = get_config_path(args.config.clone(), "taud_config.toml")?;
    spawn_config(&config_path, CONFIG_FILE_CONTENTS)?;

    let config: TauConfig = Config::<TauConfig>::load(config_path)?;

    let settings = Settings::load(args, config)?;

    let ex = Arc::new(Executor::new());
    smol::block_on(ex.run(start(settings, ex.clone())))
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{create_dir_all, remove_dir_all},
        path::PathBuf,
    };

    use super::*;
    use crate::{
        error::TaudResult, month_tasks::MonthTasks, task_info::TaskInfo, util::get_current_time,
    };

    const TEST_DATA_PATH: &str = "/tmp/test_tau_data";

    fn get_path() -> Result<PathBuf> {
        remove_dir_all(TEST_DATA_PATH).ok();

        let path = PathBuf::from(TEST_DATA_PATH);

        // mkdir dataset_path if not exists
        create_dir_all(path.join("month"))?;
        create_dir_all(path.join("task"))?;
        Ok(path)
    }

    #[test]
    fn load_and_save_tasks() -> TaudResult<()> {
        let dataset_path = get_path()?;

        // load and save TaskInfo
        ///////////////////////

        let mut task = TaskInfo::new("test_title", "test_desc", None, 0.0, &dataset_path)?;

        task.save()?;

        let t_load = TaskInfo::load(&task.get_ref_id(), &dataset_path)?;

        assert_eq!(task, t_load);

        task.set_title("test_title_2");

        task.save()?;

        let t_load = TaskInfo::load(&task.get_ref_id(), &dataset_path)?;

        assert_eq!(task, t_load);

        // load and save MonthTasks
        ///////////////////////

        let task_tks = vec![];

        let mut mt = MonthTasks::new(&task_tks, &dataset_path);

        mt.save()?;

        let mt_load = MonthTasks::load_or_create(&get_current_time(), &dataset_path)?;

        assert_eq!(mt, mt_load);

        mt.add(&task.get_ref_id());

        mt.save()?;

        let mt_load = MonthTasks::load_or_create(&get_current_time(), &dataset_path)?;

        assert_eq!(mt, mt_load);

        // activate task
        ///////////////////////

        let task = TaskInfo::new("test_title_3", "test_desc", None, 0.0, &dataset_path)?;

        task.save()?;

        let mt_load = MonthTasks::load_or_create(&get_current_time(), &dataset_path)?;

        assert!(!mt_load.get_task_tks().contains(&task.get_ref_id()));

        task.activate()?;

        let mt_load = MonthTasks::load_or_create(&get_current_time(), &dataset_path)?;

        assert!(mt_load.get_task_tks().contains(&task.get_ref_id()));

        remove_dir_all(TEST_DATA_PATH).ok();

        Ok(())
    }
}
