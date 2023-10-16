use std::{collections::HashSet, sync::Arc};

use darkfi::{
    async_daemonize, cli_desc,
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResult},
        server::{listen_and_serve, RequestHandler},
    },
    system::{StoppableTask, StoppableTaskPtr},
    Error, Result,
};
use darkfi_serial::async_trait;
use log::{debug, error, info};
use serde::Deserialize;
use smol::{
    lock::{Mutex, MutexGuard},
    stream::StreamExt,
    Executor,
};
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use url::Url;

mod stratum;

const CONFIG_FILE: &str = "darkfi_mmproxy.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../darkfi_mmproxy.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "darkfi-mmproxy", about = cli_desc!())]
struct Args {
    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(long, default_value = "tcp://127.0.0.1:3333")]
    /// JSON-RPC server listen URL
    rpc_listen: Url,

    #[structopt(long)]
    /// Set log file output
    log: Option<String>,
}

struct MiningProxy {
    /// JSON-RPC connection tracker
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
}

impl MiningProxy {
    fn new() -> Self {
        Self { rpc_connections: Mutex::new(HashSet::new()) }
    }
}

#[async_trait]
impl RequestHandler for MiningProxy {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        error!(target: "mmproxy::rpc", "--> {}", req.stringify().unwrap());

        match req.method.as_str() {
            "ping" => self.pong(req.id, req.params).await,
            _ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'_, HashSet<StoppableTaskPtr>> {
        self.rpc_connections.lock().await
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<Executor<'static>>) -> Result<()> {
    info!("Starting JSON-RPC server");
    let mmproxy = Arc::new(MiningProxy::new());
    let mmproxy_ = Arc::clone(&mmproxy);
    let rpc_task = StoppableTask::new();
    rpc_task.clone().start(
        listen_and_serve(args.rpc_listen, mmproxy.clone(), None, ex.clone()),
        |res| async move {
            match res {
                Ok(()) | Err(Error::RpcServerStopped) => mmproxy_.stop_connections().await,
                Err(e) => error!("Failed stopping JSON-RPC server: {}", e),
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new(ex)?;
    signals_handler.wait_termination(signals_task).await?;
    info!("Caught termination signal, cleaning up and exiting...");

    Ok(())
}
