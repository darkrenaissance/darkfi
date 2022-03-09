use std::{fs::create_dir_all, path::PathBuf, sync::Arc};

use async_executor::Executor;
use async_trait::async_trait;
use clap::{IntoApp, Parser};
use log::debug;
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

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Timestamp {
    //XXX change this
    time: String,
}

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
struct TaskInfo {
    ref_id: String,
    id: u32,
    title: String,
    desc: String,
    assign: String,
    project: String,
    due: Timestamp,
    rank: u32,
    created_at: Timestamp,
    events: Vec<TaskEvent>,
    comments: Vec<Comment>,
    settings: Settings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MonthTasks {
    created_at: Timestamp,
    settings: Settings,
    task_tks: Vec<TaskInfo>,
}

impl TaskInfo {
    pub fn new(
        ref_id: String,
        id: u32,
        title: String,
        desc: String,
        assign: String,
        project: String,
        due: Timestamp,
        rank: u32,
        created_at: Timestamp,
        settings: Settings,
    ) -> Self {
        Self {
            ref_id,
            id,
            title,
            desc,
            assign,
            project,
            due,
            rank,
            created_at,
            comments: vec![],
            events: vec![],
            settings,
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

    let server_config = RpcServerConfig {
        socket_addr: config.rpc_listener_url.url.parse()?,
        use_tls: false,
        // this is all random filler that is meaningless bc tls is disabled
        identity_path: Default::default(),
        identity_pass: Default::default(),
    };

    let rpc_interface = Arc::new(JsonRpcInterface {});

    listen_and_serve(server_config, rpc_interface, executor).await
}

struct JsonRpcInterface {}

#[async_trait]
impl RequestHandler for JsonRpcInterface {
    async fn handle_request(&self, req: JsonRequest, _executor: Arc<Executor<'_>>) -> JsonResult {
        if req.params.as_array().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, req.id))
        }

        debug!(target: "RPC", "--> {}", serde_json::to_string(&req).unwrap());

        match req.method.as_str() {
            Some("cmd_add") => return self.cmd_add(req.id, req.params).await,
            Some(_) | None => return JsonResult::Err(jsonerr(MethodNotFound, None, req.id)),
        }
    }
}

impl JsonRpcInterface {
    // --> {"method": "cmd_add", "params": [String]}
    // <-- {"result": "params"}
    async fn cmd_add(&self, id: Value, _params: Value) -> JsonResult {
        JsonResult::Resp(jsonresp(json!("New task added"), id))
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
