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

// TODO: This module needs cleanup related to PublicKey/SecretKey types.

use std::{
    cmp::max,
    collections::BTreeMap,
    convert::{From, TryFrom, TryInto},
    fmt,
    ops::Add,
    str::FromStr,
    time::{Duration, Instant},
};

use anyhow::Context;
use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;

use bdk::electrum_client::{
    Client as ElectrumClient, ElectrumApi, GetBalanceRes, GetHistoryRes, HeaderNotification,
};
use bitcoin::{
    blockdata::{
        script::{Builder, Script},
        transaction::{OutPoint, SigHashType, Transaction, TxIn, TxOut},
    },
    consensus::encode::serialize_hex,
    hash_types::PubkeyHash as BtcPubKeyHash,
    network::constants::Network,
    util::{
        address::Address,
        ecdsa::{PrivateKey as BtcPrivKey, PublicKey as BtcPubKey},
        psbt::serialize::Serialize,
    },
};
use log::*;
use secp256k1::{
    constants::{PUBLIC_KEY_SIZE, SECRET_KEY_SIZE},
    key::{PublicKey, SecretKey},
    rand::rngs::OsRng,
    All, Message as BtcMessage, Secp256k1,
};

use super::bridge::{NetworkClient, TokenNotification, TokenSubscribtion};
use darkfi::{
    crypto::{keypair::PublicKey as DrkPublicKey, token_id::generate_id2},
    util::{
        expand_path, load_keypair_to_str,
        serial::{deserialize, serialize, Decodable, Encodable},
        NetworkName,
    },
    wallet::cashierdb::{CashierDb, TokenKey},
    Error, Result,
};

// Swap out these types for any future non bitcoin-rs types
pub type PubAddress = Address;
pub type PubKey = BtcPubKey;
pub type PrivKey = BtcPrivKey;

const KEYPAIR_LENGTH: usize = SECRET_KEY_SIZE + PUBLIC_KEY_SIZE;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct BlockHeight(u32);

impl From<BlockHeight> for u32 {
    fn from(height: BlockHeight) -> Self {
        height.0
    }
}

impl TryFrom<HeaderNotification> for BlockHeight {
    type Error = BtcFailed;
    fn try_from(value: HeaderNotification) -> BtcResult<Self> {
        Ok(Self(value.height.try_into().context("Failed to fit usize into u32")?))
    }
}

impl Add<u32> for BlockHeight {
    type Output = BlockHeight;
    fn add(self, rhs: u32) -> Self::Output {
        BlockHeight(self.0 + rhs)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExpiredTimelocks {
    None,
    Cancel,
    Punish,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Keypair {
    secret: SecretKey,
    public: PublicKey,
    context: Secp256k1<All>,
}

impl Keypair {
    pub fn new() -> Self {
        let secp = Secp256k1::new();
        let mut rng = OsRng::new().expect("OsRng");

        let (secret, public) = secp.generate_keypair(&mut rng);
        Self { secret, public, context: secp }
    }

    pub fn to_bytes(&self) -> [u8; KEYPAIR_LENGTH] {
        let mut bytes: [u8; KEYPAIR_LENGTH] = [0u8; KEYPAIR_LENGTH];

        bytes[..SECRET_KEY_SIZE].copy_from_slice(self.secret.as_ref());
        bytes[SECRET_KEY_SIZE..].copy_from_slice(&self.public.serialize());

        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> BtcResult<Keypair> {
        if bytes.len() != KEYPAIR_LENGTH {
            return Err(BtcFailed::KeypairError("Not right size".to_string()))
        }
        let secp = Secp256k1::new();

        let secret = SecretKey::from_slice(&bytes[..SECRET_KEY_SIZE])?;
        let public = PublicKey::from_slice(&bytes[SECRET_KEY_SIZE..])?;

        Ok(Keypair { secret, public, context: secp })
    }
    fn secret(&self) -> SecretKey {
        self.secret
    }
    pub fn pubkey(&self) -> PublicKey {
        self.public
    }
    pub fn as_tuple(&self) -> (SecretKey, PublicKey) {
        (self.secret, self.public)
    }
}

impl Default for Keypair {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Clone)]
pub struct Account {
    keypair: Arc<Keypair>,
    btc_privkey: BtcPrivKey,
    pub btc_pubkey: BtcPubKey,
    pub address: Address,
    pub script_pubkey: Script,
    pub network: Network,
}

impl Account {
    pub fn new(keypair: &Keypair, network: Network) -> Self {
        let (secret_key, _public_key) = keypair.as_tuple();

        let btc_privkey = BtcPrivKey::new(secret_key, network);
        let btc_pubkey = btc_privkey.public_key(&keypair.context);
        let address = Account::derive_btc_address(btc_pubkey, network);
        let script_pubkey = address.script_pubkey();

        Self {
            keypair: Arc::new(keypair.clone()),
            btc_privkey,
            btc_pubkey,
            address,
            script_pubkey,
            network,
        }
    }
    pub fn priv_from_secret(keypair: &Keypair, network: Network) -> BtcPrivKey {
        BtcPrivKey::new(keypair.secret(), network)
    }
    pub fn btcpub_from_keypair(keypair: &Keypair) -> BtcPubKey {
        BtcPubKey::new(keypair.public)
    }
    pub fn btc_privkey(&self) -> &BtcPrivKey {
        &self.btc_privkey
    }
    pub fn btc_pubkey(&self) -> &BtcPubKey {
        &self.btc_pubkey
    }
    pub fn btc_pubkey_hash(&self) -> BtcPubKeyHash {
        self.btc_pubkey.pubkey_hash()
    }
    pub fn derive_btc_script_pubkey(pubkey: PublicKey, network: Network) -> Script {
        let btc_pubkey = BtcPubKey::new(pubkey);
        let address = Address::p2pkh(&btc_pubkey, network);
        address.script_pubkey()
    }
    pub fn derive_btc_pubkey(pubkey: PublicKey) -> BtcPubKey {
        BtcPubKey::new(pubkey)
    }
    pub fn derive_btc_address(btc_pubkey: BtcPubKey, network: Network) -> Address {
        Address::p2pkh(&btc_pubkey, network)
    }
    pub fn derive_script(btc_pubkey_hash: BtcPubKeyHash) -> Script {
        Script::new_p2pkh(&btc_pubkey_hash)
    }
}
fn print_status_change(
    script: &Script,
    old: Option<ScriptStatus>,
    new: ScriptStatus,
) -> ScriptStatus {
    match (old, new) {
        (None, new_status) => {
            debug!(target: "BTC BRIDGE", "Found relevant script: {:?}, Status: {:?}", script, new_status);
        }
        (Some(old_status), new_status) if old_status != new_status => {
            debug!(target: "BTC BRIDGE", "Script status changed: {:?}, to {} from {}", script, new_status, old_status);
        }
        _ => {}
    }

    new
}
fn sync_interval(avg_block_time: Duration) -> Duration {
    max(avg_block_time / 10, Duration::from_secs(1))
}
pub struct Client {
    electrum: ElectrumClient,
    subscriptions: Vec<Script>,
    latest_block_height: BlockHeight,
    last_sync: Instant,
    sync_interval: Duration,
    script_history: BTreeMap<Script, Vec<GetHistoryRes>>,
}
impl Client {
    pub fn new(electrum_url: &str) -> BtcResult<Self> {
        let config = bdk::electrum_client::ConfigBuilder::default().retry(5).build();
        let _client = ElectrumClient::from_config(electrum_url, config)?;

        let electrum = ElectrumClient::new(electrum_url)
            .map_err(|err| darkfi::Error::from(super::BtcFailed::from(err)))?;

        let latest_block = electrum.block_headers_subscribe()?;

        //testnet avg block time
        let interval = sync_interval(Duration::from_secs(300));

        Ok(Self {
            electrum,
            subscriptions: Vec::new(),
            latest_block_height: BlockHeight::try_from(latest_block)?,
            last_sync: Instant::now(),
            sync_interval: interval,
            script_history: Default::default(),
        })
    }
    fn update_state(&mut self) -> Result<()> {
        let now = Instant::now();
        if now < self.last_sync + self.sync_interval {
            return Ok(())
        }

        self.last_sync = now;
        self.update_latest_block()?;
        self.update_script_histories()?;

        Ok(())
    }
    fn update_latest_block(&mut self) -> BtcResult<()> {
        let latest_block = self.electrum.block_headers_subscribe()?;
        let latest_block_height = BlockHeight::try_from(latest_block)?;

        if latest_block_height > self.latest_block_height {
            debug!( target: "BTC BRIDGE", "{} {}",
                u32::from(latest_block_height),
                "Got notification for new block"
            );
            self.latest_block_height = latest_block_height;
        }

        Ok(())
    }

    fn update_script_histories(&mut self) -> BtcResult<()> {
        let histories = self.electrum.batch_script_get_history(self.script_history.keys())?;

        if histories.len() != self.script_history.len() {
            debug!(
                "Expected {} history entries, received {}",
                self.script_history.len(),
                histories.len()
            );
        }

        let scripts = self.script_history.keys().cloned();
        let histories = histories.into_iter();

        self.script_history = scripts.zip(histories).collect::<BTreeMap<_, _>>();

        Ok(())
    }

    pub fn status_of_script(&mut self, script: Script) -> BtcResult<ScriptStatus> {
        if !self.script_history.contains_key(&script) {
            self.script_history.insert(script.clone(), vec![]);
        }
        self.update_state()?;

        let history = self.script_history.entry(script).or_default();

        match history.as_slice() {
            [] => Ok(ScriptStatus::Unseen),
            [_remaining @ .., last] => {
                if last.height <= 0 {
                    Ok(ScriptStatus::InMempool)
                } else {
                    Ok(ScriptStatus::Confirmed(Confirmed::from_inclusion_and_latest_block(
                        last.height as u32,
                        u32::from(self.latest_block_height),
                    )))
                }
            }
        }
    }
}
pub struct BtcClient {
    main_account: Account,
    client: Arc<Mutex<Client>>,
    notify_channel:
        (async_channel::Sender<TokenNotification>, async_channel::Receiver<TokenNotification>),
    network: Network,
}
impl BtcClient {
    pub async fn new(
        cashier_wallet: Arc<CashierDb>,
        network: &str,
        keypair_path: &str,
    ) -> Result<Arc<Self>> {
        let main_keypair: Keypair;

        let main_keypairs = cashier_wallet.get_main_keys(&NetworkName::Bitcoin).await?;

        if keypair_path.is_empty() {
            if main_keypairs.is_empty() {
                main_keypair = Keypair::new();
                cashier_wallet
                    .put_main_keys(
                        &TokenKey {
                            secret_key: serialize(&main_keypair),
                            public_key: serialize(&SecPublicKey(main_keypair.pubkey())),
                        },
                        &NetworkName::Bitcoin,
                    )
                    .await?;
            } else {
                main_keypair = deserialize(&main_keypairs[main_keypairs.len() - 1].secret_key)?;
            }
        } else {
            let keypair_str = load_keypair_to_str(expand_path(keypair_path)?)?;
            let keypair_bytes: Vec<u8> = serde_json::from_str(&keypair_str)?;
            main_keypair = Keypair::from_bytes(&keypair_bytes)
                .map_err(|e| BtcFailed::DecodeAndEncodeError(e.to_string()))?;
        }

        let notify_channel = async_channel::unbounded();

        let (network, url) = match network {
            "mainnet" => (Network::Bitcoin, "ssl://electrum.blockstream.info:50002"),
            "testnet" => (Network::Testnet, "ssl://electrum.blockstream.info:60002"),
            _ => return Err(Error::UnsupportedCoinNetwork),
        };

        let main_account = Account::new(&main_keypair, network);

        info!(target: "BTC BRIDGE", "Main BTC Address: {}", main_account.address.to_string());

        Ok(Arc::new(Self {
            main_account,
            client: Arc::new(Mutex::new(Client::new(url)?)),
            notify_channel,
            network,
        }))
    }

    async fn handle_subscribe_request(
        self: Arc<Self>,
        btc_keys: Account,
        drk_pub_key: DrkPublicKey,
    ) -> BtcResult<()> {
        let client = self.client.clone();

        let keys_clone = btc_keys.clone();
        let script = keys_clone.script_pubkey;

        if client.lock().await.subscriptions.contains(&script) {
            return Ok(())
        } else {
            client.lock().await.subscriptions.push(script.clone());
        }
        //Fetch any current balance
        let prev_balance = client.lock().await.electrum.script_get_balance(&script)?;
        let mut last_status = None;

        loop {
            async_std::task::sleep(Duration::from_secs(5)).await;
            let new_status = match client.lock().await.status_of_script(script.clone()) {
                Ok(new_status) => new_status,
                Err(error) => {
                    debug!(target: "BTC BRIDGE", "Failed to get status of script: {:#}", error);
                    return Err(BtcFailed::BtcError("Failed to get status of script".to_string()))
                }
            };

            last_status = Some(print_status_change(&script, last_status, new_status));

            match new_status {
                ScriptStatus::Unseen => continue,
                ScriptStatus::InMempool => break,
                ScriptStatus::Confirmed(inner) => {
                    //Only break when confirmations happen
                    let confirmations = inner.confirmations();
                    if confirmations > 1 {
                        break
                    }
                }
            }
        }

        let index = &mut client.lock().await.subscriptions.iter().position(|p| p == &script);

        if let Some(ind) = index {
            trace!(target: "BTC BRIDGE", "Removing subscription from list");
            let _ = &mut client.lock().await.subscriptions.remove(*ind);
        }

        let cur_balance: GetBalanceRes =
            client.lock().await.electrum.script_get_balance(&script)?;

        let send_notification = self.notify_channel.0.clone();
        //FIXME: dev
        if cur_balance.unconfirmed < prev_balance.unconfirmed {
            return Err(BtcFailed::Notification("New balance is less than previous balance".into()))
        }
        //Just check unconfirmed for now
        let amnt = cur_balance.confirmed - prev_balance.confirmed;
        let ui_amnt = amnt;
        send_notification
            .send(TokenNotification {
                network: NetworkName::Bitcoin,
                // is btc an acceptable token name?
                token_id: generate_id2(
                    "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa",
                    &NetworkName::Bitcoin,
                )?,
                drk_pub_key,
                received_balance: amnt as u64,
                decimals: 8,
            })
            .await
            .map_err(Error::from)?;

        info!(target: "BTC BRIDGE", "Received {} btc", ui_amnt);
        let _ = self.send_btc_to_main_wallet(amnt as u64, btc_keys).await;

        Ok(())
    }

    async fn send_btc_to_main_wallet(
        self: Arc<Self>,
        amount: u64,
        btc_keys: Account,
    ) -> BtcResult<()> {
        info!(target: "BTC BRIDGE", "Sending {} BTC to main wallet", amount);
        let client = self.client.lock().await;
        let electrum = &client.electrum;
        let keys_clone = btc_keys.clone();
        let script = keys_clone.script_pubkey;
        let utxo = electrum.script_list_unspent(&script)?;

        let mut inputs = Vec::new();
        let mut amounts: u64 = 0;
        for tx in utxo {
            let tx_in = TxIn {
                previous_output: OutPoint { txid: tx.tx_hash, vout: tx.tx_pos as u32 },
                sequence: 0xffffffff,
                witness: Vec::new(),
                script_sig: Script::new(),
            };
            inputs.push(tx_in);
            amounts += tx.value;
        }
        let main_script_pubkey = self.main_account.script_pubkey.clone();

        //TODO: Change to PSBT
        let transaction = Transaction {
            input: inputs.clone(),
            output: vec![TxOut { script_pubkey: main_script_pubkey.clone(), value: amounts }],
            lock_time: 0,
            version: 2,
        };

        let tx_size = transaction.get_size();

        let fee_per_kb = electrum.estimate_fee(1)?;
        let _fee = tx_size as f64 * fee_per_kb * 100000_f64;

        let transaction = Transaction {
            input: inputs,
            output: vec![TxOut {
                script_pubkey: main_script_pubkey,
                // TODO: calculate fee properly above
                value: amounts - 400,
            }],
            lock_time: 0,
            version: 2,
        };

        let _txid = transaction.txid();

        let signed_tx = sign_transaction(
            transaction,
            script,
            btc_keys.keypair.secret,
            btc_keys.btc_pubkey,
            &btc_keys.keypair.context,
        )?;

        let _txid = signed_tx.txid();
        let signed_tx = BtcTransaction(signed_tx);
        let _serialized_tx = serialize(&signed_tx);

        info!(target: "BTC BRIDGE", "Signed tx: {:?}",
            serialize_hex(&signed_tx.0));

        let txid = electrum.transaction_broadcast_raw(&signed_tx.0.serialize().to_vec())?;

        info!(target: "BTC BRIDGE", "Sent {} satoshi to main wallet, txid: {}", amount, txid);
        Ok(())
    }
}

#[async_trait]
impl NetworkClient for BtcClient {
    async fn subscribe(
        self: Arc<Self>,
        drk_pub_key: DrkPublicKey,
        _mint: Option<String>,
        executor: Arc<Executor<'_>>,
    ) -> Result<TokenSubscribtion> {
        // Generate bitcoin keys
        let keypair = Keypair::new();
        let btc_keys = Account::new(&keypair, self.network);
        let private_key = serialize(&keypair);
        let public_key = btc_keys.address.to_string();

        // start scheduler for checking balance
        trace!(target: "BRIDGE BITCOIN", "Subscribing for deposit");

        executor
            .spawn(async move {
                let result = self.handle_subscribe_request(btc_keys, drk_pub_key).await;
                if let Err(e) = result {
                    error!(target: "BTC BRIDGE SUBSCRIPTION","{}", e.to_string());
                }
            })
            .detach();

        Ok(TokenSubscribtion { private_key, public_key })
    }

    async fn subscribe_with_keypair(
        self: Arc<Self>,
        private_key: Vec<u8>,
        _public_key: Vec<u8>,
        drk_pub_key: DrkPublicKey,
        _mint: Option<String>,
        executor: Arc<Executor<'_>>,
    ) -> Result<String> {
        let keypair: Keypair = deserialize(&private_key)?;
        let btc_keys = Account::new(&keypair, self.network);
        let public_key = btc_keys.address.to_string();

        executor
            .spawn(async move {
                let result = self.handle_subscribe_request(btc_keys, drk_pub_key).await;
                if let Err(e) = result {
                    error!(target: "BTC BRIDGE SUBSCRIPTION","{}", e.to_string());
                }
            })
            .detach();

        Ok(public_key)
    }

    async fn get_notifier(self: Arc<Self>) -> Result<async_channel::Receiver<TokenNotification>> {
        Ok(self.notify_channel.1.clone())
    }

    async fn send(
        self: Arc<Self>,
        address: Vec<u8>,
        _mint: Option<String>,
        amount: u64,
    ) -> Result<()> {
        // address is not a btc address, so derive the btc address
        let electrum = &self.client.lock().await.electrum;
        let public_key = deserialize::<SecPublicKey>(&address)?.0;
        let script_pubkey = Account::derive_btc_script_pubkey(public_key, self.network);

        let main_script_pubkey = &self.main_account.script_pubkey;

        let main_utxo = electrum
            .script_list_unspent(main_script_pubkey)
            .map_err(|e| Error::from(BtcFailed::from(e)))?;

        let transaction = Transaction {
            input: vec![TxIn {
                previous_output: OutPoint {
                    txid: main_utxo[0].tx_hash,
                    vout: main_utxo[0].tx_pos as u32,
                },
                sequence: 0xffffffff,
                witness: Vec::new(),
                script_sig: Script::new(),
            }],
            output: vec![TxOut {
                script_pubkey: script_pubkey.clone(),
                // TODO: Calculate fees
                value: amount - 300,
            }],
            lock_time: 0,
            version: 2,
        };

        let signed_tx = sign_transaction(
            transaction,
            script_pubkey,
            self.main_account.keypair.secret,
            self.main_account.btc_pubkey,
            &self.main_account.keypair.context,
        )?;

        let txid = electrum
            .transaction_broadcast_raw(&signed_tx.serialize().to_vec())
            .map_err(|e| Error::from(BtcFailed::from(e)))?;

        info!(target: "BTC BRIDGE", "Sent {} satoshi to external wallet, txid: {}", amount, txid);
        Ok(())
    }
}

pub fn sign_transaction(
    tx: Transaction,
    script_pubkey: Script,
    priv_key: SecretKey,
    pub_key: BtcPubKey,
    curve: &Secp256k1<All>,
) -> BtcResult<Transaction> {
    let mut signed_inputs: Vec<TxIn> = Vec::new();

    for (i, unsigned_input) in tx.input.iter().enumerate() {
        let sighash = tx.signature_hash(i, &script_pubkey, SigHashType::All as u32);

        let msg = BtcMessage::from_slice(sighash.as_ref())?;

        let signature = curve.sign(&msg, &priv_key);
        let byte_signature = &signature.serialize_der();
        let mut with_hashtype = byte_signature.to_vec();
        with_hashtype.push(SigHashType::All as u8);

        let redeem_script =
            Builder::new().push_slice(with_hashtype.as_slice()).push_key(&pub_key).into_script();
        signed_inputs.push(TxIn {
            previous_output: unsigned_input.previous_output,
            script_sig: redeem_script,
            sequence: unsigned_input.sequence,
            witness: unsigned_input.witness.clone(),
        });
    }

    Ok(Transaction {
        version: tx.version,
        lock_time: tx.lock_time,
        input: signed_inputs,
        output: tx.output,
    })
}
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ScriptStatus {
    Unseen,
    InMempool,
    Confirmed(Confirmed),
}

impl ScriptStatus {
    pub fn from_confirmations(confirmations: u32) -> Self {
        match confirmations {
            0 => Self::InMempool,
            confirmations => Self::Confirmed(Confirmed::new(confirmations - 1)),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Confirmed {
    depth: u32,
}

impl Confirmed {
    pub fn new(depth: u32) -> Self {
        Self { depth }
    }
    pub fn from_inclusion_and_latest_block(inclusion_height: u32, latest_block: u32) -> Self {
        let depth = latest_block.saturating_sub(inclusion_height);

        Self { depth }
    }

    pub fn confirmations(&self) -> u32 {
        self.depth + 1
    }

    pub fn meets_target<T>(&self, target: T) -> bool
    where
        u32: PartialOrd<T>,
    {
        self.confirmations() >= target
    }
}

impl ScriptStatus {
    // Check if the script has any confirmations.
    pub fn is_confirmed(&self) -> bool {
        matches!(self, ScriptStatus::Confirmed(_))
    }
    // Check if the script has met the given confirmation target.
    pub fn is_confirmed_with<T>(&self, target: T) -> bool
    where
        u32: PartialOrd<T>,
    {
        match self {
            ScriptStatus::Confirmed(inner) => inner.meets_target(target),
            _ => false,
        }
    }

    pub fn has_been_seen(&self) -> bool {
        matches!(self, ScriptStatus::InMempool | ScriptStatus::Confirmed(_))
    }
}

impl fmt::Display for ScriptStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScriptStatus::Unseen => write!(f, "unseen"),
            ScriptStatus::InMempool => write!(f, "in mempool"),
            ScriptStatus::Confirmed(inner) => {
                write!(f, "confirmed with {} blocks", inner.confirmations())
            }
        }
    }
}

// Aliases
pub struct BtcTransaction(bitcoin::Transaction);
pub struct BtcAddress(bitcoin::Address);
pub struct BtcPublicKey(bitcoin::PublicKey);
pub struct BtcPrivateKey(bitcoin::PrivateKey);
pub struct SecPublicKey(secp256k1::PublicKey);

impl Encodable for BtcTransaction {
    fn encode<S: std::io::Write>(&self, s: S) -> Result<usize> {
        let tx = self.0.serialize();
        let len = tx.encode(s)?;
        Ok(len)
    }
}
impl Encodable for BtcAddress {
    fn encode<S: std::io::Write>(&self, s: S) -> Result<usize> {
        let addr = self.0.to_string();
        let len = addr.encode(s)?;
        Ok(len)
    }
}

impl Decodable for BtcAddress {
    fn decode<D: std::io::Read>(mut d: D) -> Result<Self> {
        let addr: String = Decodable::decode(&mut d)?;
        let addr = bitcoin::Address::from_str(&addr)
            .map_err(|err| darkfi::Error::from(BtcFailed::from(err)))?;
        Ok(BtcAddress(addr))
    }
}

impl Encodable for BtcPublicKey {
    fn encode<S: std::io::Write>(&self, s: S) -> Result<usize> {
        let key = self.0.to_bytes();
        let len = key.encode(s)?;
        Ok(len)
    }
}

impl Decodable for BtcPublicKey {
    fn decode<D: std::io::Read>(mut d: D) -> Result<Self> {
        let key: Vec<u8> = Decodable::decode(&mut d)?;
        let key = bitcoin::PublicKey::from_slice(&key)
            .map_err(|err| darkfi::Error::from(BtcFailed::from(err)))?;
        Ok(BtcPublicKey(key))
    }
}

impl Encodable for BtcPrivateKey {
    fn encode<S: std::io::Write>(&self, s: S) -> Result<usize> {
        let key: String = self.0.to_string();
        let len = key.encode(s)?;
        Ok(len)
    }
}

impl Decodable for BtcPrivateKey {
    fn decode<D: std::io::Read>(mut d: D) -> Result<Self> {
        let key: String = Decodable::decode(&mut d)?;
        let key = bitcoin::PrivateKey::from_str(&key)
            .map_err(|err| darkfi::Error::from(BtcFailed::from(err)))?;
        Ok(BtcPrivateKey(key))
    }
}
impl Encodable for SecPublicKey {
    fn encode<S: std::io::Write>(&self, s: S) -> Result<usize> {
        let key: Vec<u8> = self.0.serialize().to_vec();
        let len = key.encode(s)?;
        Ok(len)
    }
}
impl Decodable for SecPublicKey {
    fn decode<D: std::io::Read>(mut d: D) -> Result<Self> {
        let key: Vec<u8> = Decodable::decode(&mut d)?;
        let key = secp256k1::PublicKey::from_slice(&key)
            .map_err(|err| darkfi::Error::from(BtcFailed::from(err)))?;
        Ok(SecPublicKey(key))
    }
}
// TODO: add secret + public keys together for Encodable
impl Encodable for Keypair {
    fn encode<S: std::io::Write>(&self, s: S) -> Result<usize> {
        let key: Vec<u8> = self.to_bytes().to_vec();
        let len = key.encode(s)?;
        Ok(len)
    }
}

impl Decodable for Keypair {
    fn decode<D: std::io::Read>(mut d: D) -> Result<Self> {
        let key: Vec<u8> = Decodable::decode(&mut d)?;
        let key = Keypair::from_bytes(key.as_slice()).map_err(|_| {
            darkfi::Error::from(BtcFailed::DecodeAndEncodeError("load keypair from slice".into()))
        })?;
        Ok(key)
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum BtcFailed {
    #[error("There is no enough value {0}")]
    NotEnoughValue(u64),
    #[error("could not parse BTC address: {0}")]
    BadBtcAddress(String),
    #[error("Unable to create Electrum Client: {0}")]
    ElectrumError(String),
    #[error("BtcFailed: {0}")]
    BtcError(String),
    #[error("Decode and decode keys error: {0}")]
    DecodeAndEncodeError(String),
    #[error("Keypair error from Secp256k1:  {0}")]
    KeypairError(String),
    #[error("Received Notification Error: {0}")]
    Notification(String),
}

impl From<darkfi::error::Error> for BtcFailed {
    fn from(err: darkfi::error::Error) -> BtcFailed {
        BtcFailed::BtcError(err.to_string())
    }
}
impl From<secp256k1::Error> for BtcFailed {
    fn from(err: secp256k1::Error) -> BtcFailed {
        BtcFailed::KeypairError(err.to_string())
    }
}
impl From<bitcoin::util::address::Error> for BtcFailed {
    fn from(err: bitcoin::util::address::Error) -> BtcFailed {
        BtcFailed::BadBtcAddress(err.to_string())
    }
}
impl From<bdk::electrum_client::Error> for BtcFailed {
    fn from(err: bdk::electrum_client::Error) -> BtcFailed {
        BtcFailed::ElectrumError(err.to_string())
    }
}

impl From<bitcoin::util::key::Error> for BtcFailed {
    fn from(err: bitcoin::util::key::Error) -> BtcFailed {
        BtcFailed::DecodeAndEncodeError(err.to_string())
    }
}
impl From<anyhow::Error> for BtcFailed {
    fn from(err: anyhow::Error) -> BtcFailed {
        BtcFailed::DecodeAndEncodeError(err.to_string())
    }
}

impl From<BtcFailed> for Error {
    fn from(error: BtcFailed) -> Self {
        Error::CashierError(error.to_string())
    }
}

pub type BtcResult<T> = std::result::Result<T, BtcFailed>;

#[cfg(test)]
mod tests {
    use super::*;
    use darkfi::util::serial::{deserialize, serialize};
    use secp256k1::constants::{PUBLIC_KEY_SIZE, SECRET_KEY_SIZE};
    use std::str::FromStr;

    const KEYPAIR_LENGTH: usize = SECRET_KEY_SIZE + PUBLIC_KEY_SIZE;

    #[test]
    pub fn test_serialize_btc_address() -> super::BtcResult<()> {
        let btc_addr =
            bitcoin::Address::from_str(&String::from("mxVFsFW5N4mu1HPkxPttorvocvzeZ7KZyk"))?;

        let btc_addr = BtcAddress(btc_addr);

        let btc_ser = serialize(&btc_addr);
        let btc_dser = deserialize::<BtcAddress>(&btc_ser)?.0;

        assert_eq!(btc_addr.0, btc_dser);

        Ok(())
    }

    #[test]
    pub fn test_serialize_and_deserialize_keypair() -> super::BtcResult<()> {
        let keypair = Keypair::new();

        let bytes: [u8; KEYPAIR_LENGTH] = keypair.to_bytes();
        let keys = Keypair::from_bytes(&bytes)?;

        assert_eq!(keypair, keys);

        Ok(())
    }
}
