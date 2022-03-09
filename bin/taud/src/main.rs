use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::Arc,
};

use async_executor::Executor;
use async_trait::async_trait;
use log::debug;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use simplelog::{ColorChoice, LevelFilter, TermLogger, TerminalMode};

use darkfi::{
    rpc::{
        jsonrpc::{error as jsonerr, response as jsonresp, ErrorCode::*, JsonRequest, JsonResult},
        rpcserver::{listen_and_serve, RequestHandler, RpcServerConfig},
    },
    Result,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Timestamp {
    //XXX change this
    time: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Config {
    path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Settings {
    config: Config,
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

async fn start(executor: Arc<Executor<'_>>) -> Result<()> {
    let rpc_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 7777);

    let server_config = RpcServerConfig {
        socket_addr: rpc_addr,
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
    TermLogger::init(
        LevelFilter::Debug,
        simplelog::Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )?;

    let ex = Arc::new(Executor::new());
    smol::block_on(start(ex))
}
