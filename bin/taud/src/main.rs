use std::{fs::create_dir_all, sync::Arc};

use async_executor::Executor;
use clap::{IntoApp, Parser};
use simplelog::{ColorChoice, TermLogger, TerminalMode};

use darkfi::{
    net::Settings as P2pSettings,
    rpc::rpcserver::{listen_and_serve, RpcServerConfig},
    util::{
        cli::{log_config, spawn_config, Config},
        expand_path,
        path::get_config_path,
    },
    Error, Result,
};

mod crdt;
mod error;
mod jsonrpc;
mod month_tasks;
mod task_info;
mod util;

use crate::{
    crdt::Node,
    jsonrpc::JsonRpcInterface,
    task_info::TaskInfo,
    util::{CliTaud, Settings, TauConfig, CONFIG_FILE_CONTENTS},
};

async fn start(config: TauConfig, executor: Arc<Executor<'_>>) -> Result<()> {
    if config.dataset_path.is_empty() {
        return Err(Error::ParseFailed("Failed to parse dataset_path"))
    }

    let dataset_path = expand_path(&config.dataset_path)?;

    // mkdir dataset_path if not exists
    create_dir_all(dataset_path.join("month"))?;
    create_dir_all(dataset_path.join("task"))?;

    let settings = Settings { dataset_path };

    //
    // Crdt
    //

    let p2p_settings = P2pSettings::default();

    let node = Node::new("node", p2p_settings).await;

    let ex2 = executor.clone();
    let node2 = node.clone();
    let crdt_task = executor.spawn(node2.start(ex2.clone()));

    //
    // RPC
    //
    let server_config = RpcServerConfig {
        socket_addr: config.rpc_listener_url.url.parse()?,
        use_tls: false,
        // this is all random filler that is meaningless bc tls is disabled
        identity_path: Default::default(),
        identity_pass: Default::default(),
    };

    let (snd, rcv) = async_channel::unbounded::<TaskInfo>();

    let rpc_interface = Arc::new(JsonRpcInterface::new(snd, settings));

    let node2 = node.clone();
    let recv_update_from_rpc: smol::Task<Result<()>> = executor.spawn(async move {
        loop {
            let task_info = rcv.recv().await?;
            node2.clone().send_event(task_info).await?;
        }
    });

    listen_and_serve(server_config, rpc_interface, executor).await?;

    crdt_task.cancel().await;
    recv_update_from_rpc.cancel().await;
    Ok(())
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = CliTaud::parse();
    let matches = CliTaud::command().get_matches();

    let verbosity_level = matches.occurrences_of("verbose");
    let (lvl, conf) = log_config(verbosity_level)?;
    TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    let config_path = get_config_path(args.config, "taud_config.toml")?;
    spawn_config(&config_path, CONFIG_FILE_CONTENTS)?;

    let config: TauConfig = Config::<TauConfig>::load(config_path)?;

    let ex = Arc::new(Executor::new());
    smol::block_on(ex.run(start(config, ex.clone())))
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{create_dir_all, remove_dir_all},
        path::PathBuf,
    };

    use super::*;
    use crate::{month_tasks::MonthTasks, task_info::TaskInfo, util::get_current_time};

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
    fn load_and_save_tasks() -> Result<()> {
        let settings = Settings { dataset_path: get_path()? };

        // load and save TaskInfo
        ///////////////////////

        let mut task = TaskInfo::new("test_title", "test_desc", None, 0, &settings)?;

        task.save()?;

        let t_load = TaskInfo::load(&task.get_ref_id(), &settings)?;

        assert_eq!(task, t_load);

        task.set_title("test_title_2");

        task.save()?;

        let t_load = TaskInfo::load(&task.get_ref_id(), &settings)?;

        assert_eq!(task, t_load);

        // load and save MonthTasks
        ///////////////////////

        let task_tks = vec![];

        let mut mt = MonthTasks::new(&task_tks, &settings);

        mt.save()?;

        let mt_load = MonthTasks::load_or_create(&get_current_time(), &settings)?;

        assert_eq!(mt, mt_load);

        mt.add(&task.get_ref_id());

        mt.save()?;

        let mt_load = MonthTasks::load_or_create(&get_current_time(), &settings)?;

        assert_eq!(mt, mt_load);

        // activate task
        ///////////////////////

        let task = TaskInfo::new("test_title_3", "test_desc", None, 0, &settings)?;

        task.save()?;

        let mt_load = MonthTasks::load_or_create(&get_current_time(), &settings)?;

        assert!(!mt_load.get_task_tks().contains(&task.get_ref_id()));

        task.activate()?;

        let mt_load = MonthTasks::load_or_create(&get_current_time(), &settings)?;

        assert!(mt_load.get_task_tks().contains(&task.get_ref_id()));

        remove_dir_all(TEST_DATA_PATH).ok();

        Ok(())
    }
}
