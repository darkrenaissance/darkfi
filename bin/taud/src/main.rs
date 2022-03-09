use std::{fs::create_dir_all, path::PathBuf, sync::Arc};

use async_executor::Executor;
use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use clap::{IntoApp, Parser};
use log::debug;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use simplelog::{ColorChoice, TermLogger, TerminalMode};

use darkfi::{
    rpc::{
        jsonrpc::{error as jsonerr, response as jsonresp, ErrorCode::*, JsonRequest, JsonResult},
        rpcserver::{listen_and_serve, RequestHandler, RpcServerConfig},
    },
    util::{
        cli::{log_config, spawn_config, Config, UrlConfig},
        expand_path, join_config_path,
    },
    Error, Result,
};

const CONFIG_FILE_CONTENTS: &[u8] = include_bytes!("../taud_config.toml");

/// taud cli
#[derive(Parser)]
#[clap(name = "taud")]
pub struct CliTaud {
    /// Sets a custom config file
    #[clap(short, long)]
    pub config: Option<String>,
    /// Increase verbosity
    #[clap(short, parse(from_occurrences))]
    pub verbose: u8,
}

fn random_ref_id() -> String {
    thread_rng().sample_iter(&Alphanumeric).take(30).map(char::from).collect()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Timestamp(String);

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TauConfig {
    /// path to dataset
    pub dataset_path: String,
    /// Path to DER-formatted PKCS#12 archive. (used only with tls listener url)
    pub tls_identity_path: String,
    /// The address where taud should bind its RPC socket
    pub rpc_listener_url: UrlConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Settings {
    dataset_path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TaskEvent {
    action: String,
    timestamp: Timestamp,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Comment {
    content: String,
    author: String,
    timestamp: Timestamp,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MonthTasks {
    created_at: Timestamp,
    settings: Settings,
    task_tks: Vec<TaskInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TaskInfo {
    ref_id: String,
    id: u32,
    title: String,
    desc: String,
    assign: Vec<String>,
    project: Vec<String>,
    due: Option<Timestamp>,
    rank: u32,
    created_at: Timestamp,
    events: Vec<TaskEvent>,
    comments: Vec<Comment>,
}

impl TaskInfo {
    pub fn new(title: String, desc: String, due: Option<Timestamp>, rank: u32) -> Self {
        // TODO
        // check due date

        // generate ref_id
        let ref_id = random_ref_id();

        // XXX must find the next free id
        let mut rng = rand::thread_rng();
        let id: u32 = rng.gen();

        let created_at: Timestamp = Timestamp(Utc::now().to_string());

        Self {
            ref_id,
            id,
            title,
            desc,
            assign: vec![],
            project: vec![],
            due,
            rank,
            created_at,
            comments: vec![],
            events: vec![],
        }
    }

    fn assign(&mut self, n: String) {
        self.assign.push(n);
    }

    fn project(&mut self, p: String) {
        self.project.push(p);
    }

    fn save(&self) -> Result<()> {
        Ok(())
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
        let args = params.as_array();
        if args.is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id))
        }
        let args = args.unwrap();

        if args.len() != 6 {
            return JsonResult::Err(jsonerr(InvalidParams, None, id))
        }

        let mut task: TaskInfo;

        match (args[0].as_str(), args[1].as_str(), args[5].as_u64()) {
            (Some(title), Some(desc), Some(rank)) => {
                let due: Option<Timestamp> = if args[4].as_str().is_some() {
                    let timestamp = args[4].as_str().unwrap().parse::<i64>();

                    if timestamp.is_err() {
                        return JsonResult::Err(jsonerr(
                            InvalidParams,
                            Some("invalid timestamp".into()),
                            id,
                        ))
                    }

                    Some(Timestamp(Utc.timestamp(timestamp.unwrap(), 0).to_string()))
                } else {
                    None
                };

                task = TaskInfo::new(title.to_string(), desc.to_string(), due, rank as u32);
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
                task.assign(a.as_str().unwrap().into());
            }
        }

        let project = args[3].as_array();
        if project.is_some() && project.unwrap().len() > 0 {
            for p in project.unwrap() {
                task.project(p.as_str().unwrap().into());
            }
        }

        match task.save() {
            Ok(()) => JsonResult::Resp(jsonresp(json!(true), id)),
            Err(e) => JsonResult::Err(jsonerr(ServerError(-32603), Some(e.to_string()), id)),
        }
    }
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

    let config: TauConfig = Config::<TauConfig>::load(config_path)?;

    let ex = Arc::new(Executor::new());
    smol::block_on(start(config, ex))
}
