use std::{collections::HashMap, net::SocketAddr};

use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use chrono::Utc;
use easy_parallel::Parallel;
use futures_lite::future;
use lazy_init::Lazy;
use log::{debug, error, info};
use num_bigint::BigUint;
use rand::Rng;
use serde_derive::Deserialize;
use serde_json::{json, Value};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use url::Url;

use darkfi::{
    async_daemonize,
    blockchain::{NullifierStore, RootStore},
    cli_desc,
    consensus2::{
        Timestamp, ValidatorState, MAINNET_GENESIS_HASH_BYTES, TESTNET_GENESIS_HASH_BYTES,
    },
    crypto::{keypair::PublicKey, types::DrkTokenId},
    net,
    net::P2pPtr,
    node::{Client, State},
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
        decode_base10, expand_path,
        path::get_config_path,
        serial::serialize,
        sleep,
    },
    wallet::walletdb::{WalletDb, WalletPtr},
    Error, Result,
};

mod error;
use error::{server_error, RpcError};

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

    #[structopt(long, default_value = "~/.config/darkfi/faucetd_wallet.db")]
    /// Path to wallet database
    wallet_path: String,

    #[structopt(long, default_value = "changeme")]
    /// Password for the wallet database
    wallet_pass: String,

    #[structopt(long, default_value = "~/.config/darkfi/faucetd_blockchain")]
    /// Path to blockchain database
    database: String,

    #[structopt(long, default_value = "tcp://127.0.0.1:5381")]
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
    /// Connect to seed (repeatable flag)
    seed: Vec<SocketAddr>,

    #[structopt(long)]
    /// Connect to peer (repeatable flag)
    peer: Vec<SocketAddr>,

    #[structopt(long, default_value = "600")]
    /// Airdrop timeout limit in seconds
    airdrop_timeout: i64,

    #[structopt(long, default_value = "10")]
    /// Airdrop amount limit
    airdrop_limit: String, // We convert this to biguint with decode_base10

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

pub struct Faucetd {
    airdrop_timeout: i64,
    airdrop_limit: BigUint,
    airdrop_map: Arc<Mutex<HashMap<String, i64>>>,
    client: Client,
    p2p: P2pPtr,
    state: Arc<Mutex<State>>,
}

#[async_trait]
impl RequestHandler for Faucetd {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if !req.params.is_array() {
            return jsonrpc::error(InvalidParams, None, req.id).into()
        }

        let params = req.params.as_array().unwrap();

        match req.method.as_str() {
            Some("airdrop") => return self.airdrop(req.id, params).await,
            Some(_) | None => return jsonrpc::error(MethodNotFound, None, req.id).into(),
        }
    }
}

impl Faucetd {
    pub async fn new(
        db: &sled::Db,
        wallet: WalletPtr,
        p2p: P2pPtr,
        timeout: i64,
        limit: BigUint,
    ) -> Result<Self> {
        // Initialize client
        let client = Client::new(wallet).await?;
        let tree = client.get_tree().await?;
        let merkle_roots = RootStore::new(db)?;
        let nullifiers = NullifierStore::new(db)?;

        let kp = client.main_keypair.lock().await.public;

        // Initialize state
        let state = Arc::new(Mutex::new(State {
            tree,
            merkle_roots,
            nullifiers,
            cashier_pubkeys: vec![],
            faucet_pubkeys: vec![kp],
            mint_vk: Lazy::new(),
            burn_vk: Lazy::new(),
        }));

        Ok(Self {
            airdrop_timeout: timeout,
            airdrop_limit: limit,
            airdrop_map: Arc::new(Mutex::new(HashMap::new())),
            client,
            p2p,
            state,
        })
    }

    // RPCAPI:
    // Processes an airdrop request and airdrops requested amount to address.
    // Returns the transaction ID upon success.
    // --> {"jsonrpc": "2.0", "method": "airdrop", "params": ["1DarkFi...", 1.42], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "txID", "id": 1}
    async fn airdrop(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 2 || !params[0].is_string() || !params[1].is_f64() {
            return jsonrpc::error(InvalidParams, None, id).into()
        }

        let pubkey = match PublicKey::from_str(params[0].as_str().unwrap()) {
            Ok(v) => v,
            Err(_) => return server_error(RpcError::ParseError, id),
        };

        let amount = match decode_base10(params[1].as_str().unwrap(), 8, true) {
            Ok(v) => v,
            Err(_) => return server_error(RpcError::ParseError, id),
        };

        if amount > self.airdrop_limit {
            return server_error(RpcError::AmountExceedsLimit, id)
        }

        let now = Utc::now().timestamp();
        let map = self.airdrop_map.lock().await;
        if let Some(last_airdrop) = map.get(params[1].as_str().unwrap()) {
            if now - last_airdrop <= self.airdrop_timeout {
                return server_error(RpcError::TimeLimitReached, id)
            }
        };
        drop(map);

        // TODO: Token ID decision
        // TODO: Rename this function to tx build
        // let tx = match self
        // .client
        // .send(pubkey, amount, DrkTokenId::from(1), true, self.state.clone())
        // .await
        // {
        // Ok(v) => v,
        // Err(e) => {
        // error!("airdrop(): Failed building transaction: {}", e);
        // return jsonrpc::error(InternalError, None, id).into()
        // }
        // };

        // let tx_hash = blake3::hash(&serialize(&tx)).to_hex().as_str().to_string();
        let tx_hash = "f00b4r";

        // TODO: p2p tx broadcast

        let mut map = self.airdrop_map.lock().await;
        map.insert(params[1].as_str().unwrap().to_string(), now);
        drop(map);

        jsonrpc::response(json!(tx_hash), id).into()
    }
}

async fn prune_airdrop_map(map: Arc<Mutex<HashMap<String, i64>>>, timeout: i64) {
    loop {
        sleep(timeout as u64).await;
        debug!("Pruning airdrop map");

        let now = Utc::now().timestamp();

        let mut prune = vec![];

        let im_map = map.lock().await;
        for (k, v) in im_map.iter() {
            if now - *v > timeout {
                prune.push(k.clone());
            }
        }
        drop(im_map);

        let mut mut_map = map.lock().await;
        for i in prune {
            mut_map.remove(&i);
        }
        drop(mut_map);
    }
}

async fn init_wallet(wallet_path: &str, wallet_pass: &str) -> Result<WalletPtr> {
    let expanded = expand_path(wallet_path)?;
    let wallet_path = format!("sqlite://{}", expanded.to_str().unwrap());
    let wallet = WalletDb::new(&wallet_path, wallet_pass).await?;
    Ok(wallet)
}

async_daemonize!(realmain);
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
        peers: args.peer.clone(),
        seeds: args.seed.clone(),
        ..Default::default()
    };

    let p2p = net::P2p::new(network_settings).await;
    let registry = p2p.protocol_registry();

    // TODO: Register replicator + tx broadcast protocols

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

    let airdrop_timeout = args.airdrop_timeout;
    let airdrop_limit = decode_base10(&args.airdrop_limit, 8, true)?;

    // Initialize program state
    let faucetd = Faucetd::new(&sled_db, wallet, p2p, airdrop_timeout, airdrop_limit).await?;
    let faucetd = Arc::new(faucetd);

    // Task to periodically clean up the hashmap of airdrops.
    ex.spawn(prune_airdrop_map(faucetd.airdrop_map.clone(), airdrop_timeout)).detach();

    // JSON-RPC server
    info!("Starting JSON-RPC server");
    ex.spawn(listen_and_serve(args.rpc_listen, faucetd)).detach();

    // Wait for SIGINT
    shutdown.recv().await?;
    print!("\r");
    info!("Caught termination signal, cleaning up and exiting...");

    info!("Flushing database...");
    let flushed_bytes = sled_db.flush_async().await?;
    info!("Flushed {} bytes", flushed_bytes);

    Ok(())
}
