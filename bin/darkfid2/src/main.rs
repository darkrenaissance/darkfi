use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use easy_parallel::Parallel;
use futures_lite::future;
use log::{error, info};
use serde_derive::Deserialize;
use serde_json::{json, Value};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use url::Url;

use darkfi::{
    cli_desc,
    crypto::{
        address::Address,
        keypair::{Keypair, PublicKey, SecretKey},
    },
    rpc::{
        jsonrpc,
        jsonrpc::{
            ErrorCode::{InternalError, InvalidParams, MethodNotFound},
            JsonRequest, JsonResult,
        },
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
    synced: Mutex<bool>,
}

#[async_trait]
impl RequestHandler for Darkfid {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if !req.params.is_array() {
            return jsonrpc::error(InvalidParams, None, req.id).into()
        }

        let params = req.params.as_array().unwrap();

        match req.method.as_str() {
            Some("ping") => return self.pong(req.id, params).await,
            Some("keygen") => return self.keygen(req.id, params).await,
            Some("get_key") => return self.get_key(req.id, params).await,
            Some("export_keypair") => return self.export_keypair(req.id, params).await,
            Some("import_keypair") => return self.import_keypair(req.id, params).await,
            Some("set_default_address") => return self.set_default_address(req.id, params).await,
            Some(_) | None => return jsonrpc::error(MethodNotFound, None, req.id).into(),
        }
    }
}

impl Darkfid {
    pub async fn new(wallet: WalletPtr) -> Result<Self> {
        let client = Client::new(wallet).await?;
        Ok(Self { client, synced: Mutex::new(false) })
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
    // Fetches public keys by given indexes from the wallet and returns it in an
    // encoded format. `-1` is supported to fetch all available keys.
    // --> {"jsonrpc": "2.0", "method": "get_key", "params": [1, 2], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["foo", "bar"], "id": 1}
    async fn get_key(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.is_empty() {
            return jsonrpc::error(InvalidParams, None, id).into()
        }

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
                // This cast is safe on 64bit since we've already sorted out
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

    // RPCAPI:
    // Exports the given keypair index.
    // Returns the encoded secret key upon success.
    // --> {"jsonrpc": "2.0", "method": "export_keypair", "params": [0], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "foobar", "id": 1}
    async fn export_keypair(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_u64() {
            return jsonrpc::error(InvalidParams, None, id).into()
        }

        let keypairs = match self.client.get_keypairs().await {
            Ok(v) => v,
            Err(e) => {
                error!("Failed fetching keypairs: {}", e);
                return err_kp_fetch(id)
            }
        };

        if let Some(kp) = keypairs.get(params[0].as_u64().unwrap() as usize) {
            return jsonrpc::response(json!(kp.secret.to_bytes()), id).into()
        }

        err_kp_not_found(id)
    }

    // RPCAPI:
    // Imports a given secret key into the wallet as a keypair.
    // Returns the public counterpart as the result upon success.
    // --> {"jsonrpc": "2.0", "method": "import_keypair", "params": ["foobar"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "pubfoobar", "id": 1}
    async fn import_keypair(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_string() {
            return jsonrpc::error(InvalidParams, None, id).into()
        }

        let bytes: [u8; 32] = match serde_json::from_str(params[0].as_str().unwrap()) {
            Ok(v) => v,
            Err(e) => {
                error!("Failed parsing secret key from string: {}", e);
                return err_invalid_kp(id)
            }
        };

        let secret = match SecretKey::from_bytes(bytes) {
            Ok(v) => v,
            Err(e) => {
                error!("Failed parsing secret key from string: {}", e);
                return err_invalid_kp(id)
            }
        };

        let public = PublicKey::from_secret(secret);
        let keypair = Keypair { secret, public };
        let address = Address::from(public).to_string();

        match self.client.put_keypair(&keypair).await {
            Ok(()) => {}
            Err(e) => {
                error!("Failed inserting keypair into wallet: {}", e);
                return jsonrpc::error(InternalError, None, id).into()
            }
        };

        jsonrpc::response(json!(address), id).into()
    }

    // RPCAPI:
    // Sets the default wallet address to the given index.
    // Returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "set_default_address", "params": [2], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    async fn set_default_address(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_u64() {
            return jsonrpc::error(InvalidParams, None, id).into()
        }

        let idx = params[0].as_u64().unwrap();

        let keypairs = match self.client.get_keypairs().await {
            Ok(v) => v,
            Err(e) => {
                error!("Failed fetching keypairs: {}", e);
                return err_kp_fetch(id)
            }
        };

        if keypairs.len() as u64 != idx - 1 {
            return err_kp_not_found(id)
        }

        let kp = keypairs[idx as usize];
        match self.client.set_default_keypair(&kp.public).await {
            Ok(()) => {}
            Err(e) => {
                error!("Failed setting default keypair: {}", e);
                return jsonrpc::error(InternalError, None, id).into()
            }
        };

        jsonrpc::response(json!(true), id).into()
    }
}

async fn init_wallet(wallet_path: &str, wallet_pass: &str) -> Result<WalletPtr> {
    let expanded = expand_path(wallet_path)?;
    let wallet_path = format!("sqlite://{}", expanded.to_str().unwrap());
    let wallet = WalletDb::new(&wallet_path, wallet_pass).await?;
    Ok(wallet)
}

async fn realmain(args: Args, darkfid: Arc<Darkfid>, ex: Arc<Executor<'_>>) -> Result<()> {
    // We use this synchronous channel to block in this function, and
    // to catch a shutdown signal, where we can clean up and exit gracefully.
    let (signal, shutdown) = std::sync::mpsc::channel();
    ctrlc::set_handler(move || signal.send(()).unwrap()).unwrap();

    ex.spawn(listen_and_serve(args.rpc_listen, darkfid)).detach();

    shutdown.recv().unwrap();
    print!("\r");
    info!("Caught ^C, cleaning up and exiting...");
    // Flush dbs

    Ok(())
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
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();
    let (_, result) = Parallel::new()
        // Run four executor threads
        .each(0..4, |_| future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            future::block_on(async {
                realmain(args, darkfid, ex.clone()).await?;
                drop(signal);
                Ok::<(), darkfi::Error>(())
            })
        });

    result
}
