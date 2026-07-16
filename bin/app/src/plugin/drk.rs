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

use darkfi::{
    system::{Publisher, PublisherPtr, StoppableTask, sleep},
    tx::Transaction,
    util::parse::encode_base10,
    Result as DarkFiResult,
};
use darkfi_money_contract::model::TokenId;
use darkfi_serial::{serialize, Decodable, Encodable};
use darkfi_sdk::crypto::{keypair::{Address, Network, PublicKey, StandardAddress}};
use drk::{Drk, money::BALANCE_BASE10_DECIMALS, rpc::subscribe_blocks};
use smol::lock::RwLock;
use smol::channel::unbounded;
use std::{
    io::Cursor,
    sync::{Arc, OnceLock, Weak},
};
use url::Url;

use crate::{
    error::{Error, Result},
    prop::BatchGuardPtr,
    scene::{
        MethodCallSub, Pimpl, SceneNode, SceneNodePtr, SceneNodeType, SceneNodeWeak,
    },
    ExecutorPtr,
};

const DARKFID_ENDPOINT: &str = "tcp://127.0.0.1:18345"; // TODO: should be configurable at runtime
const DARKFID_RETRY_TIME: u64 = 20;

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
    Building,
    Broadcasting,
    Confirming,
    Confirmed,
    Error(String),
}

impl TxStatus {
    fn text(&self) -> String {
        match self {
            TxStatus::Building => "Building transaction...".to_string(),
            TxStatus::Broadcasting => "Broadcasting transaction...".to_string(),
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
    scan_progress_pub: PublisherPtr<(u32, u32)>,

    drk: Arc<RwLock<Drk>>,
    build_tx_channel: smol::channel::Sender<BuildTxRequest>,
}

impl DrkPlugin {
    pub async fn new(
        node: SceneNodeWeak,
        sg_root: SceneNodePtr,
        ex: ExecutorPtr,
    ) -> Result<Pimpl> {
        let node_ref = node.upgrade().unwrap();

        let setting_root = Arc::new(SceneNode::new("setting", SceneNodeType::SettingRoot));
        node_ref.link(setting_root.clone());

        let endpoint = Url::parse(DARKFID_ENDPOINT).unwrap();

        let drk = match Drk::new(Network::Testnet, get_cache_path().to_string_lossy().to_string(), get_wallet_path().to_string_lossy().to_string(), "changeme".to_string(), Some(endpoint), &ex, false).await {
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
            scan_progress_pub: Publisher::new(),
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
        let get_address_task = ex.spawn(async move {
            while Self::process_get_default_address(&me2, &method_sub).await {}
        });

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
        let build_tx_task = ex.spawn(async move {
            while Self::process_build_tx(&me2, &method_sub_build_tx).await {}
        });

        let node_ref = node.upgrade().unwrap();
        let me2 = Arc::downgrade(&self_);
        let method_sub_broadcast_tx = node_ref.subscribe_method_call("broadcast_tx").unwrap();
        let broadcast_tx_task = ex.spawn(async move {
            while Self::process_broadcast_tx(&me2, &method_sub_broadcast_tx).await {}
        });

        let tasks = vec![get_address_task, get_balances_task, get_tx_status_task, build_tx_task, broadcast_tx_task, build_tx_processor];
        self_.clone().start(ex.clone(), tasks).await;

        Ok(Pimpl::Drk(self_))
    }

    async fn apply_settings(_self: Arc<Self>, _batch: BatchGuardPtr) {
        // TODO
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
            let encoded = encode_base10(balance, BALANCE_BASE10_DECIMALS);
            let alias = aliases.get(&token_id_str).cloned().unwrap_or_else(|| "UNKN".to_string());
            let token_id = token_id_str.parse::<TokenId>().unwrap();

            result.push((alias, token_id, balance));
        }

        // Sort by balance
        result.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        Ok(result)
    }

    /// Emit balances_updated signal
    pub async fn emit_balances_updated(&self) {
        if let Some(node) = self.node.upgrade() {
            let _ = node.trigger("balances_updated", vec![]).await;
        }
    }

    /// Emit tx_updated signal
    pub async fn emit_tx_updated(&self, state: &TxState) {
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
    pub async fn emit_tx_status_updated(&self, status: &TxStatus) {
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
    pub async fn emit_tx_built(&self, amount: String, token_symbol: String, recipient: Address, tx: Transaction) {
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
    pub async fn emit_tx_built_error(&self, error: String) {
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
            d!("get_tx_history method closed");
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
                e!("Failed to encode balances");
                let _ = send_res.send(vec![]).await;
            }
        } else {
            e!("No send_res channel available");
        }

        true
    }

    /// Build a transaction without broadcasting it
    pub async fn build_tx(&self, amount: &str, token_id: TokenId, recipient: PublicKey) -> DarkFiResult<Transaction> {
        let drk = self.drk.read().await;

        drk.transfer(amount, token_id, recipient, None, None, false).await
    }

    /// Build a transaction from a BuildTxRequest (called by background task)
    async fn build_tx_request(&self, request: BuildTxRequest) -> DarkFiResult<(Transaction, String, Address, String)> {
        let drk = self.drk.read().await;
        let aliases = drk.get_aliases_mapped_by_token().await.unwrap_or_default();
        let token_symbol = aliases.get(&request.token_id.to_string()).unwrap_or(&"UNKN".to_string()).to_string();
        let recipient: Address = StandardAddress::from_public(drk.network, request.recipient).into();

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

        let Some(send_res) = method_call.send_res else {
            return true
        };

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
            self_.emit_tx_status_updated(&TxStatus::Error("failed to mark coins as spent".to_string())).await;
            return true
        };
        let tx_id = match drk.broadcast_tx(&tx, &mut vec![]).await {
            Ok(t) => t,
            Err(e) => {
                e!("Failed to broadcast transaction: {e}");
                self_.emit_tx_status_updated(&TxStatus::Error("failed to broadcast".to_string())).await;
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

        true
    }

    async fn start(self: Arc<Self>, ex: ExecutorPtr, tasks: Vec<smol::Task<()>>) {
        let endpoint = Url::parse(DARKFID_ENDPOINT).unwrap();

        let self2 = self.clone();
        let drk = self.drk.clone();
        let (shell_sender, shell_receiver) = unbounded();
        let ex_ = ex.clone();
        let progress_sub = self.scan_progress_pub.clone().subscribe().await;

        let scan_progress_task = ex.spawn(async move {
            let mut first_height = None;
            loop {
                let (height, final_height) = progress_sub.receive().await;
                if first_height.is_none() {
                    first_height = Some(height);
                }
                let progress: f64 = match final_height-first_height.unwrap() {
                    0 => 0.,
                    _ => (height-first_height.unwrap()) as f64 / (final_height-first_height.unwrap()) as f64,
                };
                let status: u8 = if progress > 0.5 {
                    2
                } else {
                    1
                };
                if let Some(node) = self2.node.upgrade() {
                    let _ = node.trigger("connect", serialize(&status)).await;
                }
            }
        });

        let self2 = self.clone();

        // Task that handles the RPC subscription with retry logic
        let subscribe_task = ex.spawn(async move {
            loop {
                i!("Attempting to connect to darkfid daemon at {}", endpoint);
                let subscribe_rpc_task = StoppableTask::new();
                let shell_sender = shell_sender.clone();
                let drk = drk.clone();
                let endpoint = endpoint.clone();
                let ex = ex_.clone();
                let progress_pub = self2.scan_progress_pub.clone();

                let _ = self2.node.upgrade().unwrap().trigger("connect", serialize(&0u8)).await;

                if let Err(e) = drk.read().await.scan_blocks(&mut vec![], Some(&shell_sender), &false, Some(progress_pub)).await {
                    e!("Failed during drk scanning: {e}");
                    let _ = self2.node.upgrade().unwrap().trigger("connect", serialize(&0u8)).await;

                    // Wait before retrying
                    i!("Retrying connection to darkfid in {} seconds...", DARKFID_RETRY_TIME);
                    sleep(DARKFID_RETRY_TIME).await;
                    continue
                }

                let _ = self2.node.upgrade().unwrap().trigger("connect", serialize(&3u8)).await;

                match subscribe_blocks(&drk, subscribe_rpc_task, shell_sender.clone(), endpoint, &ex).await {
                    Ok(()) => {
                        i!("darkfid subscription closed normally (detached task stopped)");
                    }
                    Err(e) => {
                        e!("darkfid connection failed: {e}");
                    }
                }

                let _ = self2.node.upgrade().unwrap().trigger("connect", serialize(&0u8)).await;

                // Wait before retrying
                i!("Retrying connection to darkfid in {} seconds...", DARKFID_RETRY_TIME);
                sleep(DARKFID_RETRY_TIME).await;
            }
        });

        let self2 = self.clone();
        let subscribe_recv_task = ex.spawn(async move {
            loop {
                let recv = shell_receiver.recv().await;

                if let Ok(lines) = recv {
                    self2.emit_balances_updated().await;

                    for line in lines.iter() {
                        i!(line);
                    }
                }
            }
        });

        let mut all_tasks = vec![scan_progress_task, subscribe_task, subscribe_recv_task];
        all_tasks.extend(tasks);
        self.tasks.set(all_tasks).unwrap();
    }
}
