use std::{fs::create_dir_all, path::PathBuf, sync::Arc};

use async_executor::Executor;
use async_trait::async_trait;
use clap::{IntoApp, Parser};
use log::debug;
use serde_json::{json, Value};
use simplelog::{ColorChoice, TermLogger, TerminalMode};

use darkfi::{
    rpc::{
        jsonrpc::{error as jsonerr, response as jsonresp, ErrorCode::*, JsonRequest, JsonResult},
        rpcserver::{listen_and_serve, RequestHandler, RpcServerConfig},
    },
    util::{
        cli::{log_config, spawn_config, Config},
        expand_path, join_config_path,
    },
    Error, Result,
};

mod month_tasks;
mod task_info;
mod util;

use crate::{
    month_tasks::MonthTasks,
    task_info::TaskInfo,
    util::{get_current_time, CliTaud, Settings, TauConfig, Timestamp, CONFIG_FILE_CONTENTS},
};
struct JsonRpcInterface {
    settings: Settings,
}

#[async_trait]
impl RequestHandler for JsonRpcInterface {
    async fn handle_request(&self, req: JsonRequest, _executor: Arc<Executor<'_>>) -> JsonResult {
        if req.params.as_array().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, req.id))
        }

        debug!(target: "RPC", "--> {}", serde_json::to_string(&req).unwrap());

        match req.method.as_str() {
            Some("add") => return self.add(req.id, req.params).await,
            Some("list") => return self.list(req.id, req.params).await,
            Some(_) | None => return JsonResult::Err(jsonerr(MethodNotFound, None, req.id)),
        }
    }
}

impl JsonRpcInterface {
    // RPCAPI:
    // Add new task and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "add", "params": ["title", "desc", ["assign"], ["project"], "due", "rank"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    async fn add(&self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array().unwrap();

        if args.len() != 6 {
            return JsonResult::Err(jsonerr(InvalidParams, None, id))
        }

        let mut task: TaskInfo;

        match (args[0].as_str(), args[1].as_str(), args[5].as_u64()) {
            (Some(title), Some(desc), Some(rank)) => {
                let due: Option<Timestamp> = if args[4].is_i64() {
                    let timestamp = args[4].as_i64().unwrap();
                    let timestamp = Timestamp(timestamp);

                    if timestamp < get_current_time() {
                        return JsonResult::Err(jsonerr(
                            InvalidParams,
                            Some("invalid due date".into()),
                            id,
                        ))
                    }

                    Some(timestamp)
                } else {
                    None
                };

                match TaskInfo::new(title, desc, due, rank as u32, &self.settings) {
                    Ok(t) => task = t,
                    Err(e) => {
                        return JsonResult::Err(jsonerr(InternalError, Some(e.to_string()), id))
                    }
                }
            }
            (None, _, _) => {
                return JsonResult::Err(jsonerr(InvalidParams, Some("invalid title".into()), id))
            }
            (_, None, _) => {
                return JsonResult::Err(jsonerr(InvalidParams, Some("invalid desc".into()), id))
            }
            (_, _, None) => {
                return JsonResult::Err(jsonerr(InvalidParams, Some("invalid rank".into()), id))
            }
        }

        let assign = args[2].as_array();
        if assign.is_some() && assign.unwrap().len() > 0 {
            for a in assign.unwrap() {
                task.assign(a.as_str().unwrap());
            }
        }

        let project = args[3].as_array();
        if project.is_some() && project.unwrap().len() > 0 {
            for p in project.unwrap() {
                task.project(p.as_str().unwrap());
            }
        }

        let result = || -> Result<()> {
            task.save()?;
            task.activate()?;
            Ok(())
        };

        match result() {
            Ok(()) => JsonResult::Resp(jsonresp(json!(true), id)),
            Err(e) => JsonResult::Err(jsonerr(ServerError(-32603), Some(e.to_string()), id)),
        }
    }

    // RPCAPI:
    // List tasks
    // --> {"jsonrpc": "2.0", "method": "list", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [task, ...], "id": 1}
    async fn list(&self, id: Value, _params: Value) -> JsonResult {
        let tasks: Result<Vec<TaskInfo>> = MonthTasks::load_current_open_tasks(&self.settings);

        match tasks {
            Ok(tks) => JsonResult::Resp(jsonresp(json!(tks), id)),
            Err(e) => JsonResult::Err(jsonerr(ServerError(-32603), Some(e.to_string()), id)),
        }
    }
}

async fn start(config: TauConfig, executor: Arc<Executor<'_>>) -> Result<()> {
    if config.dataset_path.is_empty() {
        return Err(Error::ParseFailed("Failed to parse dataset_path"))
    }

    let dataset_path = expand_path(&config.dataset_path)?;

    // mkdir dataset_path if not exists
    create_dir_all(dataset_path.join("month"))?;
    create_dir_all(dataset_path.join("task"))?;

    let settings = Settings { dataset_path };

    let server_config = RpcServerConfig {
        socket_addr: config.rpc_listener_url.url.parse()?,
        use_tls: false,
        // this is all random filler that is meaningless bc tls is disabled
        identity_path: Default::default(),
        identity_pass: Default::default(),
    };

    let rpc_interface = Arc::new(JsonRpcInterface { settings });

    listen_and_serve(server_config, rpc_interface, executor).await
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = CliTaud::parse();
    let matches = CliTaud::into_app().get_matches();

    let config_path = if args.config.is_some() {
        expand_path(&args.config.unwrap())?
    } else {
        join_config_path(&PathBuf::from("taud_config.toml"))?
    };

    // Spawn config file if it's not in place already.
    spawn_config(&config_path, CONFIG_FILE_CONTENTS)?;

    let verbosity_level = matches.occurrences_of("verbose");

    let (lvl, conf) = log_config(verbosity_level)?;

    TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    let config: TauConfig = Config::<TauConfig>::load(config_path.to_path_buf())?;

    let ex = Arc::new(Executor::new());
    smol::block_on(ex.run(start(config, ex.clone())))
}

#[cfg(test)]
mod tests {
    use std::fs::create_dir_all;

    use crate::{month_tasks::MonthTasks, task_info::TaskInfo};

    use super::*;

    fn get_path() -> Result<PathBuf> {
        let path = PathBuf::from("/tmp/test_tau_data");

        // mkdir dataset_path if not exists
        create_dir_all(path.join("month"))?;
        create_dir_all(path.join("task"))?;
        Ok(path)
    }

    #[test]
    fn load_and_save_tasks() -> Result<()> {
        let settings = Settings { dataset_path: get_path()?.clone() };

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

        Ok(())
    }

    #[test]
    fn test_activate_task() -> Result<()> {
        let settings = Settings { dataset_path: get_path()?.clone() };

        // activate task
        ///////////////////////

        let task = TaskInfo::new("test_title_3", "test_desc", None, 0, &settings)?;

        task.save()?;

        let mt_load = MonthTasks::load_or_create(&get_current_time(), &settings)?;

        assert!(!mt_load.get_task_tks().contains(&task.get_ref_id()));

        task.activate()?;

        let mt_load = MonthTasks::load_or_create(&get_current_time(), &settings)?;

        assert!(mt_load.get_task_tks().contains(&task.get_ref_id()));

        Ok(())
    }
}
