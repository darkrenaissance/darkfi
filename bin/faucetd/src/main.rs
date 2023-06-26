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

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    str::FromStr,
};

use async_std::{
    stream::StreamExt,
    sync::{Arc, Mutex, RwLock},
    task,
};
use async_trait::async_trait;
use chrono::Utc;
use darkfi_money_contract::{
    client::{
        transfer_v1::TransferCallBuilder, MONEY_KEYS_COL_IS_DEFAULT, MONEY_KEYS_COL_PUBLIC,
        MONEY_KEYS_COL_SECRET, MONEY_KEYS_TABLE, MONEY_TREE_COL_TREE, MONEY_TREE_TABLE,
    },
    MoneyFunction::TransferV1 as MoneyTransfer,
    MONEY_CONTRACT_ZKAS_BURN_NS_V1, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{
        contract_id::MONEY_CONTRACT_ID, mimc_vdf, Keypair, MerkleTree, PublicKey, DARK_TOKEN_ID,
    },
    num_bigint::BigUint,
    num_traits::Num,
    pasta::{group::ff::PrimeField, pallas},
    tx::ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable};
use log::{debug, error, info};
use rand::rngs::OsRng;
use serde_json::{json, Value};
use signal_hook::consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};
use signal_hook_async_std::Signals;
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
    runtime::vm_runtime::SMART_CONTRACT_ZKAS_DB_NAME,
    tx::Transaction,
    util::{
        async_util::sleep,
        parse::decode_base10,
        path::{expand_path, get_config_path},
    },
    wallet::{WalletDb, WalletPtr},
    zk::{halo2::Field, proof::ProvingKey, vm::ZkCircuit, vm_heap::empty_witnesses},
    zkas::ZkBinary,
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
    sync_slots: usize,

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
type AirdropMap = Arc<Mutex<HashMap<[u8; 32], i64>>>;
type ChallengeMap = Arc<Mutex<HashMap<[u8; 32], (BigUint, u64)>>>;

pub struct Faucetd {
    synced: Mutex<bool>, // AtomicBool is weird in Arc
    sync_p2p: P2pPtr,
    validator_state: ValidatorStatePtr,
    keypair: Keypair,
    _wallet: WalletPtr,
    merkle_tree: MerkleTree,
    airdrop_timeout: i64,
    airdrop_limit: u64,
    airdrop_map: AirdropMap,
    challenge_map: ChallengeMap,
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
            Some("challenge") => return self.challenge(req.id, params).await,
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
        let cid = *MONEY_CONTRACT_ID;

        // Do a lookup for the money contract's zkas database and fetch the circuits.
        let blockchain = { validator_state.read().await.blockchain.clone() };
        let db_handle =
            blockchain.contracts.lookup(&blockchain.sled_db, &cid, SMART_CONTRACT_ZKAS_DB_NAME)?;

        let Some(mint_zkbytes) = db_handle.get(serialize(&MONEY_CONTRACT_ZKAS_MINT_NS_V1))? else {
            error!("{} zkas bincode not found in sled database", MONEY_CONTRACT_ZKAS_MINT_NS_V1);
            return Err(Error::ZkasBincodeNotFound);
        };

        let Some(burn_zkbytes) = db_handle.get(serialize(&MONEY_CONTRACT_ZKAS_BURN_NS_V1))? else {
            error!("{} zkas bincode not found in sled database", MONEY_CONTRACT_ZKAS_BURN_NS_V1);
            return Err(Error::ZkasBincodeNotFound);
        };

        let (mint_zkbin, _): (Vec<u8>, Vec<u8>) = deserialize(&mint_zkbytes)?;
        let (burn_zkbin, _): (Vec<u8>, Vec<u8>) = deserialize(&burn_zkbytes)?;

        let k = 13;
        let mint_zkbin = ZkBinary::decode(&mint_zkbin)?;
        let mint_circuit = ZkCircuit::new(empty_witnesses(&mint_zkbin), mint_zkbin.clone());

        let burn_zkbin = ZkBinary::decode(&burn_zkbin)?;
        let burn_circuit = ZkCircuit::new(empty_witnesses(&burn_zkbin), burn_zkbin.clone());

        info!("Creating mint circuit proving key");
        let mint_provingkey = ProvingKey::build(k, &mint_circuit);
        info!("Creating burn circuit proving key");
        let burn_provingkey = ProvingKey::build(k, &burn_circuit);

        {
            let provingkeys = vec![
                (MONEY_CONTRACT_ZKAS_MINT_NS_V1.to_string(), mint_provingkey, mint_zkbin),
                (MONEY_CONTRACT_ZKAS_BURN_NS_V1.to_string(), burn_provingkey, burn_zkbin),
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
            challenge_map: Arc::new(Mutex::new(HashMap::new())),
            proving_keys,
        };

        Ok(faucetd)
    }

    async fn initialize_wallet(wallet: WalletPtr) -> Result<MerkleTree> {
        // Perform wallet initialization for the money contract
        let wallet_schema = include_str!("../../../src/contract/money/wallet.sql");

        // Get a wallet connection
        info!("Acquiring wallet connection");
        let conn = wallet.conn.lock().await;

        info!("Initializing wallet schema");
        conn.execute(wallet_schema, rusqlite::params![])?;

        let query = format!("SELECT * FROM {}", MONEY_TREE_COL_TREE);
        let merkle_tree = conn.query_row(&query, [], |row| {
            let tree_bytes: Vec<u8> = row.get(MONEY_TREE_COL_TREE)?;
            Ok(deserialize(&tree_bytes).unwrap())
        });

        let merkle_tree = match merkle_tree {
            Ok(v) => {
                info!("Merkle tree already exists");
                v
            }

            Err(_) => {
                let tree = MerkleTree::new(100);
                let tree_bytes = serialize(&tree);
                let query = format!(
                    "DELETE FROM {}; INSERT INTO {} ({}) VALUES (?1)",
                    MONEY_TREE_TABLE, MONEY_TREE_TABLE, MONEY_TREE_COL_TREE
                );

                conn.execute(&query, rusqlite::params![tree_bytes])?;
                info!("Successfully initialized Merkle tree");
                tree
            }
        };

        Ok(merkle_tree)
    }

    async fn initialize_keypair(wallet: WalletPtr) -> Result<Keypair> {
        let conn = wallet.conn.lock().await;

        let query = format!(
            "SELECT {}, {} FROM {};",
            MONEY_KEYS_COL_PUBLIC, MONEY_KEYS_COL_SECRET, MONEY_KEYS_TABLE
        );

        let keypair = conn.query_row(&query, [], |row| {
            let public_bytes: Vec<u8> = row.get("public")?;
            let secret_bytes: Vec<u8> = row.get("secret")?;

            let public = deserialize(&public_bytes).unwrap();
            let secret = deserialize(&secret_bytes).unwrap();

            Ok(Keypair { public, secret })
        });

        let keypair = match keypair {
            Ok(k) => k,
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

                conn.execute(&query, rusqlite::params![is_default, public_bytes, secret_bytes])?;
                info!("Wrote keypair to wallet");
                keypair
            }
        };

        Ok(keypair)
    }

    // RPCAPI:
    // Request a VDF challenge in order to become eligible for an airdrop. It is then
    // necessary to execute the VDF with the challenge as input and pass it to the
    // `airdrop` call, which the faucet will then verify.
    //
    // **Params:**
    // * `array[0]`: base58 encoded address string of the recipient
    //
    // **Returns:**
    // * `array[0]`: hex-encoded challenge string
    // * `array[1]`: n steps (`u64`) needed for VDF evaluation
    //
    // --> {"jsonrpc": "2.0", "method": "challenge", "params": ["1DarkFi..."], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["0x123...", 10000], "id": 1}
    async fn challenge(&self, id: Value, params: &[Value]) -> JsonResult {
        const N_STEPS: u64 = 2_000_000;

        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        if !(*self.synced.lock().await) {
            error!("challenge(): Blockchain is not yet synced");
            return JsonError::new(InternalError, None, id).into()
        }

        let pubkey = match PublicKey::from_str(params[0].as_str().unwrap()) {
            Ok(v) => v,
            Err(e) => {
                error!("challenge(): Failed parsing PublicKey from String: {}", e);
                return server_error(RpcError::ParseError, id)
            }
        };

        let map = self.challenge_map.lock().await;
        if map.contains_key(&pubkey.to_bytes()) {
            return server_error(RpcError::RateLimitReached, id)
        }
        drop(map);

        // Create a random challenge
        let mut hasher = blake3::Hasher::new();
        hasher.update(&pubkey.to_bytes());
        hasher.update(&pallas::Base::random(&mut OsRng).to_repr());
        let h = hasher.finalize();
        let c = BigUint::from_str_radix(&h.to_hex(), 16).unwrap();

        // Add/Update this airdrop into the hashmap
        let mut map = self.challenge_map.lock().await;
        map.insert(pubkey.to_bytes(), (c.clone(), N_STEPS));
        drop(map);

        JsonResponse::new(json!([c.to_str_radix(16), N_STEPS]), id).into()
    }

    // RPCAPI:
    // Processes a native token airdrop request and airdrops requested amount to address.
    // Returns the transaction ID upon success.
    //
    // **Params:**
    // * `array[0]`: base58 encoded address string of the recipient
    // * `array[1]`: Amount to airdrop in form of f64
    // * `array[2]`: VDF evaluation witness as hex-encoded BigUint string
    //
    // **Returns:**
    // * hex-encoded transaction ID string
    //
    // --> {"jsonrpc": "2.0", "method": "airdrop", "params": ["1DarkFi...", 1.42, "0x123..."], "id": 1}
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

        // Decode public key
        let pubkey = match PublicKey::from_str(params[0].as_str().unwrap()) {
            Ok(v) => v,
            Err(e) => {
                error!("airdrop(): Failed parsing PublicKey from String: {}", e);
                return server_error(RpcError::ParseError, id)
            }
        };

        // Decode requested airdrop amount
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

        // Decode VDF witness
        let witness = params[2].as_str().unwrap();
        let Ok(witness) = BigUint::from_str_radix(witness, 16) else {
            error!("airdrop(): Failed parsing VDF witness from string");
            return server_error(RpcError::ParseError, id)
        };

        // Check if there as a previous airdrop and the timeout has passed.
        let now = Utc::now().timestamp();
        let map = self.airdrop_map.lock().await;
        if let Some(last_airdrop) = map.get(&pubkey.to_bytes()) {
            if now - last_airdrop <= self.airdrop_timeout {
                error!("airdrop(): Time limit reached for {}", pubkey);
                return server_error(RpcError::TimeLimitReached, id)
            }
        };
        drop(map);

        // Check if a VDF challenge exists
        let map = self.challenge_map.lock().await;
        let Some((challenge, n_steps)) = map.get(&pubkey.to_bytes()).cloned() else {
                error!("airdrop(): No VDF challenge found for {}", pubkey);
            return server_error(RpcError::NoVdfChallenge, id)
        };
        drop(map);

        // Verify the VDF
        info!("airdrop(): Verifying VDF for {}...", pubkey);
        if !mimc_vdf::verify(&challenge, n_steps, &witness) {
            error!("airdrop(): VDF verification failed for {}", pubkey);
            return server_error(RpcError::VdfVerifyFailed, id)
        }

        // Remove the challenge from the map at this point. Latter stuff might
        // fail, but we want clients to be able to request things again.
        let mut mut_map = self.challenge_map.lock().await;
        mut_map.remove(&pubkey.to_bytes());
        drop(mut_map);

        let cid = *MONEY_CONTRACT_ID;

        let (mint_zkbin, mint_pk, burn_zkbin, burn_pk) = {
            let proving_keys_r = self.proving_keys.read().await;
            let Some(arr) = proving_keys_r.get(&cid.to_bytes()) else {
                error!("Contract ID {} not found in proving keys hashmap", cid);
                return server_error(RpcError::InternalError, id)
            };

            let Some(mint_data) = arr.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_MINT_NS_V1) else {
                error!("{} proof data not found in vector", MONEY_CONTRACT_ZKAS_MINT_NS_V1);
                return server_error(RpcError::InternalError, id)
            };

            let Some(burn_data) = arr.iter().find(|x| x.0 == MONEY_CONTRACT_ZKAS_BURN_NS_V1) else {
                error!("{} proof data not found in vector", MONEY_CONTRACT_ZKAS_BURN_NS_V1);
                return server_error(RpcError::InternalError, id)
            };

            (mint_data.2.clone(), mint_data.1.clone(), burn_data.2.clone(), burn_data.1.clone())
        };

        // Create money contract transfer params and proofs
        let builder = TransferCallBuilder {
            keypair: self.keypair,
            recipient: pubkey,
            value: amount,
            token_id: *DARK_TOKEN_ID,
            rcpt_spend_hook: pallas::Base::zero(),
            rcpt_user_data: pallas::Base::zero(),
            rcpt_user_data_blind: pallas::Base::random(&mut OsRng),
            change_spend_hook: pallas::Base::zero(),
            change_user_data: pallas::Base::zero(),
            change_user_data_blind: pallas::Base::random(&mut OsRng),
            coins: vec![],
            tree: self.merkle_tree.clone(),
            mint_zkbin,
            mint_pk,
            burn_zkbin,
            burn_pk,
            clear_input: true,
        };

        let debris = match builder.build() {
            Ok(v) => v,
            Err(e) => {
                error!("Failed to build transfer tx params: {}", e);
                return server_error(RpcError::InternalError, id)
            }
        };

        // Build transaction
        let mut data = vec![MoneyTransfer as u8];
        debris.params.encode(&mut data).unwrap();
        let calls = vec![ContractCall { contract_id: cid, data }];
        let proofs = vec![debris.proofs];
        let mut tx = Transaction { calls, proofs, signatures: vec![] };
        let sigs = tx.create_sigs(&mut OsRng, &debris.signature_secrets).unwrap();
        tx.signatures = vec![sigs];

        // Safety check to see if the transaction is actually valid.
        let lock = self.validator_state.read().await;
        let current_slot = lock.consensus.time_keeper.current_slot();
        if let Err(e) = lock.verify_transactions(&[tx.clone()], current_slot, false).await {
            error!("airdrop(): Failed to verify transaction before broadcasting: {}", e);
            return JsonError::new(InternalError, None, id).into()
        }

        // Broadcast transaction to the network.
        self.sync_p2p.broadcast(&tx).await;

        // Add/Update this airdrop into the hashmap
        let mut map = self.airdrop_map.lock().await;
        map.insert(pubkey.to_bytes(), now);
        drop(map);

        let tx_hash = blake3::hash(&serialize(&tx)).to_hex().as_str().to_string();
        JsonResponse::new(json!(tx_hash), id).into()
    }
}

async fn prune_airdrop_maps(rate_map: AirdropMap, challenge_map: ChallengeMap, timeout: i64) {
    loop {
        sleep(timeout as u64).await;
        debug!("Pruning airdrop maps");

        let now = Utc::now().timestamp();

        let mut prune = vec![];

        let im_map = rate_map.lock().await;
        for (k, v) in im_map.iter() {
            if now - *v > timeout {
                prune.push(*k);
            }
        }
        drop(im_map);

        let mut mut_rate_map = rate_map.lock().await;
        let mut mut_challenge_map = challenge_map.lock().await;

        for i in prune {
            mut_rate_map.remove(&i);
            mut_challenge_map.remove(&i);
        }

        drop(mut_rate_map);
        drop(mut_challenge_map);
    }
}

async fn handle_signals(
    mut signals: Signals,
    _cfg_path: PathBuf,
    term_tx: smol::channel::Sender<()>,
) {
    debug!("Started signal handler");
    while let Some(signal) = signals.next().await {
        match signal {
            SIGHUP => {
                info!("Caught SIGHUP");
            }

            SIGTERM | SIGINT | SIGQUIT => {
                term_tx.send(()).await.unwrap();
            }

            _ => unreachable!(),
        }
    }
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'_>>) -> Result<()> {
    let cfg_path = get_config_path(args.config, CONFIG_FILE)?;

    // Signal handling for config reload and graceful termination.
    let signals = Signals::new([SIGHUP, SIGTERM, SIGINT, SIGQUIT])?;
    let handle = signals.handle();
    let (term_tx, term_rx) = smol::channel::bounded::<()>(1);
    let signals_task = task::spawn(handle_signals(signals, cfg_path.clone(), term_tx));

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

    // Initialize validator state
    let state = ValidatorState::new(
        &sled_db,
        bootstrap_ts,
        genesis_ts,
        genesis_data,
        initial_distribution,
        wallet.clone(),
        faucet_pubkeys,
        false,
        false,
    )
    .await?;

    // P2P network. The faucet doesn't participate in consensus, so we only
    // build the sync protocol.
    let network_settings = net::Settings {
        inbound_addrs: args.sync_p2p_accept,
        outbound_connections: args.sync_slots,
        external_addrs: args.sync_p2p_external,
        peers: args.sync_p2p_peer.clone(),
        seeds: args.sync_p2p_seed.clone(),
        allowed_transports: args.sync_p2p_transports,
        localnet: args.localnet,
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

    // Task to periodically clean up the airdrop rate/challenge hashmaps
    ex.spawn(prune_airdrop_maps(
        faucetd.airdrop_map.clone(),
        faucetd.challenge_map.clone(),
        airdrop_timeout,
    ))
    .detach();

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

    // TODO: I think this is not needed anymore
    //info!("Waiting for sync P2P outbound connections");
    //sync_p2p.clone().wait_for_outbound(ex).await?;

    match block_sync_task(sync_p2p, state.clone()).await {
        Ok(()) => *faucetd.synced.lock().await = true,
        Err(e) => error!("Failed syncing blockchain: {}", e),
    }

    // Wait for termination signal
    term_rx.recv().await?;
    print!("\r");
    info!("Caught termination signal, cleaning up and exiting...");
    handle.close();
    signals_task.await;

    info!("Flushing database...");
    let flushed_bytes = sled_db.flush_async().await?;
    info!("Flushed {} bytes", flushed_bytes);

    Ok(())
}
