use std::net::SocketAddr;

use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use easy_parallel::Parallel;
use futures_lite::future;
use lazy_init::Lazy;
use log::{debug, error, info};
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
        proto::{
            ProtocolParticipant, ProtocolProposal, ProtocolSync, ProtocolSyncConsensus, ProtocolTx,
            ProtocolVote,
        },
        state::ValidatorStatePtr,
        task::{block_sync_task, proposal_task},
        util::Timestamp,
        ValidatorState, MAINNET_GENESIS_HASH_BYTES, TESTNET_GENESIS_HASH_BYTES,
    },
    crypto::{
        address::Address,
        keypair::{Keypair, PublicKey, SecretKey},
    },
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
        expand_path,
        path::get_config_path,
    },
    wallet::walletdb::{init_wallet, WalletPtr},
    Error, Result,
};

mod error;
use error::{server_error, RpcError};

const CONFIG_FILE: &str = "darkfid_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../darkfid_config.toml");

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
    /// P2P accept address for the consensus protocol
    consensus_p2p_accept: Option<SocketAddr>,

    #[structopt(long)]
    /// P2P external address for the consensus protocol
    consensus_p2p_external: Option<SocketAddr>,

    #[structopt(long, default_value = "8")]
    /// Connection slots for the consensus protocol
    consensus_slots: u32,

    #[structopt(long)]
    /// Connect to peer for the consensus protocol (repeatable flag)
    consensus_peer: Vec<SocketAddr>,

    #[structopt(long)]
    /// Connect to seed for the consensus protocol (repeatable flag)
    consensus_seed: Vec<SocketAddr>,

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
    /// Connect to peer for the syncing protocol (repeatable flag)
    sync_peer: Vec<SocketAddr>,

    #[structopt(long)]
    /// Connect to seed for the syncing protocol (repeatable flag)
    sync_seed: Vec<SocketAddr>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,

    #[structopt(short)]
    /// Genesis time
    genesis_time: i64,
}

pub struct Darkfid {
    synced: Mutex<bool>, // AtomicBool is weird in Arc
    client: Client,
    consensus_p2p: Option<P2pPtr>,
    sync_p2p: Option<P2pPtr>,
    validator_state: ValidatorStatePtr,
    state: Arc<Mutex<State>>,
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
            Some("get_slot") => return self.get_slot(req.id, params).await,
            Some(_) | None => return jsonrpc::error(MethodNotFound, None, req.id).into(),
        }
    }
}

impl Darkfid {
    pub async fn new(
        db: &sled::Db,
        wallet: WalletPtr,
        validator_state: ValidatorStatePtr,
        consensus_p2p: Option<P2pPtr>,
        sync_p2p: Option<P2pPtr>,
    ) -> Result<Self> {
        // Initialize Client
        let client = Client::new(wallet).await?;
        let tree = client.get_tree().await?;
        let merkle_roots = RootStore::new(db)?;
        let nullifiers = NullifierStore::new(db)?;

        // Initialize State
        let state = Arc::new(Mutex::new(State {
            tree,
            merkle_roots,
            nullifiers,
            cashier_pubkeys: vec![],
            faucet_pubkeys: vec![],
            mint_vk: Lazy::new(),
            burn_vk: Lazy::new(),
        }));

        Ok(Self {
            synced: Mutex::new(false),
            client,
            consensus_p2p,
            sync_p2p,
            validator_state,
            state,
        })
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

    // RPCAPI:
    // Queries the blockchain database for a block in the given slot.
    // Returns a readable block upon success.
    // --> {"jsonrpc": "2.0", "method": "get_slot", "params": [0], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    async fn get_slot(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_u64() {
            return jsonrpc::error(InvalidParams, None, id).into()
        }

        let blocks = match self
            .validator_state
            .read()
            .await
            .blockchain
            .get_blocks_by_slot(&[params[0].as_u64().unwrap()])
        {
            Ok(v) => v,
            Err(e) => {
                error!("Failed fetching block by slot: {}", e);
                return jsonrpc::error(InternalError, None, id).into()
            }
        };

        if blocks.is_empty() {
            return server_error(RpcError::UnknownSlot, id)
        }

        debug!("{:#?}", blocks[0]);
        jsonrpc::response(json!(true), id).into()
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
    // TODO: Is this ok?
    let mut rng = rand::thread_rng();
    let id: u64 = rng.gen();

    // Initialize validator state
    let state = ValidatorState::new(&sled_db, id, genesis_ts, genesis_data)?;

    let sync_p2p = {
        info!("Registering sync P2P protocols...");
        let sync_network_settings = net::Settings {
            inbound: args.sync_p2p_accept,
            outbound_connections: args.sync_slots,
            external_addr: args.sync_p2p_external,
            peers: args.sync_peer.clone(),
            seeds: args.sync_seed.clone(),
            ..Default::default()
        };

        let p2p = net::P2p::new(sync_network_settings).await;
        let registry = p2p.protocol_registry();

        let _state = state.clone();
        registry
            .register(net::SESSION_ALL, move |channel, p2p| {
                let state = _state.clone();
                async move { ProtocolSync::init(channel, state, p2p, args.consensus).await.unwrap() }
            })
            .await;

        let _state = state.clone();
        registry
            .register(net::SESSION_ALL, move |channel, p2p| {
                let state = _state.clone();
                async move { ProtocolTx::init(channel, state, p2p).await.unwrap() }
            })
            .await;

        Some(p2p)
    };

    // P2P network settings for the consensus protocol
    let consensus_p2p = {
        if !args.consensus {
            None
        } else {
            info!("Registering consensus P2P protocols...");
            let consensus_network_settings = net::Settings {
                inbound: args.consensus_p2p_accept,
                outbound_connections: args.consensus_slots,
                external_addr: args.consensus_p2p_external,
                peers: args.consensus_peer.clone(),
                seeds: args.consensus_seed.clone(),
                ..Default::default()
            };
            let p2p = net::P2p::new(consensus_network_settings).await;
            let registry = p2p.protocol_registry();

            let _state = state.clone();
            registry
                .register(net::SESSION_ALL, move |channel, p2p| {
                    let state = _state.clone();
                    async move { ProtocolParticipant::init(channel, state, p2p).await.unwrap() }
                })
                .await;

            let _state = state.clone();
            registry
                .register(net::SESSION_ALL, move |channel, p2p| {
                    let state = _state.clone();
                    async move { ProtocolProposal::init(channel, state, p2p).await.unwrap() }
                })
                .await;

            let _state = state.clone();
            let _sync_p2p = sync_p2p.clone().unwrap();
            registry
                .register(net::SESSION_ALL, move |channel, p2p| {
                    let state = _state.clone();
                    let __sync_p2p = _sync_p2p.clone();
                    async move {
                        ProtocolVote::init(channel, state, __sync_p2p, p2p)
                            .await
                            .unwrap()
                    }
                })
                .await;

            let _state = state.clone();
            registry
                .register(net::SESSION_ALL, move |channel, p2p| {
                    let state = _state.clone();
                    async move { ProtocolSyncConsensus::init(channel, state, p2p).await.unwrap() }
                })
                .await;

            Some(p2p)
        }
    };

    // Initialize program state
    let darkfid =
        Darkfid::new(&sled_db, wallet, state.clone(), consensus_p2p.clone(), sync_p2p.clone())
            .await?;
    let darkfid = Arc::new(darkfid);

    // JSON-RPC server
    info!("Starting JSON-RPC server");
    ex.spawn(listen_and_serve(args.rpc_listen, darkfid.clone())).detach();

    info!("Starting sync P2P network");
    sync_p2p.clone().unwrap().start(ex.clone()).await?;
    let _ex = ex.clone();
    let _sync_p2p = sync_p2p.clone();
    ex.spawn(async move {
        if let Err(e) = _sync_p2p.unwrap().run(_ex).await {
            error!("Failed starting sync P2P network: {}", e);
        }
    })
    .detach();

    match block_sync_task(sync_p2p.clone().unwrap(), state.clone()).await {
        Ok(()) => *darkfid.synced.lock().await = true,
        Err(e) => error!("Failed syncing blockchain: {}", e),
    }

    // Consensus protocol
    if args.consensus {
        info!("Starting consensus P2P network");
        consensus_p2p.clone().unwrap().start(ex.clone()).await?;
        let _ex = ex.clone();
        let _consensus_p2p = consensus_p2p.clone();
        ex.spawn(async move {
            if let Err(e) = _consensus_p2p.unwrap().run(_ex).await {
                error!("Failed starting consensus P2P network: {}", e);
            }
        })
        .detach();

        info!("Starting consensus protocol task");
        ex.spawn(proposal_task(consensus_p2p.unwrap(), state)).detach();
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
