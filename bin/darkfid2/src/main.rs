use std::net::SocketAddr;

use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use easy_parallel::Parallel;
use futures_lite::future;
use log::{error, info};
use rand::Rng;
use serde_derive::Deserialize;
use serde_json::{json, Value};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use url::Url;

use darkfi::{
    cli_desc,
    consensus2::{
        state::ValidatorStatePtr, util::Timestamp, Tx, ValidatorState, MAINNET_GENESIS_HASH_BYTES,
        TESTNET_GENESIS_HASH_BYTES,
    },
    crypto::{
        address::Address,
        keypair::{Keypair, PublicKey, SecretKey},
    },
    net,
    net::P2pPtr,
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
    Error, Result,
};

mod client;
use client::Client;

mod error;
use error::{server_error, RpcError};

mod protocol;
use protocol::{ProtocolParticipant, ProtocolProposal, ProtocolTx, ProtocolVote};

mod consensus;
use consensus::proposal_task;

const CONFIG_FILE: &str = "darkfid_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../darkfid_config.toml");

// TODO: Flag to participate in consensus
#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "darkfid", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(long, default_value = "testnet")]
    /// Chain to use (testnet, mainnet)
    chain: String,

    #[structopt(long)]
    /// Participate in consensus
    consensus: bool,

    #[structopt(long, default_value = "~/.config/darkfi/darkfid_wallet.db")]
    /// Path to wallet database
    wallet_path: String,

    #[structopt(long, default_value = "changeme")]
    /// Password for the wallet database
    wallet_pass: String,

    #[structopt(long, default_value = "~/.config/darkfi/darkfid_blockchain")]
    /// Path to blockchain database
    database: String,

    #[structopt(long, default_value = "tcp://127.0.0.1:5397")]
    /// JSON-RPC listen URL
    rpc_listen: Url,

    #[structopt(long)]
    /// P2P accept address
    p2p_accept: Option<SocketAddr>,

    #[structopt(long)]
    /// P2P external address
    p2p_external: Option<SocketAddr>,

    #[structopt(long, default_value = "8")]
    /// Connection slots
    slots: u32,

    #[structopt(long)]
    /// Connect to peer (repeatable flag)
    connect: Vec<SocketAddr>,

    #[structopt(long)]
    /// Connect to seed (repeatable flag)
    seed: Vec<SocketAddr>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

pub struct Darkfid {
    client: Client,
    state: ValidatorStatePtr,
    p2p: P2pPtr,
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
            Some("tx") => return self.receive_tx(req.id, params).await,
            Some(_) | None => return jsonrpc::error(MethodNotFound, None, req.id).into(),
        }
    }
}

impl Darkfid {
    pub async fn new(wallet: WalletPtr, state: ValidatorStatePtr, p2p: P2pPtr) -> Result<Self> {
        let client = Client::new(wallet).await?;
        Ok(Self { client, state, p2p, synced: Mutex::new(false) })
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
                server_error(RpcError::Keygen, id)
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
                return server_error(RpcError::Nan, id)
            }

            if i.as_i64() == Some(-1) {
                fetch_all = true;
                break
            }

            if i.as_i64() < Some(-1) {
                return server_error(RpcError::LessThanNegOne, id)
            }
        }

        let keypairs = match self.client.get_keypairs().await {
            Ok(v) => v,
            Err(e) => {
                error!("Failed fetching keypairs: {}", e);
                return server_error(RpcError::KeypairFetch, id)
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
                return server_error(RpcError::KeypairFetch, id)
            }
        };

        if let Some(kp) = keypairs.get(params[0].as_u64().unwrap() as usize) {
            return jsonrpc::response(json!(kp.secret.to_bytes()), id).into()
        }

        server_error(RpcError::KeypairNotFound, id)
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
                return server_error(RpcError::InvalidKeypair, id)
            }
        };

        let secret = match SecretKey::from_bytes(bytes) {
            Ok(v) => v,
            Err(e) => {
                error!("Failed parsing secret key from string: {}", e);
                return server_error(RpcError::InvalidKeypair, id)
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
                return server_error(RpcError::KeypairFetch, id)
            }
        };

        if keypairs.len() as u64 != idx - 1 {
            return server_error(RpcError::KeypairNotFound, id)
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

    async fn receive_tx(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_string() {
            return jsonrpc::error(InvalidParams, None, id).into()
        }

        let payload = String::from(params[0].as_str().unwrap());
        let tx = Tx { payload };

        self.state.write().await.append_tx(tx.clone());

        let result = self.p2p.broadcast(tx).await;
        match result {
            Ok(()) => jsonrpc::response(json!(true), id).into(),
            Err(_) => jsonrpc::error(InternalError, None, id).into(),
        }
    }
}

async fn init_wallet(wallet_path: &str, wallet_pass: &str) -> Result<WalletPtr> {
    let expanded = expand_path(wallet_path)?;
    let wallet_path = format!("sqlite://{}", expanded.to_str().unwrap());
    let wallet = WalletDb::new(&wallet_path, wallet_pass).await?;
    Ok(wallet)
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

    // Initialize or load wallet
    let wallet = init_wallet(&args.wallet_path, &args.wallet_pass).await?;

    // Initialize or open sled database
    let db_path = format!("{}/{}", expand_path(&args.database)?.to_str().unwrap(), args.chain);
    let sled_db = sled::open(&db_path)?;

    // Initialize validator state
    // TODO: genesis_ts should be some hardcoded constant
    let genesis_ts = Timestamp(1650103269);
    let genesis_data = match args.chain.as_str() {
        "mainnet" => *MAINNET_GENESIS_HASH_BYTES,
        "testnet" => *TESTNET_GENESIS_HASH_BYTES,
        x => {
            error!("Unsupported chain `{}`", x);
            return Err(Error::UnsupportedChain)
        }
    };
    // TODO: Is this ok?
    let mut rng = rand::thread_rng();
    let id: u64 = rng.gen();
    let state = ValidatorState::new(&sled_db, id, genesis_ts, genesis_data)?;

    // P2P network
    let network_settings = net::Settings {
        inbound: args.p2p_accept,
        outbound_connections: args.slots,
        external_addr: args.p2p_external,
        peers: args.connect.clone(),
        seeds: args.seed.clone(),
        ..Default::default()
    };

    let p2p = net::P2p::new(network_settings).await;

    // Activate these protocols only if we're participating in consensus.
    if args.consensus {
        info!("Registering consensus P2P protocols...");
        let registry = p2p.protocol_registry();

        let _state = state.clone();
        registry
            .register(!net::SESSION_SEED, move |channel, p2p| {
                let state = _state.clone();
                async move { ProtocolTx::init(channel, state, p2p).await.unwrap() }
            })
            .await;

        let _state = state.clone();
        registry
            .register(!net::SESSION_SEED, move |channel, p2p| {
                let state = _state.clone();
                async move { ProtocolVote::init(channel, state, p2p).await.unwrap() }
            })
            .await;

        let _state = state.clone();
        registry
            .register(!net::SESSION_SEED, move |channel, p2p| {
                let state = _state.clone();
                async move { ProtocolProposal::init(channel, state, p2p).await.unwrap() }
            })
            .await;

        let _state = state.clone();
        registry
            .register(!net::SESSION_SEED, move |channel, p2p| {
                let state = _state.clone();
                async move { ProtocolParticipant::init(channel, state, p2p).await.unwrap() }
            })
            .await;
    }

    info!("Starting P2P networking");
    p2p.clone().start(ex.clone()).await?;
    let _ex = ex.clone();
    let _p2p = p2p.clone();
    ex.spawn(async move {
        if let Err(e) = _p2p.run(_ex).await {
            error!("P2P run failed: {}", e);
        }
    })
    .detach();

    // Initialize program state
    let darkfid = Darkfid::new(wallet, state.clone(), p2p.clone()).await?;
    let darkfid = Arc::new(darkfid);

    // JSON-RPC server
    info!("Starting JSON-RPC server");
    ex.spawn(listen_and_serve(args.rpc_listen, darkfid)).detach();

    // Consensus protocol
    if args.consensus {
        info!("Starting consensus protocol task");
        ex.spawn(proposal_task(p2p, state)).detach();
    }

    // Wait for SIGINT
    shutdown.recv().await?;
    print!("\r");
    info!("Caught termination signal, cleaning up and exiting...");

    info!("Flushing database...");
    let flushed_bytes = sled_db.flush_async().await?;
    info!("Flushed {} bytes", flushed_bytes);

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
