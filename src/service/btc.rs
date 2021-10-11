use super::bridge::{NetworkClient, TokenNotification, TokenSubscribtion};
use crate::serial::{deserialize, serialize, Decodable, Encodable};
use crate::{Error, Result};
use crate::util::{generate_id, NetworkName};
use async_trait::async_trait;
use bitcoin::blockdata::script::Script;
use bitcoin::hash_types::{PubkeyHash as BtcPubKeyHash};
use bitcoin::network::constants::Network;
use bitcoin::util::address::Address;
use bitcoin::util::ecdsa::{PrivateKey as BtcPrivKey, PublicKey as BtcPubKey};
use electrum_client::{Client as ElectrumClient, ElectrumApi, GetBalanceRes};
use log::*;
use std::convert::From;

use secp256k1::constants::{PUBLIC_KEY_SIZE, SECRET_KEY_SIZE};
use secp256k1::key::{PublicKey, SecretKey};
use secp256k1::{rand::rngs::OsRng, Secp256k1};

use async_std::sync::Arc;
use std::str::FromStr;

// Swap out these types for any future non bitcoin-rs types
pub type PubAddress = Address;
pub type PubKey = BtcPubKey;
pub type PrivKey = BtcPrivKey;

const KEYPAIR_LENGTH: usize = SECRET_KEY_SIZE + PUBLIC_KEY_SIZE;

#[derive(Copy, Clone)]
pub struct Keypair {
    secret: SecretKey,
    public: PublicKey,
}
impl Keypair {
    pub fn new() -> Self {
        let secp = Secp256k1::new();
        let mut rng = OsRng::new().expect("OsRng");

        let (secret, public) = secp.generate_keypair(&mut rng);
        Self { secret, public }
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
        //TODO: Map to errors properly
        let secret = SecretKey::from_slice(&bytes[..SECRET_KEY_SIZE]).unwrap();
        let public = PublicKey::from_slice(&bytes[SECRET_KEY_SIZE..]).unwrap();

        Ok(Keypair { secret, public })
    }
    pub fn secret(&self) -> SecretKey {
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

pub struct BtcKeys {
    keypair: Arc<Keypair>,
    btc_privkey: BtcPrivKey,
    pub btc_pubkey: BtcPubKey,
    pub network: Network,
}

impl BtcKeys {
    pub fn new(keypair: &Keypair, network: Network) -> Self {

        let (secret_key, public_key) = keypair.as_tuple();

        let btc_privkey = BtcPrivKey::new(secret_key, network);
        let btc_pubkey = BtcPubKey::new(public_key);

        Self {
            keypair: Arc::new(*keypair),
            btc_privkey,
            btc_pubkey,
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
    pub fn derive_btc_address(btc_pubkey: BtcPubKey, network: Network) -> Address {
        Address::p2pkh(&btc_pubkey, network)
    }
    pub fn derive_script(btc_pubkey_hash: BtcPubKeyHash) -> Script {
        Script::new_p2pkh(&btc_pubkey_hash)
    }
}

pub struct BtcClient {
    main_keypair: Keypair,
    notify_channel: (
        async_channel::Sender<TokenNotification>,
        async_channel::Receiver<TokenNotification>,
    ),
    client: Arc<ElectrumClient>,
    network: Network,

}

impl BtcClient {
    pub async fn new(main_keypair: Vec<u8>, network: &str) -> Result<Arc<Self>> {
        let main_keypair: Keypair = deserialize(&main_keypair)?;

        let notify_channel = async_channel::unbounded();

        let (network, url) = match network {
            "mainnet" => (Network::Bitcoin, "ssl://electrum.blockstream.info:50002"),
            "testnet" => (Network::Testnet, "ssl://electrum.blockstream.info:60002"),
            _ => return Err(Error::NotSupportedNetwork),
        };

        let electrum_client = ElectrumClient::new(&url)
            .map_err(|err| crate::Error::from(super::BtcFailed::from(err)))?;

        Ok(Arc::new(Self {
            main_keypair,
            notify_channel,
            client: Arc::new(electrum_client),
            network,
        }))
    }

    async fn handle_subscribe_request(
        self: Arc<Self>,
        keypair: BtcKeys,
        drk_pub_key: jubjub::SubgroupPoint,
    ) -> BtcResult<()> {
        debug!(
            target: "BTC BRIDGE",
            "Handle subscribe request"
        );
        let client = &self.client;

        // p2pkh script
        let script = BtcKeys::derive_script(keypair.btc_pubkey_hash());
        //Subscribe to script, returns Error::AlreadySubscribed otherwise
        let status = client.script_subscribe(&script)?;

        //Fetch any current balance
        let prev_balance = client.script_get_balance(&script)?;

        let cur_balance: GetBalanceRes;

        loop {
            let current_status = client.script_pop(&script)?;
            if current_status != status {
                debug!(target: "BTC CLIENT", "ScriptPubKey status has not changed");
                continue
            }
            match current_status {
                // Script has a notification update
                Some(_status_update) => {
                    cur_balance = client.script_get_balance(&script)?;
                    break;
                }
                // No items in the queue
                None => {
                    debug!(target: "BTC CLIENT",
                           "Scriptpubkey does not yet exist in script notifications!");
                    continue;
                }
            }
        }

        let send_notification = self.notify_channel.0.clone();

        if cur_balance.confirmed < prev_balance.confirmed {
            return Err(BtcFailed::Notification(
                "New balance is less than previous balance".into(),
            ));
        }

        let amnt = cur_balance.confirmed - prev_balance.confirmed;
        let ui_amnt = amnt;

        send_notification
            .send(TokenNotification {
                network: NetworkName::Bitcoin,
                // is btc an acceptable token name?
                token_id: generate_id("btc", &NetworkName::Bitcoin)?,
                drk_pub_key,
                received_balance: amnt,
            })
            .await
            .map_err(Error::from)?;

        debug!(target: "BTC BRIDGE", "Received {} btc", ui_amnt);

        Ok(())
    }
}

#[async_trait]
impl NetworkClient for BtcClient {
    async fn subscribe(
        self: Arc<Self>,
        drk_pub_key: jubjub::SubgroupPoint,
        _mint: Option<String>,
    ) -> Result<TokenSubscribtion> {
        // Generate bitcoin keys
        let keypair = Keypair::new();
        let btc_keys = BtcKeys::new(&keypair, self.network);
        let secret_key = serialize(&keypair);
        let public_key = BtcKeys::btcpub_from_keypair(&keypair).to_string();

        // start scheduler for checking balance
        debug!(target: "BRIDGE BITCOIN", "Subscribing for deposit");

        smol::spawn(self.handle_subscribe_request(btc_keys, drk_pub_key)).detach();

        Ok(TokenSubscribtion {
            secret_key,
            public_key
        })
    }

    async fn subscribe_with_keypair(
        self: Arc<Self>,
        _private_key: Vec<u8>,
        _public_key: Vec<u8>,
        _drk_pub_key: jubjub::SubgroupPoint,
        _mint: Option<String>,
    ) -> Result<String> {
        // TODO this not implemented yet
        Ok(String::new())
    }
    async fn get_notifier(self: Arc<Self>) -> Result<async_channel::Receiver<TokenNotification>> {
        Ok(self.notify_channel.1.clone())
    }
    async fn send(self: Arc<Self>, _address: Vec<u8>, _amount: u64) -> Result<()> {
        // TODO this not implemented yet
        Ok(())
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

    use crate::serial::{deserialize, serialize};
    use std::str::FromStr;

    #[test]
    pub fn test_serialize_btc_address() -> super::BtcResult<()> {
        let btc_addr =
            bitcoin::Address::from_str(&String::from("mxVFsFW5N4mu1HPkxPttorvocvzeZ7KZyk"))?;

        let btc_ser = serialize(&btc_addr);
        let btc_dser = deserialize(&btc_ser)?;

        assert_eq!(btc_addr, btc_dser);

        Ok(())
    }
}
