/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use std::{collections::HashMap, str::FromStr};

use async_std::sync::{Arc, Mutex, RwLock};
use async_trait::async_trait;
use chrono::Utc;
use darkfi::{
    tx::Transaction,
    zk::{proof::ProvingKey, vm::ZkCircuit, vm_stack::empty_witnesses},
    zkas::ZkBinary,
};
use darkfi_money_contract::{
    client::{
        build_transfer_tx, MONEY_KEYS_COL_IS_DEFAULT, MONEY_KEYS_COL_PUBLIC, MONEY_KEYS_COL_SECRET,
        MONEY_KEYS_TABLE, MONEY_TREE_COL_TREE, MONEY_TREE_TABLE,
    },
    MoneyFunction, ZKAS_BURN_NS, ZKAS_MINT_NS,
};
use darkfi_sdk::{
    crypto::{constants::MERKLE_DEPTH, ContractId, Keypair, MerkleNode, PublicKey, TokenId},
    db::ZKAS_DB_NAME,
    incrementalmerkletree::bridgetree::BridgeTree,
    pasta::{group::ff::PrimeField, pallas},
    tx::ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable};
use log::{debug, error, info};
use rand::rngs::OsRng;
use serde_json::{json, Value};
use sqlx::Row;
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};
use url::Url;

use darkfi::{
    async_daemonize, cli_desc,
    consensus::{
        constants::{
            MAINNET_GENESIS_HASH_BYTES, MAINNET_GENESIS_TIMESTAMP, TESTNET_GENESIS_HASH_BYTES,
            TESTNET_GENESIS_TIMESTAMP,
        },
        proto::{ProtocolSync, ProtocolTx},
        task::block_sync_task,
        ValidatorState, ValidatorStatePtr,
    },
    net,
    net::P2pPtr,
    rpc::{
        jsonrpc::{
            ErrorCode::{InternalError, InvalidParams, MethodNotFound},
            JsonError, JsonRequest, JsonResponse, JsonResult,
        },
        server::{listen_and_serve, RequestHandler},
    },
    util::{async_util::sleep, parse::decode_base10, path::expand_path},
    wallet::{walletdb::init_wallet, WalletPtr},
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

    #[structopt(long, default_value = "tcp://127.0.0.1:9340")]
    /// JSON-RPC listen URL
    rpc_listen: Url,

    #[structopt(long)]
    /// P2P accept addresses for the syncing protocol
    sync_p2p_accept: Vec<Url>,

    #[structopt(long)]
    /// P2P external addresses for the syncing protocol
    sync_p2p_external: Vec<Url>,

    #[structopt(long, default_value = "8")]
    /// Connection slots for the syncing protocol
    sync_slots: u32,

    #[structopt(long)]
    /// Connect to seed for the syncing protocol (repeatable flag)
    sync_p2p_seed: Vec<Url>,

    #[structopt(long)]
    /// Connect to peer for the syncing protocol (repeatable flag)
    sync_p2p_peer: Vec<Url>,

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
    airdrop_limit: String, // We convert this to u64 with decode_base10

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

type ProvingKeyMap = Arc<RwLock<HashMap<[u8; 32], Vec<(String, ProvingKey, ZkBinary)>>>>;

pub struct Faucetd {
    synced: Mutex<bool>, // AtomicBool is weird in Arc
    sync_p2p: P2pPtr,
    validator_state: ValidatorStatePtr,
    keypair: Keypair,
    _wallet: WalletPtr,
    merkle_tree: BridgeTree<MerkleNode, MERKLE_DEPTH>,
    airdrop_timeout: i64,
    airdrop_limit: u64,
    airdrop_map: Arc<Mutex<HashMap<[u8; 32], i64>>>,
    proving_keys: ProvingKeyMap,
}

#[async_trait]
impl RequestHandler for Faucetd {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if !req.params.is_array() {
            return JsonError::new(InvalidParams, None, req.id).into()
        }

        let params = req.params.as_array().unwrap();

        match req.method.as_str() {
            Some("airdrop") => return self.airdrop(req.id, params).await,
            Some(_) | None => return JsonError::new(MethodNotFound, None, req.id).into(),
        }
    }
}

impl Faucetd {
    pub async fn new(
        validator_state: ValidatorStatePtr,
        sync_p2p: P2pPtr,
        wallet: WalletPtr,
        timeout: i64,
        limit: u64,
    ) -> Result<Self> {
        // Here we initialize the wallet for the money contract.
        let merkle_tree = Self::initialize_wallet(wallet.clone()).await?;

        // This is kinda bad, but whatever. The hashmaps hold proving keys for
        // the money contract. We keep it under RwLock in case we want to add
        // other proving keys to it later.
        let proving_keys = Arc::new(RwLock::new(HashMap::new()));

        // For now we'll create the keys for the money contract
        // FIXME: This shouldn't be hardcoded (see consensus/state.rs)
        let cid = ContractId::from(pallas::Base::from(u64::MAX - 420));

        // Do a lookup for the money contract's zkas database and fetch the circuits.
        let blockchain = { validator_state.read().await.blockchain.clone() };
        let db_handle = blockchain.contracts.lookup(&blockchain.sled_db, &cid, ZKAS_DB_NAME)?;

        let Some(mint_zkbin) = db_handle.get(&serialize(&ZKAS_MINT_NS))? else {
            error!("{} zkas bincode not found in sled database", ZKAS_MINT_NS);
            return Err(Error::ZkasBincodeNotFound);
        };
        let Some(burn_zkbin) = db_handle.get(&serialize(&ZKAS_BURN_NS))? else {
            error!("{} zkas bincode not found in sled database", ZKAS_BURN_NS);
            return Err(Error::ZkasBincodeNotFound);
        };

        let mint_zkbin = ZkBinary::decode(&mint_zkbin)?;
        let burn_zkbin = ZkBinary::decode(&burn_zkbin)?;

        let k = 13;
        let mint_circuit = ZkCircuit::new(empty_witnesses(&mint_zkbin), mint_zkbin.clone());
        let burn_circuit = ZkCircuit::new(empty_witnesses(&burn_zkbin), burn_zkbin.clone());

        info!("Creating mint circuit proving key");
        let mint_provingkey = ProvingKey::build(k, &mint_circuit);
        info!("Creating burn circuit proving key");
        let burn_provingkey = ProvingKey::build(k, &burn_circuit);

        {
            let provingkeys = vec![
                (ZKAS_MINT_NS.to_string(), mint_provingkey, mint_zkbin),
                (ZKAS_BURN_NS.to_string(), burn_provingkey, burn_zkbin),
            ];

            let mut proving_keys_w = proving_keys.write().await;
            proving_keys_w.insert(cid.inner().to_repr(), provingkeys);
        }

        // Get or create an initial keypair for signing transactions
        let keypair = Self::initialize_keypair(wallet.clone()).await?;
        info!("Faucet pubkey: {}", keypair.public);

        let faucetd = Self {
            synced: Mutex::new(false),
            sync_p2p,
            validator_state,
            keypair,
            _wallet: wallet,
            merkle_tree,
            airdrop_timeout: timeout,
            airdrop_limit: limit,
            airdrop_map: Arc::new(Mutex::new(HashMap::new())),
            proving_keys,
        };

        Ok(faucetd)
    }

    async fn initialize_wallet(wallet: WalletPtr) -> Result<BridgeTree<MerkleNode, MERKLE_DEPTH>> {
        // Perform wallet initialization for the money contract
        let wallet_schema = include_str!("../../../src/contract/money/wallet.sql");

        // Get a wallet connection
        info!("Acquiring wallet connection");
        let mut conn = wallet.conn.acquire().await?;

        info!("Initializing wallet schema");
        sqlx::query(wallet_schema).execute(&mut conn).await?;

        let query = format!("SELECT * FROM {}", MONEY_TREE_COL_TREE);
        let merkle_tree = match sqlx::query(&query).fetch_one(&mut conn).await {
            Ok(t) => {
                info!("Merkle tree already exists");
                deserialize(t.get(MONEY_TREE_COL_TREE))?
            }
            Err(_) => {
                let tree = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(100);
                let tree_bytes = serialize(&tree);
                let query = format!(
                    "DELETE FROM {}; INSERT INTO {} ({}) VALUES (?1)",
                    MONEY_TREE_TABLE, MONEY_TREE_TABLE, MONEY_TREE_COL_TREE
                );
                sqlx::query(&query).bind(tree_bytes).execute(&mut conn).await?;
                info!("Successfully initialized Merkle tree");
                tree
            }
        };

        Ok(merkle_tree)
    }

    async fn initialize_keypair(wallet: WalletPtr) -> Result<Keypair> {
        let mut conn = wallet.conn.acquire().await?;

        let query = format!(
            "SELECT {}, {} FROM {};",
            MONEY_KEYS_COL_PUBLIC, MONEY_KEYS_COL_SECRET, MONEY_KEYS_TABLE
        );
        let keypair = match sqlx::query(&query).fetch_one(&mut conn).await {
            Ok(row) => {
                let public = deserialize(row.get(MONEY_KEYS_COL_PUBLIC))?;
                let secret = deserialize(row.get(MONEY_KEYS_COL_SECRET))?;
                Keypair { public, secret }
            }
            Err(_) => {
                let keypair = Keypair::random(&mut OsRng);
                let is_default = 0;
                let public_bytes = serialize(&keypair.public);
                let secret_bytes = serialize(&keypair.secret);
                let query = format!(
                    "INSERT INTO {} ({}, {}, {}) VALUES (?1, ?2, ?3)",
                    MONEY_KEYS_TABLE,
                    MONEY_KEYS_COL_IS_DEFAULT,
                    MONEY_KEYS_COL_PUBLIC,
                    MONEY_KEYS_COL_SECRET
                );
                sqlx::query(&query)
                    .bind(is_default)
                    .bind(public_bytes)
                    .bind(secret_bytes)
                    .execute(&mut conn)
                    .await?;

                info!("Wrote keypair to wallet");
                keypair
            }
        };

        Ok(keypair)
    }

    // RPCAPI:
    // Processes an airdrop request and airdrops requested token and amount to address.
    // Returns the transaction ID upon success.
    // Params:
    // 0: base58 encoded address of the recipient
    // 1: Amount to airdrop in form of f64
    // 2: base58 encoded token ID to airdrop
    //
    // --> {"jsonrpc": "2.0", "method": "airdrop", "params": ["1DarkFi...", 1.42, "1F00b4r..."], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "txID", "id": 1}
    async fn airdrop(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 3 ||
            !params[0].is_string() ||
            !params[1].is_f64() ||
            !params[2].is_string()
        {
            return JsonError::new(InvalidParams, None, id).into()
        }

        if !(*self.synced.lock().await) {
            error!("airdrop(): Blockchain is not yet synced");
            return JsonError::new(InternalError, None, id).into()
        }

        let pubkey = match PublicKey::from_str(params[0].as_str().unwrap()) {
            Ok(v) => v,
            Err(e) => {
                error!("airdrop(): Failed parsing PublicKey from String: {}", e);
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

        // Here we allow the faucet to mint arbitrary token IDs.
        // TODO: Revert this to native token when we have contracts for minting tokens.
        let token_id = match TokenId::try_from(params[2].as_str().unwrap()) {
            Ok(v) => v,
            Err(e) => {
                error!("airdrop(): Failed parsing TokenID from string: {}", e);
                return server_error(RpcError::ParseError, id)
            }
        };

        // Check if there as a previous airdrop and the timeout has passed.
        let now = Utc::now().timestamp();
        let map = self.airdrop_map.lock().await;
        if let Some(last_airdrop) = map.get(&pubkey.to_bytes()) {
            if now - last_airdrop <= self.airdrop_timeout {
                return server_error(RpcError::TimeLimitReached, id)
            }
        };
        drop(map);

        // FIXME: This hardcoded shit (see consensus/state.rs)
        let cid = ContractId::from(pallas::Base::from(u64::MAX - 420));

        let (mint_zkbin, mint_pk, burn_zkbin, burn_pk) = {
            let proving_keys_r = self.proving_keys.read().await;
            let Some(arr) = proving_keys_r.get(&cid.to_bytes()) else {
                error!("Contract ID {} not found in proving keys hashmap", cid);
                return server_error(RpcError::InternalError, id)
            };

            let Some(mint_data) = arr.iter().find(|x| x.0 == ZKAS_MINT_NS) else {
                error!("{} proof data not found in vector", ZKAS_MINT_NS);
                return server_error(RpcError::InternalError, id)
            };

            let Some(burn_data) = arr.iter().find(|x| x.0 == ZKAS_BURN_NS) else {
                error!("{} prof data not found in vector", ZKAS_BURN_NS);
                return server_error(RpcError::InternalError, id)
            };

            (mint_data.2.clone(), mint_data.1.clone(), burn_data.2.clone(), burn_data.1.clone())
        };

        // Create money contract params and proofs
        let (params, proofs, secret_keys, _spent_coins) = match build_transfer_tx(
            &self.keypair,
            &pubkey,
            amount,
            token_id,
            &[], // <-- The faucet doesn't really have to pass OwnCoins I think
            &self.merkle_tree,
            &mint_zkbin,
            &mint_pk,
            &burn_zkbin,
            &burn_pk,
            true,
        ) {
            Ok(v) => v,
            Err(e) => {
                error!("Failed to build transfer tx params: {}", e);
                return server_error(RpcError::InternalError, id)
            }
        };

        // Build transaction
        let mut data = vec![MoneyFunction::Transfer as u8];
        params.encode(&mut data).unwrap();
        let calls = vec![ContractCall { contract_id: cid, data }];
        let proofs = vec![proofs];
        let mut tx = Transaction { calls, proofs, signatures: vec![] };
        let sigs = tx.create_sigs(&mut OsRng, &secret_keys).unwrap();
        tx.signatures = vec![sigs];

        // Safety check to see if the transaction is actually valid.
        if let Err(e) =
            self.validator_state.read().await.verify_transactions(&[tx.clone()], false).await
        {
            error!("airdrop(): Failed to verify transaction before broadcasting: {}", e);
            return JsonError::new(InternalError, None, id).into()
        }

        // Broadcast transaction to the network.
        if let Err(e) = self.sync_p2p.broadcast(tx.clone()).await {
            error!("airdrop(): Failed broadcasting transaction: {}", e);
            return JsonError::new(InternalError, None, id).into()
        };

        // Add/Update this airdrop into the hashmap
        let mut map = self.airdrop_map.lock().await;
        map.insert(pubkey.to_bytes(), now);
        drop(map);

        let tx_hash = blake3::hash(&serialize(&tx)).to_hex().as_str().to_string();
        JsonResponse::new(json!(tx_hash), id).into()
    }
}

async fn prune_airdrop_map(map: Arc<Mutex<HashMap<[u8; 32], i64>>>, timeout: i64) {
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
async fn realmain(args: Args, ex: Arc<smol::Executor<'_>>) -> Result<()> {
    // We use this handler to block this function after detaching all
    // tasks, and to catch a shutdown signal, where we can clean up and
    // exit gracefully.
    let (signal, shutdown) = smol::channel::bounded::<()>(1);
    ctrlc::set_handler(move || {
        async_std::task::block_on(signal.send(())).unwrap();
    })
    .unwrap();

    // Initialize or load wallet
    let wallet = init_wallet(&args.wallet_path, &args.wallet_pass).await?;

    // Initialize or open sled database
    // TODO: Use proper OsPath here, not {}/{}
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

    // Initialize validator state
    let state = ValidatorState::new(
        &sled_db,
        genesis_ts,
        genesis_data,
        wallet.clone(),
        faucet_pubkeys,
        false,
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
        outbound_transports: net::settings::get_outbound_transports(args.sync_p2p_transports),
        localnet: args.localnet,
        channel_log: args.channel_log,
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
    let faucetd = Faucetd::new(
        state.clone(),
        sync_p2p.clone(),
        wallet.clone(),
        airdrop_timeout,
        airdrop_limit,
    )
    .await?;
    let faucetd = Arc::new(faucetd);

    // Task to periodically clean up the hashmap of airdrops.
    ex.spawn(prune_airdrop_map(faucetd.airdrop_map.clone(), airdrop_timeout)).detach();

    // JSON-RPC server
    info!("Starting JSON-RPC server");
    let _ex = ex.clone();
    ex.spawn(listen_and_serve(args.rpc_listen, faucetd.clone(), _ex)).detach();

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

    info!("Waiting for sync P2P outbound connections");
    sync_p2p.clone().wait_for_outbound(ex).await?;

    match block_sync_task(sync_p2p, state.clone()).await {
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

    info!("Closing wallet connection...");
    wallet.conn.close().await;
    info!("Closed wallet connection");

    Ok(())
}
