use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use easy_parallel::Parallel;
use futures_lite::future;
use log::error;
use serde_derive::Deserialize;
use serde_json::{json, Value};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use url::Url;

use darkfi::{
    cli_desc,
    crypto::address::Address,
    rpc::{
        jsonrpc,
        jsonrpc::{ErrorCode, JsonRequest, JsonResult},
        rpcserver2::{listen_and_serve, RequestHandler},
    },
    util::{
        cli::{log_config, spawn_config},
        expand_path,
        path::get_config_path,
    },
    wallet::walletdb::{WalletDb, WalletPtr},
    Result,
};

mod client;
use client::Client;

mod error;
use error::*;

const CONFIG_FILE: &str = "darkfid_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../darkfid_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "darkfid", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(long, default_value = "~/.config/darkfi/darkfid_wallet.db")]
    /// Path to the wallet database
    wallet_path: String,

    #[structopt(long, default_value = "changeme")]
    /// Password for the wallet database
    wallet_pass: String,

    #[structopt(long, default_value = "tcp://127.0.0.1:5397")]
    /// JSON-RPC listen URL
    rpc_listen: Url,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

pub struct Darkfid {
    client: Client,
}

#[async_trait]
impl RequestHandler for Darkfid {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if !req.params.is_array() {
            return jsonrpc::error(ErrorCode::InvalidParams, None, req.id).into()
        }

        let params = req.params.as_array().unwrap();

        match req.method.as_str() {
            Some("ping") => return self.pong(req.id, params).await,
            Some("keygen") => return self.keygen(req.id, params).await,
            Some("get_key") => return self.get_key(req.id, params).await,
            Some(_) | None => return jsonrpc::error(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }
}

impl Darkfid {
    pub async fn new(wallet: WalletPtr) -> Result<Self> {
        let client = Client::new(wallet).await?;
        Ok(Self { client })
    }

    // RPCAPI:
    // Returns a `pong` to the `ping` request.
    // --> {"jsonrpc": "2.0", "method": "ping", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "pong", "id": 1}
    async fn pong(&self, id: Value, _params: &[Value]) -> JsonResult {
        jsonrpc::response(json!("pong"), id).into()
    }

    // RPCAPI:
    // Attempts to generate a new keypair and returns its address upon success.
    // --> {"jsonrpc": "2.0", "method": "keygen", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "1DarkFi...", "id": 1}
    async fn keygen(&self, id: Value, _params: &[Value]) -> JsonResult {
        match self.client.keygen().await {
            Ok(a) => jsonrpc::response(json!(a.to_string()), id).into(),
            Err(e) => {
                error!("Failed creating keypair: {}", e);
                err_keygen(id)
            }
        }
    }

    // RPCAPI:
    // Fetches a keypair by given indexes from the wallet and returns it in an
    // encoded format. `-1` is supported to fetch all available keys.
    // --> {"jsonrpc": "2.0", "method": "get_key", "params": [1, 2], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["foo", "bar"], "id": 1}
    async fn get_key(&self, id: Value, params: &[Value]) -> JsonResult {
        let mut fetch_all = false;
        for i in params {
            if !i.is_i64() {
                return err_nan(id)
            }

            if i.as_i64() == Some(-1) {
                fetch_all = true;
                break
            }

            if i.as_i64() < Some(-1) {
                return err_lt1(id)
            }
        }

        let keypairs = match self.client.get_keypairs().await {
            Ok(v) => v,
            Err(e) => {
                error!("Failed fetching keypairs: {}", e);
                return err_kp_fetch(id)
            }
        };

        let mut ret = vec![];

        if fetch_all {
            ret = keypairs.iter().map(|x| Some(Address::from(x.public).to_string())).collect()
        } else {
            for i in params {
                // This cast is safe since we've already sorted out
                // all negative cases above.
                let idx = i.as_i64().unwrap() as usize;
                if let Some(kp) = keypairs.get(idx) {
                    ret.push(Some(Address::from(kp.public).to_string()));
                } else {
                    ret.push(None)
                }
            }
        }

        jsonrpc::response(json!(ret), id).into()
    }
}

async fn init_wallet(wallet_path: &str, wallet_pass: &str) -> Result<WalletPtr> {
    let expanded = expand_path(wallet_path)?;
    let wallet_path = format!("sqlite://{}", expanded.to_str().unwrap());
    let wallet = WalletDb::new(&wallet_path, wallet_pass).await?;
    Ok(wallet)
}

fn main() -> Result<()> {
    let args = Args::from_args_with_toml("").unwrap();
    let cfg_path = get_config_path(args.config, CONFIG_FILE)?;
    spawn_config(&cfg_path, CONFIG_FILE_CONTENTS.as_bytes())?;
    let args = Args::from_args_with_toml(&std::fs::read_to_string(cfg_path)?).unwrap();

    let (lvl, conf) = log_config(args.verbose.into())?;
    TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    // Initialize or load wallet
    let ex = Executor::new();
    let wallet = future::block_on(ex.run(init_wallet(&args.wallet_path, &args.wallet_pass)))?;

    // Initialize state
    let darkfid = future::block_on(ex.run(Darkfid::new(wallet)))?;
    let darkfid = Arc::new(darkfid);
    drop(ex);

    // https://docs.rs/smol/latest/smol/struct.Executor.html#examples
    let ex = Executor::new();
    let (signal, shutdown) = async_channel::unbounded::<()>();
    let (_, result) = Parallel::new()
        // Run four executor threads
        .each(0..4, |_| future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            future::block_on(async {
                listen_and_serve(args.rpc_listen, darkfid).await?;
                drop(signal);
                Ok::<(), darkfi::Error>(())
            })
        });

    result
}
