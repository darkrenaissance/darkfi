/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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
    collections::BTreeMap,
    io::Cursor,
    sync::{Arc, OnceLock, Weak},
};

use parking_lot::Mutex as SyncMutex;
use smol::lock::RwLock;
use url::Url;

use darkfi::{
    blockchain::BlockInfo,
    rpc::{
        client::RpcClient,
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResult},
        util::JsonValue,
    },
    system::{sleep, Publisher, StoppableTask, StoppableTaskPtr},
    tx::Transaction,
    util::encoding::base64,
    Error as DarkFiError, Result as DarkFiResult,
};
use darkfi_money_contract::{
    model::TokenId, MONEY_CONTRACT_COIN_MERKLE_TREE, MONEY_CONTRACT_INFO_TREE,
    MONEY_CONTRACT_LATEST_COIN_ROOT,
};
use darkfi_sdk::crypto::{
    contract_id::MONEY_CONTRACT_ID,
    keypair::{Address, Network, PublicKey, StandardAddress},
    MerkleNode, MerkleTree,
};
use darkfi_serial::{deserialize, deserialize_async, serialize, Decodable, Encodable};
use drk::{money::KVDB_MERKLE_TREES_MONEY, rpc::DarkfidRpcClient, Drk};
use kvdb_overlay::Batch;

use crate::{
    error::{Error, Result},
    prop::{PropertyEnum, Role},
    scene::{MethodCallSub, Pimpl, SceneNodePtr, SceneNodeWeak},
    ExecutorPtr,
};

// TODO: should be configurable at runtime
//const DARKFID_ENDPOINT: &str = "tcp://127.0.0.1:18345";
/// Testnet endpoint from drk_config.toml
const DARKFID_ENDPOINT_TCP: &str = "tcp://127.0.0.1:18345";
/// TODO: replace with the real darkfid tor endpoint
const DARKFID_ENDPOINT_TOR: &str = "tor://darkfid-tor-placeholder.onion:18345";
const DARKFID_RETRY_TIME: u64 = 20;
const BLOCK_BATCHES_BUFFER: usize = 3;
const NEW_WALLET_INIT_RETRY_TIME: u64 = 5;

#[cfg(target_os = "android")]
mod paths {
    use crate::android::{get_appdata_path, get_external_storage_path};
    use std::path::PathBuf;

    pub fn get_cache_path() -> PathBuf {
        get_external_storage_path().join("drk/cache")
    }
    pub fn get_wallet_path() -> PathBuf {
        get_external_storage_path().join("drk/wallet.db")
    }
    pub fn get_use_tor_filename() -> PathBuf {
        get_external_storage_path().join("use_tor.txt")
    }
}

#[cfg(not(target_os = "android"))]
mod paths {
    use std::path::PathBuf;

    pub fn get_cache_path() -> PathBuf {
        dirs::data_local_dir().unwrap().join("darkfi/app/drk/cache")
    }
    pub fn get_wallet_path() -> PathBuf {
        dirs::data_local_dir().unwrap().join("darkfi/app/drk/wallet.db")
    }
    pub fn get_use_tor_filename() -> PathBuf {
        dirs::data_local_dir().unwrap().join("darkfi/app/drk/use_tor.txt")
    }
}

use paths::*;

macro_rules! t { ($($arg:tt)*) => { trace!(target: "plugin::drk", $($arg)*); } }
macro_rules! d { ($($arg:tt)*) => { debug!(target: "plugin::drk", $($arg)*); } }
macro_rules! i { ($($arg:tt)*) => { info!(target: "plugin::drk", $($arg)*); } }
macro_rules! e { ($($arg:tt)*) => { error!(target: "plugin::drk", $($arg)*); } }

#[derive(Debug, Clone)]
enum TxStatus {
    Confirming,
    Confirmed,
    Error(String),
}

impl TxStatus {
    fn text(&self) -> String {
        match self {
            TxStatus::Confirming => "Confirming transaction...".to_string(),
            TxStatus::Confirmed => "Transaction confirmed".to_string(),
            TxStatus::Error(ref err) => format!("Error sending transaction: {err}"),
        }
    }
}

#[derive(Debug, Clone)]
struct TxState {
    id: Option<String>,
    status: TxStatus,
    amount: Option<String>,
    token_symbol: Option<String>,
    recipient: Option<Address>,
}

pub type DrkPluginPtr = Arc<DrkPlugin>;

#[derive(Debug, Clone)]
struct BuildTxRequest {
    amount: String,
    token_id: TokenId,
    recipient: PublicKey,
}

pub struct DrkPlugin {
    node: SceneNodeWeak,
    sg_root: SceneNodePtr,
    tasks: OnceLock<Vec<smol::Task<()>>>,
    net_transport: PropertyEnum,
    subscribe_task: SyncMutex<Option<StoppableTaskPtr>>,

    drk: Arc<RwLock<Drk>>,
    build_tx_channel: smol::channel::Sender<BuildTxRequest>,
    last_balances: SyncMutex<Option<Vec<(String, TokenId, u64)>>>,
    ex: ExecutorPtr,
}

impl DrkPlugin {
    pub async fn new(node: SceneNodeWeak, sg_root: SceneNodePtr, ex: ExecutorPtr) -> Result<Pimpl> {
        let setting_node = sg_root.lookup_node("/setting").unwrap();
        let net_transport =
            PropertyEnum::wrap(&setting_node, Role::Internal, "net.transport", 0).unwrap();

        let endpoint = Self::endpoint(&net_transport);
        i!("Using {endpoint} transport for darkfid connection");

        let drk = match Drk::new(
            Network::Testnet,
            get_cache_path().to_string_lossy().to_string(),
            get_wallet_path().to_string_lossy().to_string(),
            "changeme".to_string(),
            Some(endpoint.clone()),
            &ex,
            false,
        )
        .await
        {
            Ok(wallet) => wallet,
            Err(e) => {
                eprintln!("Error initializing wallet: {e}");
                return Err(Error::ServiceFailed); // TODO: make a better error
            }
        };

        if let Err(e) = drk.initialize_wallet().await {
            e!("Error initializing wallet: {e}");
        }
        let mut output = vec![];
        if let Err(e) = drk.initialize_money(&mut output).await {
            e!("Failed to initialize Money: {e}");
        }

        if let Err(e) = drk.initialize_dao().await {
            e!("Failed to initialize DAO: {e}");
        }
        if let Err(e) = drk.initialize_deployooor().await {
            e!("Failed to initialize Deployooor: {e}");
        }

        // Generate a default address if needed
        match drk.default_address().await {
            Ok(_) => {
                i!("Default address already exists");
            }
            Err(e) => {
                i!("No default address found ({}), generating one...", e);
                if let Err(e) = drk.money_keygen(&mut output).await {
                    e!("Failed to generate keypair: {e}");
                } else {
                    i!("Generated default address");
                    match drk.addresses().await {
                        Ok(addrs) => {
                            if let Some((key_id, _, _, _)) = addrs.last() {
                                i!("Setting address with key_id {} as default", key_id);
                                if let Err(e) = drk.set_default_address(*key_id as u16).await {
                                    e!("Failed to set default address: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            e!("Failed to get addresses: {e}");
                        }
                    }
                }
            }
        }

        // Create channel for build_tx requests
        let (build_tx_tx, build_tx_rx) = smol::channel::unbounded();

        let self_ = Arc::new(Self {
            node: node.clone(),
            sg_root,
            tasks: OnceLock::new(),
            drk: drk.into_ptr(),
            build_tx_channel: build_tx_tx,
            net_transport,
            subscribe_task: SyncMutex::new(None),
            last_balances: SyncMutex::new(None),
            ex: ex.clone(),
        });

        // Start background task to process build_tx requests from channel
        let me3 = Arc::downgrade(&self_);
        let build_tx_processor = ex.spawn(async move {
            while let Ok(request) = build_tx_rx.recv().await {
                if let Some(self_) = me3.upgrade() {
                    match self_.build_tx_request(request).await {
                        Ok((tx, token_symbol, recipient, amount)) => {
                            self_.emit_tx_built(amount, token_symbol, recipient, tx).await;
                        }
                        Err(e) => {
                            e!("Failed to build transaction: {e}");
                            self_.emit_tx_built_error(e.to_string()).await;
                        }
                    }
                }
            }
        });

        let node_ref = node.upgrade().unwrap();
        let me2 = Arc::downgrade(&self_);
        let method_sub = node_ref.subscribe_method_call("get_default_address").unwrap();
        let get_address_task =
            ex.spawn(
                async move { while Self::process_get_default_address(&me2, &method_sub).await {} },
            );

        let node_ref = node.upgrade().unwrap();
        let me2 = Arc::downgrade(&self_);
        let method_sub_balances = node_ref.subscribe_method_call("get_balances").unwrap();
        let get_balances_task = ex.spawn(async move {
            while Self::process_get_balances(&me2, &method_sub_balances).await {}
        });

        let node_ref = node.upgrade().unwrap();
        let me2 = Arc::downgrade(&self_);
        let method_sub_tx_status = node_ref.subscribe_method_call("get_tx_status").unwrap();
        let get_tx_status_task = ex.spawn(async move {
            while Self::process_get_tx_status(&me2, &method_sub_tx_status).await {}
        });

        let node_ref = node.upgrade().unwrap();
        let me2 = Arc::downgrade(&self_);
        let method_sub_build_tx = node_ref.subscribe_method_call("build_tx").unwrap();
        let build_tx_task =
            ex.spawn(
                async move { while Self::process_build_tx(&me2, &method_sub_build_tx).await {} },
            );

        let node_ref = node.upgrade().unwrap();
        let me2 = Arc::downgrade(&self_);
        let method_sub_broadcast_tx = node_ref.subscribe_method_call("broadcast_tx").unwrap();
        let broadcast_tx_task = ex.spawn(async move {
            while Self::process_broadcast_tx(&me2, &method_sub_broadcast_tx).await {}
        });

        let tasks = vec![
            get_address_task,
            get_balances_task,
            get_tx_status_task,
            build_tx_task,
            broadcast_tx_task,
            build_tx_processor,
        ];
        self_.clone().start(tasks).await;

        Ok(Pimpl::Drk(self_))
    }

    /// Endpoint for the darkfid daemon connection, derived from the
    /// `net.transport` setting
    fn endpoint(net_transport: &PropertyEnum) -> Url {
        let endpoint = match net_transport.get().as_str() {
            "tor" => DARKFID_ENDPOINT_TOR,
            "tcp" => DARKFID_ENDPOINT_TCP,
            unhandled => panic!("Unhandled net.transport value: {unhandled}"),
        };
        Url::parse(endpoint).unwrap()
    }

    pub async fn get_default_address(&self) -> Result<String> {
        let drk = self.drk.read().await;
        let pubkey = drk.default_address().await.map_err(|e| {
            e!("Failed to get default address: {e}");
            Error::ServiceFailed
        })?;

        let network = drk.network;
        let address: darkfi_sdk::crypto::keypair::Address =
            StandardAddress::from_public(network, pubkey).into();

        Ok(address.to_string())
    }

    pub async fn get_balances(&self) -> Result<Vec<(String, TokenId, u64)>> {
        let drk = self.drk.read().await;

        let balances = drk.money_balance().await.map_err(|e| {
            e!("Failed to get money balance: {e}");
            Error::ServiceFailed
        })?;

        let aliases = drk.get_aliases_mapped_by_token().await.map_err(|e| {
            e!("Failed to get aliases: {e}");
            Error::ServiceFailed
        })?;

        let mut result: Vec<(String, TokenId, u64)> = Vec::new();
        for (token_id_str, balance) in balances {
            let alias = aliases.get(&token_id_str).cloned().unwrap_or_else(|| "UNKN".to_string());
            let token_id = token_id_str.parse::<TokenId>().unwrap();

            result.push((alias, token_id, balance));
        }

        // Sort by balance
        result.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        Ok(result)
    }

    /// Fetch the Money contract's `info` state tree records from darkfid.
    async fn fetch_money_info(drk: &Drk) -> DarkFiResult<BTreeMap<Vec<u8>, Vec<u8>>> {
        let params = JsonValue::Array(vec![
            JsonValue::String(MONEY_CONTRACT_ID.to_string()),
            JsonValue::String(MONEY_CONTRACT_INFO_TREE.to_string()),
        ]);
        let rep = drk.darkfid_daemon_request("blockchain.get_contract_state", &params).await?;
        let rep_str = rep.get::<String>().ok_or_else(|| {
            DarkFiError::Custom(
                "Malformed reply: contract state result is not a string".to_string(),
            )
        })?;
        let bytes = base64::decode(rep_str).ok_or_else(|| {
            DarkFiError::Custom("Malformed reply: contract state is not valid base64".to_string())
        })?;
        let records: BTreeMap<Vec<u8>, Vec<u8>> = deserialize_async(&bytes).await?;
        Ok(records)
    }

    /// Initialize a brand-new wallet at the chain tip.
    async fn new_wallet_init(&self) -> DarkFiResult<()> {
        let drk = self.drk.read().await;

        // Only an empty wallet may be initialized here
        let (last_height, last_hash) = drk
            .get_last_scanned_block()
            .map_err(|e| DarkFiError::Custom(format!("Could not get last scanned block: {e}")))?;
        if last_height != 0 || last_hash != "-" || !drk.get_coins(true).await?.is_empty() {
            return Err(DarkFiError::Custom(
                "new_wallet_init called on a non-empty wallet".to_string(),
            ))
        }

        let (tip_height, tip_hash) = drk.get_last_confirmed_block().await?;
        if tip_height == 0 {
            // Genesis-only chain
            return Ok(())
        }

        // Tree and root come from one consistent state snapshot
        let records = Self::fetch_money_info(&drk).await?;

        // Decode the Money coin tree
        let tree_bytes = records.get(MONEY_CONTRACT_COIN_MERKLE_TREE).ok_or_else(|| {
            DarkFiError::Custom("Coin tree not found in contract state".to_string())
        })?;
        let (_, rest) = tree_bytes.split_at_checked(4).ok_or_else(|| {
            DarkFiError::Custom("Malformed coin tree record: missing set size prefix".to_string())
        })?;
        let tree: MerkleTree = deserialize(rest)?;
        let mut tree = MerkleTree::from_parts(
            tree.prior_bridges().to_vec(),
            tree.current_bridge().clone(),
            tree.marked_indices().clone(),
            tree.checkpoints().clone(),
            u32::MAX as usize,
        )
        .map_err(|_| DarkFiError::Custom("Invalid coin tree record".to_string()))?;

        let root_bytes = records.get(MONEY_CONTRACT_LATEST_COIN_ROOT).ok_or_else(|| {
            DarkFiError::Custom("Latest coin root not found in contract state".to_string())
        })?;
        let latest_root: MerkleNode = deserialize(root_bytes)?;
        if tree.root(0) != Some(latest_root) {
            return Err(DarkFiError::Custom(
                "Fetched coin tree root does not match the chain's latest coin root".to_string(),
            ))
        }

        // Re-read the tip in case the chain advanced between reads
        let (tip_height2, tip_hash2) = drk.get_last_confirmed_block().await?;
        if tip_height != tip_height2 || tip_hash != tip_hash2 {
            return Err(DarkFiError::Custom(format!(
                "Tip moved during sync ({tip_height} -> {tip_height2})"
            )))
        }

        // Make the snapshot point rewindable by `reset_to_height`
        tree.checkpoint(tip_height as usize);

        let mut merkle_batch = Batch::new();
        merkle_batch.insert(KVDB_MERKLE_TREES_MONEY, &serialize(&tree));
        let mut scanned_batch = Batch::new();
        scanned_batch
            .insert(&tip_height.to_be_bytes(), &serialize(&(tip_hash.clone(), "-".to_string())));

        drk.cache.kvdb.atomic_write(&[
            (&drk.cache.merkle_trees, &merkle_batch),
            (&drk.cache.scanned_blocks, &scanned_batch),
        ])?;
        drk.cache.kvdb.flush_default_mode()?;

        i!(
            "New wallet state initialized at height {tip_height}, scanning starts from {}",
            tip_height + 1
        );

        Ok(())
    }

    /// Decode a single base64-encoded serialized block entry from darkfid.
    async fn decode_block_param(param: &JsonValue) -> DarkFiResult<BlockInfo> {
        let param_str = param.get::<String>().ok_or_else(|| {
            DarkFiError::Custom("Malformed reply: block entry is not a string".to_string())
        })?;
        let bytes = base64::decode(param_str).ok_or_else(|| {
            DarkFiError::Custom("Malformed reply: block entry is not valid base64".to_string())
        })?;
        let block = deserialize_async(&bytes).await?;
        Ok(block)
    }

    /// Get a batch of blocks from darkfid starting at `height` using the provided client.
    async fn fetch_blocks(rpc_client: &RpcClient, height: u32) -> DarkFiResult<Vec<BlockInfo>> {
        let req = JsonRequest::new(
            "blockchain.get_blocks",
            JsonValue::Array(vec![JsonValue::Number(height as f64)]),
        );

        let rep = rpc_client.request(req).await?;
        let params_array = rep.get::<Vec<JsonValue>>().ok_or_else(|| {
            DarkFiError::Custom("Malformed reply: get_blocks result is not an array".to_string())
        })?;
        let mut blocks = Vec::with_capacity(params_array.len());
        for param in params_array {
            blocks.push(Self::decode_block_param(param).await?);
        }
        Ok(blocks)
    }

    /// Fetch and scan blocks starting from the last scanned block.
    async fn scan_blocks(&self) -> DarkFiResult<()> {
        let drk = self.drk.read().await;

        // Grab last scanned block height
        let (mut height, mut hash) = drk
            .get_last_scanned_block()
            .map_err(|e| DarkFiError::Custom(format!("Could not get last scanned block: {e}")))?;

        // Never resume past a height that has no tree checkpoint
        let anchor =
            drk.get_money_tree().await?.checkpoints().back().map(|c| *c.id() as u32).unwrap_or(0);
        if height > anchor {
            let mut batch = Batch::new();
            for h in anchor + 1..=height {
                batch.remove(&h.to_be_bytes());
            }
            drk.cache.kvdb.atomic_write(&[
                (&drk.cache.scanned_blocks, &batch),
                (&drk.cache.state_inverse_diff, &batch),
            ])?;
            height = anchor;
            hash = drk
                .get_scanned_block(&height)
                .map_err(|e| DarkFiError::Custom(format!("Could not get scanned block: {e}")))?
                .0;
        }

        // Grab our last scanned block from darkfid
        let block = match drk.get_block_by_height(height).await {
            Ok(b) => Some(b),
            Err(darkfi::Error::JsonRpcError((-32121, _))) => None,
            Err(e) => return Err(e),
        };

        // Check if a reorg has happened
        if !block.as_ref().is_some_and(|b| b.hash().to_string() == hash) {
            match self
                .handle_reorg(&drk, height)
                .await
                .map_err(|e| DarkFiError::Custom(format!("Could not find common ancestor: {e}")))?
            {
                // The wallet was reset and re-initialized
                None => return Ok(()),
                Some(h) => height = h,
            }
        }

        // If last scanned block is genesis(0) we reset,
        // otherwise continue with the next block height.
        if height == 0 {
            drk.reset(&mut vec![])
                .await
                .map_err(|e| DarkFiError::Custom(format!("Wallet state reset failed: {e}")))?;
            self.emit_balances_updated().await;
        } else {
            height += 1;
        }

        // The expected previous block hash
        let mut expected_prev = if height > 0 {
            Some(
                drk.get_scanned_block(&(height - 1))
                    .map_err(|e| DarkFiError::Custom(format!("Could not get scanned block: {e}")))?
                    .0,
            )
        } else {
            None
        };

        // Grab last confirmed block height
        let (mut last_height, _) = drk.get_last_confirmed_block().await?;
        drop(drk);

        // Already scanned last confirmed block
        if height > last_height {
            return Ok(())
        }

        // Generate a new scan cache
        let mut cache = self.drk.read().await.scan_cache(false).await?;

        // Save starting height to report progress
        let start_height = height;

        // Create RPC client for block fetching
        let endpoint = Self::endpoint(&self.net_transport);
        let rpc_client = match RpcClient::new(endpoint, self.ex.clone()).await {
            Ok(client) => client,
            Err(e) => return Err(DarkFiError::Custom(format!("Failed to create RPC client: {e}"))),
        };
        let rpc_client = Arc::new(rpc_client);
        let rpc_client_ = rpc_client.clone();

        // Bounded channel limits block fetches
        let (block_tx, block_rx) = smol::channel::bounded(BLOCK_BATCHES_BUFFER);

        // Fetcher task: fetches blocks continuously, blocks when channel is full
        let fetcher_task = StoppableTask::new();
        let fetcher_task_ = fetcher_task.clone();
        fetcher_task.start(
            async move {
                let mut fetch_height = height;
                while fetch_height <= last_height {
                    match Self::fetch_blocks(&rpc_client_, fetch_height).await {
                        Ok(blocks) => {
                            let len = blocks.len() as u32;
                            if block_tx.send(blocks).await.is_err() {
                                // Channel closed, receiver is done
                                break;
                            }
                            fetch_height += len;
                        }
                        Err(e) => {
                            e!("Error fetching blocks: {e}");
                            break;
                        }
                    }
                }
                Ok(())
            },
            |_result| async move {
                // Cleanup: stop the RPC client
                rpc_client.stop().await;
            },
            DarkFiError::DetachedTaskStopped,
            self.ex.clone(),
        );

        // Update scan progress
        self.emit_progress_update(height - start_height, last_height - start_height).await;

        // Process blocks from the channel until it closes
        let original_last_height = last_height;
        let mut processing_error = None;
        loop {
            let blocks = match block_rx.recv().await {
                Ok(b) => b,
                Err(e) => {
                    // Channel closed: the fetcher finished or died early
                    if height < original_last_height && processing_error.is_none() {
                        processing_error = Some(DarkFiError::Custom(format!(
                            "Block fetcher ended early at height {height} (expected up to {original_last_height}): {e}"
                        )));
                    }
                    break;
                }
            };

            if blocks.is_empty() {
                processing_error = Some(DarkFiError::Custom(
                    "Received empty block batch from fetcher".to_string(),
                ));
                break;
            }

            let old_nullifiers = cache.owncoins_nullifiers.clone();

            // Scan each block, verifying the batch chains to our scan state
            for block in blocks {
                let block_height = block.header.height;
                if let Some(prev) = &expected_prev {
                    if &block.header.previous.to_string() != prev {
                        processing_error = Some(DarkFiError::Custom(format!(
                            "Non-contiguous block batch: block {block_height} does not chain to {prev}"
                        )));
                        break;
                    }
                }
                expected_prev = Some(block.header.hash().to_string());

                let drk = self.drk.read().await;
                if let Err(e) = drk.scan_block(&mut cache, &block).await {
                    processing_error = Some(e);
                    break;
                }

                height += 1;
            }

            if height > last_height {
                last_height = height;
            }

            // Update balances if they changed
            if cache.owncoins_nullifiers != old_nullifiers {
                self.emit_balances_updated().await;
            }

            // Update scan progress
            self.emit_progress_update(height - start_height, last_height - start_height).await;

            // Break on processing error
            if processing_error.is_some() {
                break;
            }
        }

        // Stop the fetcher gracefully before dropping the receiver
        fetcher_task_.stop().await;

        // Drop the receiver (now that fetcher is stopped)
        drop(block_rx);

        // Return any processing error that occurred
        if let Some(e) = processing_error {
            return Err(e);
        }

        Ok(())
    }

    /// Handle a subscribed block. Returns `Ok(false)` when a below-floor
    /// reorg reset the wallet: the caller must abort the subscription.
    async fn handle_subscribed_block(
        &self,
        last_scanned_height: &mut u32,
        block: BlockInfo,
    ) -> DarkFiResult<bool> {
        i!("Received block {}", block.header.height);

        let drk = self.drk.read().await;
        if block.header.height <= *last_scanned_height {
            let reset_height = block.header.height.saturating_sub(1);

            // Can't handle a reorg to below the earliest wallet state
            if self.scanned_floor(&drk)?.is_some_and(|floor| reset_height < floor) {
                drop(drk);
                self.handle_full_reset().await;
                return Ok(false)
            }

            if let Err(e) = drk.reset_to_height(reset_height, &mut vec![]).await {
                return Err(DarkFiError::Custom(format!("Wallet state reset failed: {e}")))
            }

            // Scan genesis again if needed
            if reset_height == 0 {
                let genesis = match drk.get_block_by_height(reset_height).await {
                    Ok(b) => b,
                    Err(e) => {
                        return Err(DarkFiError::Custom(format!("RPC client request failed: {e}")))
                    }
                };
                let mut scan_cache = drk.scan_cache(false).await?;
                if let Err(e) = drk.scan_block(&mut scan_cache, &genesis).await {
                    return Err(DarkFiError::Custom(format!("Scanning block failed: {e}")))
                };
            }
        }

        // The subscribed block must chain to our scanned state
        if block.header.height > 0 {
            let (prev_scanned_hash, _) =
                drk.get_scanned_block(&(block.header.height - 1)).map_err(|e| {
                    DarkFiError::Custom(format!(
                        "Missing scanned block record at height {}: {e}",
                        block.header.height - 1
                    ))
                })?;
            if prev_scanned_hash != block.header.previous.to_string() {
                return Err(DarkFiError::Custom(format!(
                    "Subscribed block {} does not chain to scanned block {}",
                    block.header.height,
                    block.header.height - 1
                )))
            }
        }

        let mut scan_cache = drk.scan_cache(false).await?;
        let old_nullifiers = scan_cache.owncoins_nullifiers.clone();

        if let Err(e) = drk.scan_block(&mut scan_cache, &block).await {
            return Err(DarkFiError::Custom(format!("Scanning block failed: {e}")))
        }

        if scan_cache.owncoins_nullifiers != old_nullifiers {
            self.emit_balances_updated().await;
        }

        // Set new last scanned block height
        *last_scanned_height = block.header.height;

        Ok(true)
    }

    /// Scan until the scanned tip matches darkfid's confirmed tip.
    /// Returns the scanned tip, used as the subscription baseline.
    async fn catch_up_to_tip(&self) -> DarkFiResult<u32> {
        loop {
            // First we do a clean scan
            if let Err(e) = self.scan_blocks().await {
                return Err(DarkFiError::Custom(format!("Failed during scanning: {e}")))
            }

            // Grab last confirmed block
            let drk = self.drk.read().await;
            let (last_confirmed_height, last_confirmed_hash) =
                drk.get_last_confirmed_block().await?;

            // Grab last scanned block
            let (last_scanned_height, last_scanned_hash) = match drk.get_last_scanned_block() {
                Ok(last) => last,
                Err(e) => {
                    return Err(DarkFiError::Custom(format!(
                        "Retrieving last scanned block failed: {e}"
                    )))
                }
            };
            drop(drk);

            // The wallet was fully reset
            if last_scanned_height == 0 && last_scanned_hash == "-" {
                return Err(DarkFiError::Custom("Wallet was reset during catch-up".to_string()))
            }

            // Rescan if other blocks have been created while we were scanning
            if last_confirmed_height != last_scanned_height ||
                last_confirmed_hash != last_scanned_hash
            {
                continue
            }

            return Ok(last_scanned_height)
        }
    }

    /// Call catch_up_to_tip() then subscribe to blocks from darkfid.
    async fn subscribe_blocks(&self, rpc_task: StoppableTaskPtr) -> DarkFiResult<()> {
        // Brand-new wallets (nothing scanned, no coins) are inited by
        // fetching the current money tree from darkfid.
        let drk = self.drk.read().await;
        let (last_height, last_hash) = drk
            .get_last_scanned_block()
            .map_err(|e| DarkFiError::Custom(format!("Could not get last scanned block: {e}")))?;
        let is_new_wallet =
            last_height == 0 && last_hash == "-" && drk.get_coins(true).await?.is_empty();
        drop(drk);

        if is_new_wallet {
            i!("New wallet detected, initializing state at the chain tip...");
            loop {
                if let Err(e) = self.new_wallet_init().await {
                    e!("New wallet init attempt failed: {e}");
                    i!("Retrying new wallet init in {NEW_WALLET_INIT_RETRY_TIME} seconds...");
                    sleep(NEW_WALLET_INIT_RETRY_TIME).await;
                    continue
                }
                break
            }
        }

        let mut last_scanned_height = self.catch_up_to_tip().await?;

        let publisher = Publisher::new();
        let subscription = publisher.clone().subscribe().await;
        let _publisher = publisher.clone();
        let rpc_client =
            Arc::new(RpcClient::new(Self::endpoint(&self.net_transport), self.ex.clone()).await?);
        let rpc_client_ = rpc_client.clone();

        rpc_task.clone().start(
            async move {
                let req = JsonRequest::new("blockchain.subscribe_blocks", JsonValue::Array(vec![]));
                rpc_client_.subscribe(req, _publisher).await
            },
            |res| async move {
                rpc_client.stop().await;
                match res {
                    Ok(()) |
                    Err(DarkFiError::DetachedTaskStopped) |
                    Err(DarkFiError::RpcServerStopped) => { /* Do nothing */ }
                    Err(e) => {
                        e!("JSON-RPC server error: {e}");
                        publisher
                            .notify(JsonResult::Error(JsonError::new(
                                ErrorCode::InternalError,
                                None,
                                0,
                            )))
                            .await;
                    }
                }
            },
            DarkFiError::RpcServerStopped,
            self.ex.clone(),
        );

        // Set the blockchain indicator to fully scanned status and remove the progress text
        if let Some(node) = self.node.upgrade() {
            let _ = node.trigger("connect", serialize(&(3u8, String::new()))).await;
        }

        // Wait for blocks from darkfid
        i!("Blockchain is fully scanned, waiting for blocks...");
        let e = 'outer: loop {
            match subscription.receive().await {
                JsonResult::Notification(n) => {
                    if n.method != "blockchain.subscribe_blocks" {
                        break DarkFiError::UnexpectedJsonRpc(format!(
                            "Got foreign notification from darkfid: {}",
                            n.method
                        ))
                    }

                    // Verify parameters
                    if !n.params.is_array() {
                        break DarkFiError::UnexpectedJsonRpc(
                            "Received notification params are not an array".to_string(),
                        )
                    }
                    let Some(params) = n.params.get::<Vec<JsonValue>>() else {
                        break DarkFiError::UnexpectedJsonRpc(
                            "Notification parameters are not a JSON array".to_string(),
                        )
                    };
                    if params.is_empty() {
                        break DarkFiError::UnexpectedJsonRpc(
                            "Notification parameters are empty".to_string(),
                        )
                    }

                    for param in params {
                        let block = match Self::decode_block_param(param).await {
                            Ok(b) => b,
                            Err(e) => break 'outer e,
                        };

                        match self.handle_subscribed_block(&mut last_scanned_height, block).await {
                            Ok(true) => continue,
                            // Below-floor reorg reset the wallet
                            Ok(false) => {
                                rpc_task.stop().await;
                                return Ok(())
                            }
                            Err(e) => break 'outer e,
                        }
                    }
                }

                JsonResult::Error(e) => {
                    // Some error happened in the transmission
                    break DarkFiError::UnexpectedJsonRpc(format!("Got error from JSON-RPC: {e:?}"))
                }

                x => {
                    // And this is weird
                    break DarkFiError::UnexpectedJsonRpc(format!(
                        "Got unexpected data from JSON-RPC: {x:?}"
                    ))
                }
            }
        };

        rpc_task.stop().await;
        Err(e)
    }

    /// Lowest height whose wallet state we possess: the earliest scanned
    /// block record. `None` when nothing was ever scanned.
    fn scanned_floor(&self, drk: &Drk) -> DarkFiResult<Option<u32>> {
        let records = drk.get_scanned_block_records().map_err(|e| {
            DarkFiError::Custom(format!("Could not get scanned block records: {e}"))
        })?;
        Ok(records.iter().map(|(height, _, _)| *height).min())
    }

    /// Full reset after a reorg below the earliest wallet state: wipe
    /// everything and re-initialize at the current tip.
    async fn handle_full_reset(&self) {
        e!("Reorg reached below the earliest wallet state, resetting and re-initializing");

        loop {
            let drk = self.drk.read().await;
            let result = drk.reset(&mut vec![]).await;
            drop(drk);

            self.emit_balances_updated().await;

            match result {
                Ok(()) => break,
                Err(e) => {
                    e!("Wallet reset attempt failed: {e}");
                    i!("Retrying wallet reset in {NEW_WALLET_INIT_RETRY_TIME} seconds...");
                    sleep(NEW_WALLET_INIT_RETRY_TIME).await;
                }
            }
        }

        loop {
            if let Err(e) = self.new_wallet_init().await {
                e!("New wallet init attempt failed: {e}");
                i!("Retrying new wallet init in {NEW_WALLET_INIT_RETRY_TIME} seconds...");
                sleep(NEW_WALLET_INIT_RETRY_TIME).await;
                continue
            }
            break
        }
    }

    /// Find the exact block height the reorg happened. If it happened below
    /// the earliest wallet state, the wallet is fully reset and re-initialized
    /// at the current tip, and `Ok(None)` is returned.
    async fn handle_reorg(&self, drk: &Drk, mut height: u32) -> DarkFiResult<Option<u32>> {
        i!("Reorg detected, finding common ancestor...");
        let floor = self.scanned_floor(drk)?;
        let start_height = height;
        height = height.saturating_sub(1);
        while height != 0 {
            // Below the earliest wallet state
            if floor.is_some_and(|floor| height < floor) {
                self.handle_full_reset().await;
                return Ok(None)
            }

            // Grab our scanned block hash for that height
            let (scanned_block_hash, _) =
                drk.get_scanned_block(&height).map_err(|e| DarkFiError::Custom(e.to_string()))?;

            // Grab the block from darkfid for that height
            let block = match drk.get_block_by_height(height).await {
                Ok(b) => Some(b),
                // Check if block was found
                Err(DarkFiError::JsonRpcError((-32121, _))) => None,
                Err(e) => {
                    e!("Error getting block {height} while finding common ancestor: {e}");
                    return Err(DarkFiError::Custom(e.to_string()))
                }
            };

            // Continue to previous one if they don't match
            if !block.as_ref().is_some_and(|b| b.hash().to_string() == scanned_block_hash) {
                height = height.saturating_sub(1);
                continue
            }

            // Reset to its height
            drk.reset_to_height(height, &mut vec![])
                .await
                .map_err(|e| DarkFiError::Custom(e.to_string()))?;

            self.emit_balances_updated().await;
            break
        }
        i!("Found common ancestor: {height} ({}-block reorg)", start_height - height);
        Ok(Some(height))
    }

    /// Emit balances_updated signal with the balances encoded in the payload.
    /// Only emits when the encoded balances differ from the last emitted ones.
    async fn emit_balances_updated(&self) {
        let Some(node) = self.node.upgrade() else { return };

        let balances = match self.get_balances().await {
            Ok(b) => b,
            Err(e) => {
                e!("Failed to get balances for balances_updated signal: {e}");
                return
            }
        };

        let mut data = vec![];
        if let Err(e) = balances.encode(&mut data) {
            e!("Failed to encode balances for balances_updated signal: {e}");
            return
        }

        let mut last = self.last_balances.lock();
        if let Some(last) = &*last {
            if *last == balances {
                return
            }
        }
        *last = Some(balances);

        let _ = node.trigger("balances_updated", data).await;
    }

    /// Emit a progress update
    async fn emit_progress_update(&self, blocks_scanned: u32, total_blocks: u32) {
        let Some(node) = self.node.upgrade() else { return };

        let percentage = if total_blocks > 0 { blocks_scanned * 100 / total_blocks } else { 0 };
        let status = if percentage > 50 { 2u8 } else { 1u8 };
        let _ = node
            .trigger(
                "connect",
                serialize(&(status, format!("{blocks_scanned}/{total_blocks} [{percentage}%]"))),
            )
            .await;
    }

    /// Emit tx_updated signal
    async fn emit_tx_updated(&self, state: &TxState) {
        if let Some(node) = self.node.upgrade() {
            let mut data = vec![];
            state.id.clone().encode(&mut data).unwrap();
            Some(state.status.text()).encode(&mut data).unwrap();
            state.amount.encode(&mut data).unwrap();
            state.token_symbol.clone().encode(&mut data).unwrap();
            state.recipient.map(|r| r.to_string()).encode(&mut data).unwrap();
            let _ = node.trigger("tx_updated", data).await;
        }
    }
    async fn emit_tx_status_updated(&self, status: &TxStatus) {
        if let Some(node) = self.node.upgrade() {
            let mut data = vec![];
            None::<String>.encode(&mut data).unwrap();
            Some(status.text()).encode(&mut data).unwrap();
            None::<String>.encode(&mut data).unwrap();
            None::<String>.encode(&mut data).unwrap();
            None::<String>.encode(&mut data).unwrap();
            let _ = node.trigger("tx_updated", data).await;
        }
    }

    /// Emit tx_built signal when transaction is built
    async fn emit_tx_built(
        &self,
        amount: String,
        token_symbol: String,
        recipient: Address,
        tx: Transaction,
    ) {
        if let Some(node) = self.node.upgrade() {
            let mut data = vec![];
            amount.encode(&mut data).unwrap();
            token_symbol.encode(&mut data).unwrap();
            recipient.to_string().encode(&mut data).unwrap();
            tx.encode(&mut data).unwrap();
            let _ = node.trigger("tx_built", data).await;
        }
    }

    /// Emit tx_built_error signal when transaction building fails
    async fn emit_tx_built_error(&self, error: String) {
        if let Some(node) = self.node.upgrade() {
            let mut data = vec![];
            error.encode(&mut data).unwrap();
            let _ = node.trigger("tx_built_error", data).await;
        }
    }

    async fn process_get_default_address(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            d!("get_default_address method closed");
            return false
        };

        t!("method called: get_default_address()");

        let Some(self_) = me.upgrade() else {
            e!("drk plugin destroyed before get_default_address task was stopped!");
            if let Some(send_res) = method_call.send_res {
                let _ = send_res.send(vec![]).await;
            }
            return false
        };

        let address = match self_.get_default_address().await {
            Ok(addr) => addr,
            Err(e) => {
                e!("Failed to get default address: {e}");
                if let Some(send_res) = method_call.send_res {
                    let _ = send_res.send(vec![]).await;
                }
                return true
            }
        };

        i!("Got default address: {address}");

        if let Some(send_res) = method_call.send_res {
            let mut cur = Cursor::new(vec![]);
            if address.encode(&mut cur).is_ok() {
                let _ = send_res.send(cur.into_inner()).await;
            } else {
                e!("Failed to encode default address");
                let _ = send_res.send(vec![]).await;
            }
        } else {
            e!("No send_res channel available");
        }

        true
    }

    async fn process_get_balances(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            d!("get_balances method closed");
            return false
        };

        t!("method called: get_balances()");

        let Some(self_) = me.upgrade() else {
            e!("drk plugin destroyed before get_balances task was stopped!");
            if let Some(send_res) = method_call.send_res {
                let _ = send_res.send(vec![]).await;
            }
            return false
        };

        let balances = match self_.get_balances().await {
            Ok(b) => b,
            Err(e) => {
                e!("Failed to get balances: {e}");
                if let Some(send_res) = method_call.send_res {
                    let _ = send_res.send(vec![]).await;
                }
                return true
            }
        };

        if let Some(send_res) = method_call.send_res {
            let mut cur = Cursor::new(vec![]);
            if balances.encode(&mut cur).is_ok() {
                let _ = send_res.send(cur.into_inner()).await;
            } else {
                e!("Failed to encode balances");
                let _ = send_res.send(vec![]).await;
            }
        } else {
            e!("No send_res channel available");
        }

        true
    }

    async fn process_get_tx_status(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            d!("get_tx_status method closed");
            return false
        };

        t!("method called: get_tx_status()");

        fn decode_data(data: &[u8]) -> std::io::Result<String> {
            let mut cur = Cursor::new(&data);
            let tx_id = String::decode(&mut cur)?;
            Ok(tx_id)
        }

        let Ok(tx_id) = decode_data(&method_call.data) else {
            d!("get_tx_status() method invalid arg data");
            return true
        };

        let Some(self_) = me.upgrade() else {
            e!("drk plugin destroyed before get_tx_status task was stopped!");
            if let Some(send_res) = method_call.send_res {
                let _ = send_res.send(vec![]).await;
            }
            return false
        };

        let drk = self_.drk.read().await;
        let Ok((_, status, _block_height, _tx)) = drk.get_tx_history_record(&tx_id).await else {
            d!("get_tx_history() method failed to get tx history record");
            return true
        };
        if let Some(send_res) = method_call.send_res {
            let mut cur = Cursor::new(vec![]);
            let status = match status.as_str() {
                "Broadcasted" => TxStatus::Confirming,
                "Confirmed" => TxStatus::Confirmed,
                _ => TxStatus::Error("unknown status".to_string()),
            };
            if status.text().encode(&mut cur).is_ok() {
                let _ = send_res.send(cur.into_inner()).await;
            } else {
                e!("Failed to encode tx status");
                let _ = send_res.send(vec![]).await;
            }
        } else {
            e!("No send_res channel available");
        }

        true
    }

    /// Build a transaction without broadcasting it
    pub async fn build_tx(
        &self,
        amount: &str,
        token_id: TokenId,
        recipient: PublicKey,
    ) -> DarkFiResult<Transaction> {
        let drk = self.drk.read().await;

        drk.transfer(amount, token_id, recipient, None, None, false).await
    }

    /// Build a transaction from a BuildTxRequest (called by background task)
    async fn build_tx_request(
        &self,
        request: BuildTxRequest,
    ) -> DarkFiResult<(Transaction, String, Address, String)> {
        let drk = self.drk.read().await;
        let aliases = drk.get_aliases_mapped_by_token().await.unwrap_or_default();
        let token_symbol =
            aliases.get(&request.token_id.to_string()).unwrap_or(&"UNKN".to_string()).to_string();
        let recipient: Address =
            StandardAddress::from_public(drk.network, request.recipient).into();

        let tx = self.build_tx(&request.amount, request.token_id, request.recipient).await?;

        Ok((tx, token_symbol, recipient, request.amount))
    }

    async fn process_build_tx(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            d!("build_tx method closed");
            return false
        };

        t!("method called: build_tx()");

        // Send empty response immediately to unblock the caller
        if let Some(send_res) = method_call.send_res {
            let _ = send_res.send(vec![]).await;
        }

        fn decode_data(data: &[u8]) -> std::io::Result<(String, TokenId, PublicKey)> {
            let mut cur = Cursor::new(&data);
            let amount = String::decode(&mut cur)?;
            let token_id = TokenId::decode(&mut cur)?;
            let recipient = PublicKey::decode(&mut cur)?;
            Ok((amount, token_id, recipient))
        }

        let Ok((amount, token_id, recipient)) = decode_data(&method_call.data) else {
            d!("build_tx() method invalid arg data");
            return true
        };

        let Some(self_) = me.upgrade() else {
            e!("drk plugin destroyed before build_tx task was stopped!");
            return false
        };

        // Send request to channel for background processing
        let request = BuildTxRequest { amount, token_id, recipient };
        let _ = self_.build_tx_channel.send(request).await;

        true
    }

    /// Broadcast a transaction
    pub async fn broadcast_tx(&self, tx: Transaction) -> Result<String> {
        let drk = self.drk.read().await;
        let tx_id = drk.broadcast_tx(&tx, &mut vec![]).await.map_err(|e| {
            e!("Failed to broadcast transaction: {e}");
            Error::ServiceFailed
        })?;

        Ok(tx_id)
    }

    async fn process_broadcast_tx(me: &Weak<Self>, sub: &MethodCallSub) -> bool {
        let Ok(method_call) = sub.receive().await else {
            d!("broadcast_tx method closed");
            return false
        };

        t!("method called: broadcast_tx()");

        let Some(send_res) = method_call.send_res else { return true };

        // Send empty response immediately to unblock the caller
        let _ = send_res.send(vec![]).await;

        let Some(self_) = me.upgrade() else {
            e!("drk plugin destroyed before broadcast_tx task was stopped!");
            return false
        };

        let Ok(tx) = Transaction::decode(&mut Cursor::new(&method_call.data)) else {
            d!("broadcast_tx() method invalid arg data");
            return true
        };

        let drk = self_.drk.read().await;
        if let Err(e) = drk.mark_tx_spend(&tx, &mut vec![]).await {
            e!("Failed to mark transaction coins as spent: {e}");
            self_
                .emit_tx_status_updated(&TxStatus::Error(
                    "failed to mark coins as spent".to_string(),
                ))
                .await;
            return true
        };
        let tx_id = match drk.broadcast_tx(&tx, &mut vec![]).await {
            Ok(t) => t,
            Err(e) => {
                e!("Failed to broadcast transaction: {e}");
                self_
                    .emit_tx_status_updated(&TxStatus::Error("failed to broadcast".to_string()))
                    .await;
                return true
            }
        };
        drop(drk);

        let state = TxState {
            id: Some(tx_id),
            status: TxStatus::Confirming,
            amount: None,
            token_symbol: None,
            recipient: None,
        };
        self_.emit_tx_updated(&state).await;
        self_.emit_balances_updated().await;

        true
    }

    async fn start(self: Arc<Self>, tasks: Vec<smol::Task<()>>) {
        let me2 = Arc::downgrade(&self);
        let subscribe_task = self.ex.spawn(async move {
            loop {
                let Some(self2) = me2.upgrade() else { break };
                let endpoint = Self::endpoint(&self2.net_transport);
                i!("Attempting to connect to darkfid daemon at {}", endpoint);
                let subscribe_rpc_task = StoppableTask::new();
                let subscribe_rpc_task_ = subscribe_rpc_task.clone();

                if let Some(node) = self2.node.upgrade() {
                    let _ = node.trigger("connect", serialize(&(0u8, String::new()))).await;
                }

                let task = StoppableTask::new();
                *self2.subscribe_task.lock() = Some(task.clone());
                let (task_res_tx, task_res_rx) = smol::channel::bounded(1);
                let self3 = self2.clone();
                task.clone().start(
                    async move {
                        let res = self3.subscribe_blocks(subscribe_rpc_task).await;
                        let _ = task_res_tx.send(res).await;
                        Ok(())
                    },
                    |res| async move {
                        match res {
                            Ok(()) | Err(DarkFiError::DetachedTaskStopped) => {}
                            Err(e) => {
                                e!("Session task error: {e}");
                            }
                        }
                    },
                    DarkFiError::DetachedTaskStopped,
                    self2.ex.clone(),
                );

                let res = match task_res_rx.recv().await {
                    Ok(res) => res,
                    Err(_) => {
                        i!("Subscribe task cancelled");
                        Err(DarkFiError::DetachedTaskStopped)
                    }
                };

                subscribe_rpc_task_.stop_nowait();

                let reset = match res {
                    Ok(()) => true,
                    Err(e) => {
                        e!("Failed during drk scanning: {e}");
                        false
                    }
                };
                *self2.subscribe_task.lock() = None;

                if let Some(node) = self2.node.upgrade() {
                    let _ = node.trigger("connect", serialize(&(0u8, String::new()))).await;
                }

                // Endpoint changed while we were connected, no need to sleep
                if Self::endpoint(&self2.net_transport) != endpoint {
                    continue
                }

                if !reset {
                    i!("Retrying connection to darkfid in {} seconds...", DARKFID_RETRY_TIME);
                    sleep(DARKFID_RETRY_TIME).await;
                }
            }
        });

        let net_transport = self.net_transport.clone();
        let net_transport_sub = net_transport.prop().subscribe_modify();
        let me3 = Arc::downgrade(&self);
        let ex_ = self.ex.clone();
        let transport_task = self.ex.spawn(async move {
            while let Ok(_) = net_transport_sub.receive().await {
                let Some(self2) = me3.upgrade() else { break };
                let transport = net_transport.get();
                let endpoint = Self::endpoint(&self2.net_transport);
                i!("Transport changed to {transport}, restarting darkfid connection at {endpoint}");

                // Cancel the scan/subscribe task
                let subscribe_task = {
                    let mut task = self2.subscribe_task.lock();
                    task.take()
                };
                if let Some(subscribe_task) = &subscribe_task {
                    subscribe_task.stop_nowait();
                }

                // Stop drk's rpc client
                let old_client = {
                    let mut drk = self2.drk.write().await;
                    drk.rpc_client.take()
                };
                if let Some(old_client) = old_client {
                    old_client.read().await.stop().await;
                }

                // Swap drk's rpc client over to the new endpoint
                let new_client = DarkfidRpcClient::new(endpoint, ex_.clone()).await;
                self2.drk.write().await.rpc_client = Some(RwLock::new(new_client));
            }
        });

        let mut all_tasks = vec![subscribe_task, transport_task];
        all_tasks.extend(tasks);
        self.tasks.set(all_tasks).unwrap();
    }
}
