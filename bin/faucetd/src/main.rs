use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use easy_parallel::Parallel;
use futures_lite::future;
use log::info;
use serde_derive::Deserialize;
use serde_json::{json, Value};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use url::Url;

use darkfi::{
    cli_desc,
    rpc::{
        jsonrpc,
        jsonrpc::{
            ErrorCode::{InvalidParams, MethodNotFound},
            JsonRequest, JsonResult,
        },
        rpcserver2::{listen_and_serve, RequestHandler},
    },
    util::{
        cli::{log_config, spawn_config},
        path::get_config_path,
    },
    Result,
};

const CONFIG_FILE: &str = "faucetd_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../faucetd_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "faucetd", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(long, default_value = "testnet")]
    /// Chain to use (testnet, mainnet)
    chain: String,

    #[structopt(long, default_value = "tcp://127.0.0.1:5381")]
    /// JSON-RPC listen URL
    rpc_listen: Url,

    #[structopt(long, default_value = "3600")]
    /// Airdrop timeout limit in seconds
    airdrop_timeout: u64,

    #[structopt(long, default_value = "10")]
    /// Airdrop amount limit
    airdrop_limit: u64,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

pub struct Faucetd {
    airdrop_timeout: u64,
    airdrop_limit: u64,
}

#[async_trait]
impl RequestHandler for Faucetd {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if !req.params.is_array() {
            return jsonrpc::error(InvalidParams, None, req.id).into()
        }

        let params = req.params.as_array().unwrap();

        match req.method.as_str() {
            Some("ping") => return self.pong(req.id, params).await,
            Some("airdrop") => return self.airdrop(req.id, params).await,
            Some(_) | None => return jsonrpc::error(MethodNotFound, None, req.id).into(),
        }
    }
}

impl Faucetd {
    pub async fn new(timeout: u64, limit: u64) -> Result<Self> {
        Ok(Self { airdrop_timeout: timeout, airdrop_limit: limit })
    }

    // RPCAPI:
    // Returns a `pong` to the `ping` request.
    // --> {"jsonrpc": "2.0", "method": "ping", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "pong", "id": 1}
    async fn pong(&self, id: Value, _params: &[Value]) -> JsonResult {
        jsonrpc::response(json!("pong"), id).into()
    }

    // Processes an airdrop request and airdrops requested amount to address.
    /// Returns transaction ID upon success.
    // --> {"jsonrpc": "2.0", "method": "airdrop", "params": ["1DarkFi...", 10], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "txID", "id": 1}
    async fn airdrop(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 2 || !params[0].is_string() || !params[1].is_u64() {
            return jsonrpc::error(InvalidParams, None, id).into()
        }

        todo!()
        // Check map for timeout
        // Update timeout if successful
        // Make transaction
        // Broadcast
        // Return txid
    }
}

async fn realmain(args: Args, ex: Arc<Executor<'_>>) -> Result<()> {
    // We use this handler to block this function after detaching all
    // tasks, and to catch a shutdown signal, where we can clean up and
    // exit gracefully.
    let (signal, shutdown) = async_channel::bounded::<()>(1);
    ctrlc_async::set_async_handler(async move {
        signal.send(()).await.unwrap();
    })
    .unwrap();

    // Initialize program state
    let faucetd = Faucetd::new(args.airdrop_timeout, args.airdrop_limit).await?;
    let faucetd = Arc::new(faucetd);

    // JSON-RPC server
    info!("Starting JSON-RPC server");
    ex.spawn(listen_and_serve(args.rpc_listen, faucetd)).detach();

    // TODO: Task to periodically scan map and drop timeouts. We don't
    // want to keep a log of airdrops.

    // Wait for SIGINT
    shutdown.recv().await?;
    print!("\r");
    info!("Caught termination signal, cleaning up and exiting...");

    Ok(())
}

fn main() -> Result<()> {
    let args = Args::from_args_with_toml("").unwrap();
    let cfg_path = get_config_path(args.config, CONFIG_FILE)?;
    spawn_config(&cfg_path, CONFIG_FILE_CONTENTS.as_bytes())?;
    let args = Args::from_args_with_toml(&std::fs::read_to_string(cfg_path)?).unwrap();

    let (lvl, conf) = log_config(args.verbose.into())?;
    TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    // https://docs.rs/smol/latest/smol/struct.Executor.html#examples
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();
    let (_, result) = Parallel::new()
        // Run four executor threads
        .each(0..4, |_| future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            future::block_on(async {
                realmain(args, ex.clone()).await?;
                drop(signal);
                Ok::<(), darkfi::Error>(())
            })
        });

    result
}
