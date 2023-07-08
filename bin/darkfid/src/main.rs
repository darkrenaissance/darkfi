/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{path::Path, str::FromStr};

use async_std::{
    stream::StreamExt,
    sync::{Arc, Mutex},
};
use async_trait::async_trait;
use darkfi_sdk::crypto::PublicKey;
use log::{error, info};
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use url::Url;

use darkfi::{
    async_daemonize, cli_desc,
    consensus::{
        constants::{
            MAINNET_BOOTSTRAP_TIMESTAMP, MAINNET_GENESIS_HASH_BYTES, MAINNET_GENESIS_TIMESTAMP,
            MAINNET_INITIAL_DISTRIBUTION, TESTNET_BOOTSTRAP_TIMESTAMP, TESTNET_GENESIS_HASH_BYTES,
            TESTNET_GENESIS_TIMESTAMP, TESTNET_INITIAL_DISTRIBUTION,
        },
        proto::{ProtocolProposal, ProtocolSync, ProtocolSyncConsensus, ProtocolTx},
        task::{block_sync_task, proposal_task},
        validator::ValidatorStatePtr,
        ValidatorState,
    },
    net,
    net::P2pPtr,
    rpc::{
        clock_sync::check_clock,
        jsonrpc::{
            ErrorCode::{InvalidParams, MethodNotFound},
            JsonError, JsonRequest, JsonResult,
        },
        server::{listen_and_serve, RequestHandler},
    },
    util::path::expand_path,
    wallet::{WalletDb, WalletPtr},
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

    #[structopt(long)]
    /// Enable single-node mode for local testing
    single_node: bool,

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
    /// P2P accept addresses for the consensus protocol (repeatable flag)
    consensus_p2p_accept: Vec<Url>,

    #[structopt(long)]
    /// P2P external addresses for the consensus protocol (repeatable flag)
    consensus_p2p_external: Vec<Url>,

    #[structopt(long, default_value = "8")]
    /// Connection slots for the consensus protocol
    consensus_slots: usize,

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
    /// Prefered transports of outbound connections for the consensus protocol (repeatable flag)
    consensus_p2p_transports: Vec<String>,

    #[structopt(long)]
    /// P2P accept addresses for the syncing protocol (repeatable flag)
    sync_p2p_accept: Vec<Url>,

    #[structopt(long)]
    /// P2P external addresses for the syncing protocol (repeatable flag)
    sync_p2p_external: Vec<Url>,

    #[structopt(long, default_value = "8")]
    /// Connection slots for the syncing protocol
    sync_slots: usize,

    #[structopt(long)]
    /// Connect to peer for the syncing protocol (repeatable flag)
    sync_p2p_peer: Vec<Url>,

    #[structopt(long)]
    /// Connect to seed for the syncing protocol (repeatable flag)
    sync_p2p_seed: Vec<Url>,

    #[structopt(long)]
    /// Prefered transports of outbound connections for the syncing protocol (repeatable flag)
    sync_p2p_transports: Vec<String>,

    #[structopt(long)]
    /// Enable localnet hosts
    localnet: bool,

    #[structopt(long)]
    /// Enable channel log
    channel_log: bool,

    #[structopt(long)]
    /// Whitelisted cashier public key (repeatable flag)
    cashier_pub: Vec<String>,

    #[structopt(long)]
    /// Whitelisted faucet public key (repeatable flag)
    faucet_pub: Vec<String>,

    #[structopt(long)]
    /// Verify system clock is correct
    clock_sync: bool,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

pub struct Darkfid {
    synced: Mutex<bool>, // AtomicBool is weird in Arc
    consensus_p2p: Option<P2pPtr>,
    sync_p2p: Option<P2pPtr>,
    _wallet: WalletPtr,
    validator_state: ValidatorStatePtr,
}

// JSON-RPC methods
mod rpc_blockchain;
mod rpc_misc;
mod rpc_tx;
mod rpc_wallet;

// Internal methods
//mod internal;

#[async_trait]
impl RequestHandler for Darkfid {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if !req.params.is_array() {
            return JsonError::new(InvalidParams, None, req.id).into()
        }

        let params = req.params.as_array().unwrap();

        match req.method.as_str() {
            // =====================
            // Miscellaneous methods
            // =====================
            Some("ping") => return self.misc_pong(req.id, params).await,
            Some("clock") => return self.misc_clock(req.id, params).await,
            Some("sync_dnet_switch") => return self.misc_sync_dnet_switch(req.id, params).await,
            Some("sync_dnet_info") => return self.misc_sync_dnet_info(req.id, params).await,
            Some("consensus_dnet_switch") => {
                return self.misc_consensus_dnet_switch(req.id, params).await
            }
            Some("consensus_dnet_info") => {
                return self.misc_consensus_dnet_info(req.id, params).await
            }

            // ==================
            // Blockchain methods
            // ==================
            Some("blockchain.get_slot") => return self.blockchain_get_slot(req.id, params).await,
            Some("blockchain.get_tx") => return self.blockchain_get_tx(req.id, params).await,
            Some("blockchain.last_known_slot") => {
                return self.blockchain_last_known_slot(req.id, params).await
            }
            Some("blockchain.subscribe_blocks") => {
                return self.blockchain_subscribe_blocks(req.id, params).await
            }
            Some("blockchain.subscribe_err_txs") => {
                return self.blockchain_subscribe_err_txs(req.id, params).await
            }
            Some("blockchain.lookup_zkas") => {
                return self.blockchain_lookup_zkas(req.id, params).await
            }

            // ===================
            // Transaction methods
            // ===================
            Some("tx.simulate") => return self.tx_simulate(req.id, params).await,
            Some("tx.broadcast") => return self.tx_broadcast(req.id, params).await,

            // ==============
            // Wallet methods
            // ==============
            Some("wallet.exec_sql") => return self.wallet_exec_sql(req.id, params).await,
            Some("wallet.query_row_single") => {
                return self.wallet_query_row_single(req.id, params).await
            }
            Some("wallet.query_row_multi") => {
                return self.wallet_query_row_multi(req.id, params).await
            }

            // ==============
            // Invalid method
            // ==============
            Some(_) | None => return JsonError::new(MethodNotFound, None, req.id).into(),
        }
    }
}

impl Darkfid {
    pub async fn new(
        validator_state: ValidatorStatePtr,
        consensus_p2p: Option<P2pPtr>,
        sync_p2p: Option<P2pPtr>,
        _wallet: WalletPtr,
    ) -> Self {
        Self { synced: Mutex::new(false), consensus_p2p, sync_p2p, _wallet, validator_state }
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'_>>) -> Result<()> {
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
        if (check_clock(&peers).await).is_err() {
            error!("System clock is invalid, terminating...");
            return Err(Error::InvalidClock)
        };
    }

    // Initialize or load wallet
    let wallet = WalletDb::new(Some(expand_path(&args.wallet_path)?), &args.wallet_pass).await?;

    // Initialize or open sled database
    let db_path =
        Path::new(expand_path(&args.database)?.to_str().unwrap()).join(args.chain.clone());
    let sled_db = sled::open(&db_path)?;

    // Initialize validator state
    let (bootstrap_ts, genesis_ts, genesis_data, initial_distribution) = match args.chain.as_str() {
        "mainnet" => (
            *MAINNET_BOOTSTRAP_TIMESTAMP,
            *MAINNET_GENESIS_TIMESTAMP,
            *MAINNET_GENESIS_HASH_BYTES,
            *MAINNET_INITIAL_DISTRIBUTION,
        ),
        "testnet" => (
            *TESTNET_BOOTSTRAP_TIMESTAMP,
            *TESTNET_GENESIS_TIMESTAMP,
            *TESTNET_GENESIS_HASH_BYTES,
            *TESTNET_INITIAL_DISTRIBUTION,
        ),
        x => {
            error!("Unsupported chain `{}`", x);
            return Err(Error::UnsupportedChain)
        }
    };
    // Parse faucet addresses
    let mut faucet_pubkeys = vec![];

    for i in args.cashier_pub {
        let pk = PublicKey::from_str(&i)?;
        faucet_pubkeys.push(pk);
    }

    for i in args.faucet_pub {
        let pk = PublicKey::from_str(&i)?;
        faucet_pubkeys.push(pk);
    }

    if args.single_node {
        info!("Node is configured to run in single-node mode!");
    }

    // Initialize validator state
    let state = ValidatorState::new(
        &sled_db,
        bootstrap_ts,
        genesis_ts,
        genesis_data,
        initial_distribution,
        wallet.clone(),
        faucet_pubkeys,
        args.consensus,
        args.single_node,
    )
    .await?;

    let sync_p2p = {
        info!("Registering block sync P2P protocols...");
        let sync_network_settings = net::Settings {
            inbound_addrs: args.sync_p2p_accept,
            outbound_connections: args.sync_slots,
            external_addrs: args.sync_p2p_external,
            peers: args.sync_p2p_peer.clone(),
            seeds: args.sync_p2p_seed.clone(),
            allowed_transports: args.sync_p2p_transports,
            localnet: args.localnet,
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
                inbound_addrs: args.consensus_p2p_accept,
                outbound_connections: args.consensus_slots,
                external_addrs: args.consensus_p2p_external,
                peers: args.consensus_p2p_peer.clone(),
                seeds: args.consensus_p2p_seed.clone(),
                allowed_transports: args.consensus_p2p_transports,
                localnet: args.localnet,
                ..Default::default()
            };
            let p2p = net::P2p::new(consensus_network_settings).await;
            let registry = p2p.protocol_registry();

            let _state = state.clone();
            registry
                .register(net::SESSION_ALL, move |channel, p2p| {
                    let state = _state.clone();
                    async move { ProtocolProposal::init(channel, state, p2p).await.unwrap() }
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
        Darkfid::new(state.clone(), consensus_p2p.clone(), sync_p2p.clone(), wallet.clone()).await;
    let darkfid = Arc::new(darkfid);

    // JSON-RPC server
    info!("Starting JSON-RPC server");
    let _ex = ex.clone();
    ex.spawn(listen_and_serve(args.rpc_listen, darkfid.clone(), _ex)).detach();

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

    // TODO: I think this is not necessary anymore
    //info!("Waiting for sync P2P outbound connections");
    //sync_p2p.clone().unwrap().wait_for_outbound(ex.clone()).await?;

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

        // TODO: I think this is not necessary anymore
        //info!("Waiting for consensus P2P outbound connections");
        //consensus_p2p.clone().unwrap().wait_for_outbound(ex.clone()).await?;

        info!("Starting consensus protocol task");
        let _ex = ex.clone();
        ex.spawn(proposal_task(consensus_p2p.unwrap(), sync_p2p.unwrap(), state, _ex)).detach();
    } else {
        info!("Not starting consensus P2P network");
    }

    // Signal handling for graceful termination.
    let (signals_handler, signals_task) = SignalHandler::new()?;
    signals_handler.wait_termination(signals_task).await?;
    info!("Caught termination signal, cleaning up and exiting...");

    // TODO: STOP P2P NETS

    info!("Flushing sled database...");
    let flushed_bytes = sled_db.flush_async().await?;
    info!("Flushed {} bytes", flushed_bytes);

    Ok(())
}
