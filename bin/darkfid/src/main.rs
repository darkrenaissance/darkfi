use std::str::FromStr;

use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use futures_lite::future;
use log::{debug, error, info};
use serde_derive::Deserialize;
use structopt::StructOpt;
use structopt_toml::StructOptToml;
use url::Url;

use darkfi::{
    async_daemonize, cli_desc,
    consensus::{
        proto::{
            ProtocolParticipant, ProtocolProposal, ProtocolSync, ProtocolSyncConsensus, ProtocolTx,
            ProtocolVote,
        },
        state::ValidatorStatePtr,
        task::{block_sync_task, proposal_task},
        ValidatorState, MAINNET_GENESIS_HASH_BYTES, MAINNET_GENESIS_TIMESTAMP,
        TESTNET_GENESIS_HASH_BYTES, TESTNET_GENESIS_TIMESTAMP,
    },
    crypto::{address::Address, keypair::PublicKey, token_list::DrkTokenList},
    net,
    net::P2pPtr,
    node::Client,
    rpc::{
        jsonrpc::{
            ErrorCode::{InvalidParams, MethodNotFound},
            JsonError, JsonRequest, JsonResult,
        },
        server::{listen_and_serve, RequestHandler},
    },
    util::{
        cli::{get_log_config, get_log_level, spawn_config},
        expand_path,
        path::get_config_path,
        time::check_clock,
    },
    wallet::walletdb::init_wallet,
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

    #[structopt(long, default_value = "tcp://127.0.0.1:8340")]
    /// JSON-RPC listen URL
    rpc_listen: Url,

    #[structopt(long)]
    /// P2P accept address for the consensus protocol
    consensus_p2p_accept: Option<Url>,

    #[structopt(long)]
    /// P2P external address for the consensus protocol
    consensus_p2p_external: Option<Url>,

    #[structopt(long, default_value = "8")]
    /// Connection slots for the consensus protocol
    consensus_slots: u32,

    #[structopt(long)]
    /// Connect to peer for the consensus protocol (repeatable flag)
    consensus_p2p_peer: Vec<Url>,

    #[structopt(long)]
    /// Peers JSON-RPC listen URL for clock synchronization (repeatable flag)
    consensus_peer_rpc: Vec<Url>,

    #[structopt(long)]
    /// Connect to seed for the consensus protocol (repeatable flag)
    consensus_p2p_seed: Vec<Url>,

    #[structopt(long)]
    /// Seed nodes JSON-RPC listen URL for clock synchronization (repeatable flag)
    consensus_seed_rpc: Vec<Url>,

    #[structopt(long)]
    /// P2P accept address for the syncing protocol
    sync_p2p_accept: Option<Url>,

    #[structopt(long)]
    /// P2P external address for the syncing protocol
    sync_p2p_external: Option<Url>,

    #[structopt(long, default_value = "8")]
    /// Connection slots for the syncing protocol
    sync_slots: u32,

    #[structopt(long)]
    /// Connect to peer for the syncing protocol (repeatable flag)
    sync_p2p_peer: Vec<Url>,

    #[structopt(long)]
    /// Connect to seed for the syncing protocol (repeatable flag)
    sync_p2p_seed: Vec<Url>,

    #[structopt(long)]
    /// Whitelisted cashier address (repeatable flag)
    cashier_pub: Vec<String>,

    #[structopt(long)]
    /// Whitelisted faucet address (repeatable flag)
    faucet_pub: Vec<String>,

    #[structopt(long)]
    /// Verify system clock is correct
    clock_sync: bool,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

pub struct Darkfid {
    synced: Mutex<bool>, // AtomicBool is weird in Arc
    _consensus_p2p: Option<P2pPtr>,
    sync_p2p: Option<P2pPtr>,
    client: Arc<Client>,
    validator_state: ValidatorStatePtr,
}

// JSON-RPC methods
mod rpc_blockchain;
mod rpc_misc;
mod rpc_tx;
mod rpc_wallet;

#[async_trait]
impl RequestHandler for Darkfid {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if !req.params.is_array() {
            return JsonError::new(InvalidParams, None, req.id).into()
        }

        let params = req.params.as_array().unwrap();

        match req.method.as_str() {
            Some("ping") => return self.pong(req.id, params).await,
            Some("clock") => return self.clock(req.id, params).await,
            Some("blockchain.get_slot") => return self.get_slot(req.id, params).await,
            Some("blockchain.merkle_roots") => return self.merkle_roots(req.id, params).await,
            Some("tx.transfer") => return self.transfer(req.id, params).await,
            Some("wallet.keygen") => return self.keygen(req.id, params).await,
            Some("wallet.get_key") => return self.get_key(req.id, params).await,
            Some("wallet.export_keypair") => return self.export_keypair(req.id, params).await,
            Some("wallet.import_keypair") => return self.import_keypair(req.id, params).await,
            Some("wallet.set_default_address") => {
                return self.set_default_address(req.id, params).await
            }
            Some("wallet.get_balances") => return self.get_balances(req.id, params).await,
            Some(_) | None => return JsonError::new(MethodNotFound, None, req.id).into(),
        }
    }
}

impl Darkfid {
    pub async fn new(
        validator_state: ValidatorStatePtr,
        consensus_p2p: Option<P2pPtr>,
        sync_p2p: Option<P2pPtr>,
    ) -> Result<Self> {
        debug!("Waiting for validator state lock");
        let client = validator_state.read().await.client.clone();
        debug!("Released validator state lock");

        Ok(Self {
            synced: Mutex::new(false),
            _consensus_p2p: consensus_p2p,
            sync_p2p,
            client,
            validator_state,
        })
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<Executor<'_>>) -> Result<()> {
    if args.consensus && args.clock_sync {
        // We verify that if peer/seed nodes are configured, their rpc config also exists
        if ((!args.consensus_p2p_peer.is_empty() && args.consensus_peer_rpc.is_empty()) ||
            (args.consensus_p2p_peer.is_empty() && !args.consensus_peer_rpc.is_empty())) ||
            ((!args.consensus_p2p_seed.is_empty() && args.consensus_seed_rpc.is_empty()) ||
                (args.consensus_p2p_seed.is_empty() && !args.consensus_seed_rpc.is_empty()))
        {
            error!(
                "Consensus peer/seed nodes misconfigured: both p2p and rpc urls must be present"
            );
            return Err(Error::ConfigInvalid)
        }
        // We verify that the system clock is valid before initializing
        let peers = [&args.consensus_peer_rpc[..], &args.consensus_seed_rpc[..]].concat();
        if (check_clock(peers).await).is_err() {
            error!("System clock is invalid, terminating...");
            return Err(Error::InvalidClock)
        };
    }

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
    let (genesis_ts, genesis_data) = match args.chain.as_str() {
        "mainnet" => (*MAINNET_GENESIS_TIMESTAMP, *MAINNET_GENESIS_HASH_BYTES),
        "testnet" => (*TESTNET_GENESIS_TIMESTAMP, *TESTNET_GENESIS_HASH_BYTES),
        x => {
            error!("Unsupported chain `{}`", x);
            return Err(Error::UnsupportedChain)
        }
    };

    debug!("Parsing token lists...");
    let tokenlist = Arc::new(DrkTokenList::new(&[
        ("drk", include_bytes!("../../../contrib/token/darkfi_token_list.min.json")),
        ("btc", include_bytes!("../../../contrib/token/bitcoin_token_list.min.json")),
        ("eth", include_bytes!("../../../contrib/token/erc20_token_list.min.json")),
        ("sol", include_bytes!("../../../contrib/token/solana_token_list.min.json")),
    ])?);
    debug!("Finished parsing token lists");

    // TODO: sqldb init cleanup
    // Initialize Client
    let client = Arc::new(Client::new(wallet, tokenlist).await?);

    // Parse cashier addresses
    let mut cashier_pubkeys = vec![];
    for i in args.cashier_pub {
        let addr = Address::from_str(&i)?;
        let pk = PublicKey::try_from(addr)?;
        cashier_pubkeys.push(pk);
    }

    // Parse fauced addresses
    let mut faucet_pubkeys = vec![];
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

    let sync_p2p = {
        info!("Registering block sync P2P protocols...");
        let sync_network_settings = net::Settings {
            inbound: args.sync_p2p_accept,
            outbound_connections: args.sync_slots,
            external_addr: args.sync_p2p_external,
            peers: args.sync_p2p_peer.clone(),
            seeds: args.sync_p2p_seed.clone(),
            ..Default::default()
        };

        let p2p = net::P2p::new(sync_network_settings).await;
        let registry = p2p.protocol_registry();

        let _state = state.clone();
        registry
            .register(net::SESSION_ALL, move |channel, p2p| {
                let state = _state.clone();
                async move {
                    ProtocolSync::init(channel, state, p2p, args.consensus)
                        .await
                        .unwrap()
                }
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
                peers: args.consensus_p2p_peer.clone(),
                seeds: args.consensus_p2p_seed.clone(),
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
    let darkfid = Darkfid::new(state.clone(), consensus_p2p.clone(), sync_p2p.clone()).await?;
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
    if args.consensus && *darkfid.synced.lock().await {
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
        ex.spawn(proposal_task(consensus_p2p.unwrap(), sync_p2p.unwrap(), state)).detach();
    } else {
        info!("Not starting consensus P2P network");
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
