use std::{collections::HashMap, net::SocketAddr, str::FromStr};

use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use chrono::Utc;
use easy_parallel::Parallel;
use futures_lite::future;
use log::{debug, error, info};
use num_bigint::BigUint;
use serde_derive::Deserialize;
use serde_json::{json, Value};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use url::Url;

use darkfi::{
    async_daemonize, cli_desc,
    consensus::{
        proto::{ProtocolSync, ProtocolTx},
        task::block_sync_task,
        Timestamp, Tx, ValidatorState, ValidatorStatePtr, MAINNET_GENESIS_HASH_BYTES,
        TESTNET_GENESIS_HASH_BYTES,
    },
    crypto::{address::Address, keypair::PublicKey, token_list::DrkTokenList},
    net,
    net::P2pPtr,
    node::Client,
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
        sleep, NetworkName,
    },
    wallet::walletdb::init_wallet,
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

    #[structopt(long, default_value = "tcp://127.0.0.1:8340")]
    /// JSON-RPC listen URL
    rpc_listen: Url,

    #[structopt(long)]
    /// P2P accept address for the syncing protocol
    sync_p2p_accept: Option<SocketAddr>,

    #[structopt(long)]
    /// P2P external address for the syncing protocol
    sync_p2p_external: Option<SocketAddr>,

    #[structopt(long, default_value = "8")]
    /// Connection slots for the syncing protocol
    sync_slots: u32,

    #[structopt(long)]
    /// Connect to seed for the syncing protocol (repeatable flag)
    sync_p2p_seed: Vec<SocketAddr>,

    #[structopt(long)]
    /// Connect to peer for the syncing protocol (repeatable flag)
    sync_p2p_peer: Vec<SocketAddr>,

    #[structopt(long)]
    /// Whitelisted cashier address (repeatable flag)
    cashier_pub: Vec<String>,

    #[structopt(long)]
    /// Whitelisted faucet address (repeatable flag)
    faucet_pub: Vec<String>,

    #[structopt(long, default_value = "600")]
    /// Airdrop timeout limit in seconds
    airdrop_timeout: i64,

    #[structopt(long, default_value = "10")]
    /// Airdrop amount limit
    airdrop_limit: String, // We convert this to biguint with decode_base10

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[structopt(short)]
    /// Genesis time
    genesis_time: i64,
}

pub struct Faucetd {
    synced: Mutex<bool>, // AtomicBool is weird in Arc
    sync_p2p: P2pPtr,
    client: Arc<Client>,
    validator_state: ValidatorStatePtr,
    airdrop_timeout: i64,
    airdrop_limit: BigUint,
    airdrop_map: Arc<Mutex<HashMap<Address, i64>>>,
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
        validator_state: ValidatorStatePtr,
        sync_p2p: P2pPtr,
        timeout: i64,
        limit: BigUint,
    ) -> Result<Self> {
        let client = validator_state.read().await.client.clone();

        Ok(Self {
            synced: Mutex::new(false),
            sync_p2p,
            client,
            validator_state,
            airdrop_timeout: timeout,
            airdrop_limit: limit,
            airdrop_map: Arc::new(Mutex::new(HashMap::new())),
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

        if !(*self.synced.lock().await) {
            error!("airdrop(): Blockchain is not yet synced");
            return jsonrpc::error(InternalError, None, id).into()
        }

        let address = match Address::from_str(params[0].as_str().unwrap()) {
            Ok(v) => v,
            Err(_) => {
                error!("airdrop(): Failed parsing address from string");
                return server_error(RpcError::ParseError, id)
            }
        };

        let pubkey = match PublicKey::try_from(address) {
            Ok(v) => v,
            Err(_) => {
                error!("airdrop(): Failed parsing PublicKey from Address");
                return server_error(RpcError::ParseError, id)
            }
        };

        let amount = params[1].as_f64().unwrap().to_string();
        let amount = match decode_base10(&amount, 8, true) {
            Ok(v) => v,
            Err(_) => {
                error!("airdrop(): Failed parsing amount from string");
                return server_error(RpcError::ParseError, id)
            }
        };

        if amount > self.airdrop_limit {
            return server_error(RpcError::AmountExceedsLimit, id)
        }

        // Check if there as a previous airdrop and the timeout has passed.
        let now = Utc::now().timestamp();
        let map = self.airdrop_map.lock().await;
        if let Some(last_airdrop) = map.get(&address) {
            if now - last_airdrop <= self.airdrop_timeout {
                return server_error(RpcError::TimeLimitReached, id)
            }
        };
        drop(map);

        let token_id = self.client.tokenlist.by_net[&NetworkName::DarkFi]
            .get("DRK".to_string())
            .unwrap()
            .drk_address;

        let amnt: u64 = match amount.try_into() {
            Ok(v) => v,
            Err(e) => {
                error!("airdrop(): Failed converting biguint to u64: {}", e);
                return jsonrpc::error(InternalError, None, id).into()
            }
        };

        let tx = match self
            .client
            .build_transaction(
                pubkey,
                amnt,
                token_id,
                true,
                self.validator_state.read().await.state_machine.clone(),
            )
            .await
        {
            Ok(v) => v,
            Err(e) => {
                error!("airdrop(): Failed building transaction: {}", e);
                return jsonrpc::error(InternalError, None, id).into()
            }
        };

        // Broadcast transaction to the network.
        match self.sync_p2p.broadcast(Tx(tx.clone())).await {
            Ok(()) => {}
            Err(e) => {
                error!("airdrop(): Failed broadcasting transaction: {}", e);
                return jsonrpc::error(InternalError, None, id).into()
            }
        }

        // Add/Update this airdrop into the hashmap
        let mut map = self.airdrop_map.lock().await;
        map.insert(address, now);
        drop(map);

        let tx_hash = blake3::hash(&serialize(&tx)).to_hex().as_str().to_string();
        jsonrpc::response(json!(tx_hash), id).into()
    }
}

async fn prune_airdrop_map(map: Arc<Mutex<HashMap<Address, i64>>>, timeout: i64) {
    loop {
        sleep(timeout as u64).await;
        debug!("Pruning airdrop map");

        let now = Utc::now().timestamp();

        let mut prune = vec![];

        let im_map = map.lock().await;
        for (k, v) in im_map.iter() {
            if now - *v > timeout {
                prune.push(*k);
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
    // TODO: genesis_ts should be some hardcoded constant
    let genesis_ts = Timestamp(args.genesis_time);
    let genesis_data = match args.chain.as_str() {
        "mainnet" => *MAINNET_GENESIS_HASH_BYTES,
        "testnet" => *TESTNET_GENESIS_HASH_BYTES,
        x => {
            error!("Unsupported chain `{}`", x);
            return Err(Error::UnsupportedChain)
        }
    };

    let tokenlist = Arc::new(DrkTokenList::new(&[
        ("drk", include_bytes!("../../../contrib/token/darkfi_token_list.min.json")),
        ("btc", include_bytes!("../../../contrib/token/bitcoin_token_list.min.json")),
        ("eth", include_bytes!("../../../contrib/token/erc20_token_list.min.json")),
        ("sol", include_bytes!("../../../contrib/token/solana_token_list.min.json")),
    ])?);

    // TODO: sqldb init cleanup
    // Initialize client
    let client = Arc::new(Client::new(wallet.clone(), tokenlist).await?);

    // Parse cashier addresses
    let mut cashier_pubkeys = vec![];
    for i in args.cashier_pub {
        let addr = Address::from_str(&i)?;
        let pk = PublicKey::try_from(addr)?;
        cashier_pubkeys.push(pk);
    }

    // Parse faucet addresses
    let mut faucet_pubkeys = vec![wallet.get_default_keypair().await?.public];
    for i in args.faucet_pub {
        let addr = Address::from_str(&i)?;
        let pk = PublicKey::try_from(addr)?;
        faucet_pubkeys.push(pk);
    }

    // Initialize validator state
    let state = ValidatorState::new(
        &sled_db,
        genesis_ts,
        genesis_data,
        client,
        cashier_pubkeys,
        faucet_pubkeys,
    )
    .await?;

    // P2P network. The faucet doesn't participate in consensus, so we only
    // build the sync protocol.
    let network_settings = net::Settings {
        inbound: args.sync_p2p_accept,
        outbound_connections: args.sync_slots,
        external_addr: args.sync_p2p_external,
        peers: args.sync_p2p_peer.clone(),
        seeds: args.sync_p2p_seed.clone(),
        ..Default::default()
    };

    let sync_p2p = net::P2p::new(network_settings).await;
    let registry = sync_p2p.protocol_registry();

    info!("Registering block sync P2P protocols...");
    let _state = state.clone();
    registry
        .register(net::SESSION_ALL, move |channel, p2p| {
            let state = _state.clone();
            async move { ProtocolSync::init(channel, state, p2p, false).await.unwrap() }
        })
        .await;

    let _state = state.clone();
    registry
        .register(net::SESSION_ALL, move |channel, p2p| {
            let state = _state.clone();
            async move { ProtocolTx::init(channel, state, p2p).await.unwrap() }
        })
        .await;

    let airdrop_timeout = args.airdrop_timeout;
    let airdrop_limit = decode_base10(&args.airdrop_limit, 8, true)?;

    // Initialize program state
    let faucetd =
        Faucetd::new(state.clone(), sync_p2p.clone(), airdrop_timeout, airdrop_limit).await?;
    let faucetd = Arc::new(faucetd);

    // Task to periodically clean up the hashmap of airdrops.
    ex.spawn(prune_airdrop_map(faucetd.airdrop_map.clone(), airdrop_timeout)).detach();

    // JSON-RPC server
    info!("Starting JSON-RPC server");
    ex.spawn(listen_and_serve(args.rpc_listen, faucetd.clone())).detach();

    info!("Starting sync P2P network");
    sync_p2p.clone().start(ex.clone()).await?;
    let _ex = ex.clone();
    let _sync_p2p = sync_p2p.clone();
    ex.spawn(async move {
        if let Err(e) = _sync_p2p.run(_ex).await {
            error!("Failed starting sync P2P network: {}", e);
        }
    })
    .detach();

    match block_sync_task(sync_p2p.clone(), state.clone()).await {
        Ok(()) => *faucetd.synced.lock().await = true,
        Err(e) => error!("Failed syncing blockchain: {}", e),
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
