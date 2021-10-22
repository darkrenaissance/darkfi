use async_std::sync::Arc;
use std::convert::From;
use std::str::FromStr;
use std::time::Duration;

use async_executor::Executor;
use async_trait::async_trait;

use bitcoin::blockdata::{
    script::{Builder, Script},
    transaction::{OutPoint, SigHashType, Transaction, TxIn, TxOut},
};
use bitcoin::consensus::encode::serialize_hex;
use bitcoin::hash_types::PubkeyHash as BtcPubKeyHash;
use bitcoin::network::constants::Network;
use bitcoin::util::address::Address;
use bitcoin::util::ecdsa::{PrivateKey as BtcPrivKey, PublicKey as BtcPubKey};
use bitcoin::util::psbt::serialize::Serialize;
use electrum_client::{Client as ElectrumClient, ElectrumApi, GetBalanceRes};
use log::*;
use secp256k1::{
    constants::{PUBLIC_KEY_SIZE, SECRET_KEY_SIZE},
    key::{PublicKey, SecretKey},
    {rand::rngs::OsRng, Secp256k1},
    {All, Message as BtcMessage /*Secp256k1,*/},
};

use super::bridge::{NetworkClient, TokenNotification, TokenSubscribtion};
use crate::serial::{deserialize, serialize, Decodable, Encodable};
use crate::util::{generate_id, NetworkName};
use crate::{Error, Result};

// Swap out these types for any future non bitcoin-rs types
pub type PubAddress = Address;
pub type PubKey = BtcPubKey;
pub type PrivKey = BtcPrivKey;

const KEYPAIR_LENGTH: usize = SECRET_KEY_SIZE + PUBLIC_KEY_SIZE;

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
        Self {
            secret,
            public,
            context: secp,
        }
    }

    pub fn to_bytes(&self) -> [u8; KEYPAIR_LENGTH] {
        let mut bytes: [u8; KEYPAIR_LENGTH] = [0u8; KEYPAIR_LENGTH];

        bytes[..SECRET_KEY_SIZE].copy_from_slice(self.secret.as_ref());
        bytes[SECRET_KEY_SIZE..].copy_from_slice(&self.public.serialize());

        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Keypair> {
        if bytes.len() != KEYPAIR_LENGTH {
            return Err(Error::BtcFailed("Not right size".to_string()));
        }
        let secp = Secp256k1::new();

        //TODO: Map to errors properly, use context for public gen
        let secret = SecretKey::from_slice(&bytes[..SECRET_KEY_SIZE]).unwrap();
        let public = PublicKey::from_slice(&bytes[SECRET_KEY_SIZE..]).unwrap();

        Ok(Keypair {
            secret,
            public,
            context: secp,
        })
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

pub struct BtcClient {
    main_account: Account,
    notify_channel: (
        async_channel::Sender<TokenNotification>,
        async_channel::Receiver<TokenNotification>,
    ),
    client: Arc<ElectrumClient>,
    network: Network,
}

impl BtcClient {
    pub async fn new(main_keypair: Keypair, network: &str) -> Result<Arc<Self>> {
        //TODO
        // info!(target: "SOL BRIDGE", "Main BTC wallet pubkey: {:?}", &main_keypair.pubkey());

        let notify_channel = async_channel::unbounded();

        let (network, url) = match network {
            "mainnet" => (Network::Bitcoin, "ssl://electrum.blockstream.info:50002"),
            "testnet" => (Network::Testnet, "ssl://electrum.blockstream.info:60002"),
            _ => return Err(Error::NotSupportedNetwork),
        };

        let main_account = Account::new(&main_keypair, network);

        let electrum_client = ElectrumClient::new(&url)
            .map_err(|err| crate::Error::from(super::BtcFailed::from(err)))?;

        Ok(Arc::new(Self {
            main_account,
            notify_channel,
            client: Arc::new(electrum_client),
            network,
        }))
    }

    async fn handle_subscribe_request(
        self: Arc<Self>,
        btc_keys: Account,
        drk_pub_key: jubjub::SubgroupPoint,
    ) -> BtcResult<()> {
        debug!(
            target: "BTC BRIDGE",
            "Handle subscribe request"
        );
        let client = &self.client;

        let keys_clone = btc_keys.clone();
        // p2pkh script
        let script = keys_clone.script_pubkey;

        //Fetch any current balance
        let prev_balance = client.script_get_balance(&script)?;

        let cur_balance: GetBalanceRes;

        let status = client.script_subscribe(&script)?;

        loop {
            let current_status = client.script_pop(&script)?;
            debug!(target: "BTC BRIDGE", "script status: {:?}", status);
            debug!(target: "BTC BRIDGE", "current_script status: {:?}", current_status);
            if current_status == status {
                async_std::task::sleep(Duration::from_secs(5)).await;
                debug!(
                    target: "BTC BRIDGE",
                    "ScriptPubKey status has not changed, amtucfd: {}, amtcfd: {}",
                    client.script_get_balance(&script)?.unconfirmed,
                    client.script_get_balance(&script)?.confirmed
                );
                continue;
            }

            match current_status {
                Some(_) => {
                    // Script has a notification update
                    debug!(target: "BTC BRIDGE", "ScripPubKey notify update");
                    //TODO: unsubscribe is never successful
                    //let _ = client.script_unsubscribe(&script)?;
                    break;
                }
                None => {
                    return Err(BtcFailed::ElectrumError(
                        "ScriptPubKey was not found".to_string(),
                    ));
                }
            };
        } // Endloop

        cur_balance = client.script_get_balance(&script)?;

        let send_notification = self.notify_channel.0.clone();

        if cur_balance.unconfirmed < prev_balance.unconfirmed {
            return Err(BtcFailed::Notification(
                "New balance is less than previous balance".into(),
            ));
        }
        //TODO: Wait until they're confirmed balances above
        let amnt = cur_balance.unconfirmed - prev_balance.unconfirmed;
        let ui_amnt = amnt;

        send_notification
            .send(TokenNotification {
                network: NetworkName::Bitcoin,
                // is btc an acceptable token name?
                token_id: generate_id("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa", &NetworkName::Bitcoin)?,
                drk_pub_key,
                received_balance: amnt as u64,
                decimals: 8,
            })
            .await
            .map_err(Error::from)?;

        debug!(target: "BTC BRIDGE", "Received {} btc", ui_amnt);
        let _ = self.send_btc_to_main_wallet(amnt as u64, btc_keys)?;

        Ok(())
    }

    fn send_btc_to_main_wallet(self: Arc<Self>, amount: u64, btc_keys: Account) -> BtcResult<()> {
        debug!(target: "BTC BRIDGE", "Sending {} BTC to main wallet", amount);
        let client = &self.client;
        let keys_clone = btc_keys.clone();
        let script = keys_clone.script_pubkey;
        let utxo = client.script_list_unspent(&script)?;

        let mut inputs = Vec::new();
        let mut amounts: u64 = 0;
        for tx in utxo {
            let tx_in = TxIn {
                previous_output: OutPoint {
                    txid: tx.tx_hash,
                    vout: tx.tx_pos as u32,
                },
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
            output: vec![TxOut {
                script_pubkey: main_script_pubkey.clone(),
                value: amounts,
            }],
            lock_time: 0,
            version: 2,
        };

        //TODO: Better handling of fees, don't cast to u64
        let tx_size = transaction.get_size();
        //Estimate fee for getting in next block

        let fee_per_kb = client.estimate_fee(1)?;
        let _fee = tx_size as f64 * fee_per_kb * 100000 as f64;
        //let value = amounts - fee as u64;

        let transaction = Transaction {
            input: inputs,
            output: vec![TxOut {
                script_pubkey: main_script_pubkey,
                // TODO: calculate fee properly above
                value: amounts - 300,
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
        );
        let _txid = signed_tx.txid();
        let _serialized_tx = serialize(&signed_tx);

        debug!(target: "BTC BRIDGE", "Signed tx: {:?}",
               serialize_hex(&signed_tx));

        //TODO: Replace unwrap with error matching
        let txid = client
            .transaction_broadcast_raw(&signed_tx.serialize().to_vec())
            .unwrap();

        debug!(target: "BTC BRIDGE", "Sent {} satoshi to main wallet, txid: {}", amount, txid);
        Ok(())
    }
}

#[async_trait]
impl NetworkClient for BtcClient {
    async fn subscribe(
        self: Arc<Self>,
        drk_pub_key: jubjub::SubgroupPoint,
        _mint: Option<String>,
        executor: Arc<Executor<'_>>,
    ) -> Result<TokenSubscribtion> {
        // Generate bitcoin keys
        let keypair = Keypair::new();
        let btc_keys = Account::new(&keypair, self.network);
        let private_key = serialize(&keypair);
        let public_key = btc_keys.address.to_string();

        // start scheduler for checking balance
        debug!(target: "BRIDGE BITCOIN", "Subscribing for deposit");

        executor
            .spawn(async move {
                let result = self.handle_subscribe_request(btc_keys, drk_pub_key).await;
                if let Err(e) = result {
                    error!(target: "BTC BRIDGE SUBSCRIPTION","{}", e.to_string());
                }
            })
            .detach();

        Ok(TokenSubscribtion {
            private_key,
            public_key,
        })
    }

    async fn subscribe_with_keypair(
        self: Arc<Self>,
        private_key: Vec<u8>,
        _public_key: Vec<u8>,
        drk_pub_key: jubjub::SubgroupPoint,
        _mint: Option<String>,
        executor: Arc<Executor<'_>>,
    ) -> Result<String> {
        let keypair: Keypair = deserialize(&private_key)?;
        let btc_keys = Account::new(&keypair, self.network);
        let public_key = keypair.pubkey().to_string();

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
        let client = &self.client;
        let public_key = deserialize(&address)?;
        let script_pubkey = Account::derive_btc_script_pubkey(public_key, self.network);

        let main_script_pubkey = &self.main_account.script_pubkey;

        //TODO: Map to errors properly
        let main_utxo = client.script_list_unspent(&main_script_pubkey).unwrap();

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
        );

        //TODO: Replace unwrap with error matching
        let txid = client
            .transaction_broadcast_raw(&signed_tx.serialize().to_vec())
            .unwrap();
        debug!(target: "BTC BRIDGE", "Sent {} satoshi to external wallet, txid: {}", amount, txid);
        Ok(())
    }
}

pub fn sign_transaction(
    tx: Transaction,
    script_pubkey: Script,
    priv_key: SecretKey,
    pub_key: BtcPubKey,
    curve: &Secp256k1<All>,
) -> Transaction {
    let mut signed_inputs: Vec<TxIn> = Vec::new();

    for (i, unsigned_input) in tx.input.iter().enumerate() {
        let sighash = tx.signature_hash(i, &script_pubkey, SigHashType::All as u32);
        //TODO: replace unwrap
        let msg = BtcMessage::from_slice(&sighash.as_ref()).unwrap();

        let signature = curve.sign(&msg, &priv_key);
        let byte_signature = &signature.serialize_der();
        let mut with_hashtype = byte_signature.to_vec();
        with_hashtype.push(SigHashType::All as u8);

        let redeem_script = Builder::new()
            .push_slice(with_hashtype.as_slice())
            .push_key(&pub_key)
            .into_script();
        signed_inputs.push(TxIn {
            previous_output: unsigned_input.previous_output,
            script_sig: redeem_script,
            sequence: unsigned_input.sequence,
            witness: unsigned_input.witness.clone(),
        });
    }

    Transaction {
        version: tx.version,
        lock_time: tx.lock_time,
        input: signed_inputs,
        output: tx.output,
    }
}
impl Encodable for bitcoin::Transaction {
    fn encode<S: std::io::Write>(&self, s: S) -> Result<usize> {
        let tx = self.serialize();
        let len = tx.encode(s)?;
        Ok(len)
    }
}
impl Encodable for bitcoin::Address {
    fn encode<S: std::io::Write>(&self, s: S) -> Result<usize> {
        let addr = self.to_string();
        let len = addr.encode(s)?;
        Ok(len)
    }
}

impl Decodable for bitcoin::Address {
    fn decode<D: std::io::Read>(mut d: D) -> Result<Self> {
        let addr: String = Decodable::decode(&mut d)?;
        let addr = bitcoin::Address::from_str(&addr)
            .map_err(|err| crate::Error::from(BtcFailed::from(err)))?;
        Ok(addr)
    }
}

impl Encodable for bitcoin::PublicKey {
    fn encode<S: std::io::Write>(&self, s: S) -> Result<usize> {
        let key = self.to_bytes();
        let len = key.encode(s)?;
        Ok(len)
    }
}

impl Decodable for bitcoin::PublicKey {
    fn decode<D: std::io::Read>(mut d: D) -> Result<Self> {
        let key: Vec<u8> = Decodable::decode(&mut d)?;
        let key = bitcoin::PublicKey::from_slice(&key)
            .map_err(|err| crate::Error::from(BtcFailed::from(err)))?;
        Ok(key)
    }
}

impl Encodable for bitcoin::PrivateKey {
    fn encode<S: std::io::Write>(&self, s: S) -> Result<usize> {
        let key: String = self.to_string();
        let len = key.encode(s)?;
        Ok(len)
    }
}

impl Decodable for bitcoin::PrivateKey {
    fn decode<D: std::io::Read>(mut d: D) -> Result<Self> {
        let key: String = Decodable::decode(&mut d)?;
        let key = bitcoin::PrivateKey::from_str(&key)
            .map_err(|err| crate::Error::from(BtcFailed::from(err)))?;
        Ok(key)
    }
}
impl Encodable for secp256k1::key::PublicKey {
    fn encode<S: std::io::Write>(&self, s: S) -> Result<usize> {
        let key: Vec<u8> = self.serialize().to_vec();
        let len = key.encode(s)?;
        Ok(len)
    }
}
impl Decodable for secp256k1::key::PublicKey {
    fn decode<D: std::io::Read>(mut d: D) -> Result<Self> {
        let key: Vec<u8> = Decodable::decode(&mut d)?;
        let key = secp256k1::key::PublicKey::from_slice(&key)
            .map_err(|err| crate::Error::from(BtcFailed::from(err)))?;
        Ok(key)
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
            crate::Error::from(BtcFailed::DecodeAndEncodeError(
                "load keypair from slice".into(),
            ))
        })?;
        Ok(key)
    }
}

#[derive(Debug)]
pub enum BtcFailed {
    NotEnoughValue(u64),
    BadBtcAddress(String),
    ElectrumError(String),
    BtcError(String),
    DecodeAndEncodeError(String),
    KeypairError(String),
    Notification(String),
}

impl std::error::Error for BtcFailed {}

impl std::fmt::Display for BtcFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            BtcFailed::NotEnoughValue(i) => {
                write!(f, "There is no enough value {}", i)
            }
            BtcFailed::BadBtcAddress(ref err) => {
                write!(f, "Unable to create Electrum Client: {}", err)
            }
            BtcFailed::ElectrumError(ref err) => write!(f, "could not parse BTC address: {}", err),
            BtcFailed::DecodeAndEncodeError(ref err) => {
                write!(f, "Decode and decode keys error: {}", err)
            }
            BtcFailed::KeypairError(ref err) => {
                write!(f, "Keypair error from Secp256k1: {}", err)
            }
            BtcFailed::Notification(i) => {
                write!(f, "Received Notification Error: {}", i)
            }
            BtcFailed::BtcError(i) => {
                write!(f, "BtcFailed: {}", i)
            }
        }
    }
}

impl From<crate::error::Error> for BtcFailed {
    fn from(err: crate::error::Error) -> BtcFailed {
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
impl From<electrum_client::Error> for BtcFailed {
    fn from(err: electrum_client::Error) -> BtcFailed {
        BtcFailed::ElectrumError(err.to_string())
    }
}

impl From<bitcoin::util::key::Error> for BtcFailed {
    fn from(err: bitcoin::util::key::Error) -> BtcFailed {
        BtcFailed::DecodeAndEncodeError(err.to_string())
    }
}

pub type BtcResult<T> = std::result::Result<T, BtcFailed>;

#[cfg(test)]
mod tests {

    use super::Keypair;
    use crate::serial::{deserialize, serialize};
    use secp256k1::constants::{PUBLIC_KEY_SIZE, SECRET_KEY_SIZE};
    use std::str::FromStr;

    const KEYPAIR_LENGTH: usize = SECRET_KEY_SIZE + PUBLIC_KEY_SIZE;

    #[test]
    pub fn test_serialize_btc_address() -> super::BtcResult<()> {
        let btc_addr =
            bitcoin::Address::from_str(&String::from("mxVFsFW5N4mu1HPkxPttorvocvzeZ7KZyk"))?;

        let btc_ser = serialize(&btc_addr);
        let btc_dser = deserialize(&btc_ser)?;

        assert_eq!(btc_addr, btc_dser);

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
