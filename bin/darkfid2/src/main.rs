use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use easy_parallel::Parallel;
use futures_lite::future;
use serde_derive::Deserialize;
use serde_json::{json, Value};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use url::Url;

use darkfi::{
    cli_desc,
    net::transport::{TcpTransport, TlsTransport},
    rpc::{
        jsonrpc,
        jsonrpc::{ErrorCode, JsonRequest, JsonResult},
        rpcserver2::{listen_and_serve, RequestHandler},
    },
    util::{
        cli::{log_config, spawn_config},
        path::get_config_path,
    },
    Error, Result,
};

const CONFIG_FILE: &str = "darkfid_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../darkfid_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "darkfid", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(long, default_value = "~/.config/darkfi/darkfid_client.db")]
    /// Path to the client database
    database_path: String,

    #[structopt(long, default_value = "~/.config/darkfi/darkfid_wallet.db")]
    /// Path to the wallet database
    wallet_path: String,

    #[structopt(long, default_value = "tcp://127.0.0.1:5397")]
    /// JSON-RPC listen URL
    rpc_listen: String,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

pub struct Darkfid;

#[async_trait]
impl RequestHandler for Darkfid {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if req.params.as_array().is_none() {
            return jsonrpc::error(ErrorCode::InvalidParams, None, req.id).into()
        }

        match req.method.as_str() {
            Some("ping") => return self.pong(req.id, req.params).await,
            Some(_) | None => return jsonrpc::error(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }
}

impl Darkfid {
    // RPCAPI:
    // Returns a `pong` to the `ping` request.
    // --> {"jsonrpc":"2.0","method":"ping","params":[],"id":1}
    // <-- {"jsonrpc":"2.0","result":"pong","id":1}
    async fn pong(&self, id: Value, _params: Value) -> JsonResult {
        jsonrpc::response(json!("pong"), id).into()
    }
}

fn main() -> Result<()> {
    let args = Args::from_args_with_toml("").unwrap();
    let cfg_path = get_config_path(args.config.clone(), CONFIG_FILE)?;
    spawn_config(&cfg_path, CONFIG_FILE_CONTENTS.as_bytes())?;
    let args = Args::from_args_with_toml(&std::fs::read_to_string(cfg_path)?).unwrap();

    let (lvl, conf) = log_config(args.verbose.into())?;
    TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    // Validate args

    let ex = Executor::new();
    let (signal, shutdown) = async_channel::unbounded::<()>();

    // // https://docs.rs/smol/latest/smol/struct.Executor.html#examples
    // let (_, result) = Parallel::new()
    // // Run four executor threads
    // .each(0..4, |_| future::block_on(ex.run(shutdown.recv())))
    // // Run the main future on the current thread.
    // .finish(|| {
    // future::block_on(async {
    // realmain(args).await?;
    // drop(signal);
    // Ok::<(), darkfi::Error>(())
    // })
    // });

    // result
    Ok(())
}
