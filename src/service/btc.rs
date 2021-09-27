use super::bridge::{NetworkClient, TokenNotification, TokenSubscribtion};
use crate::serial::{serialize, Decodable, Encodable};
use crate::Result;

use async_trait::async_trait;
use bitcoin::blockdata::script::Script;
use bitcoin::hash_types::{PubkeyHash as BtcPubKeyHash, Txid};
use bitcoin::network::constants::Network;
use bitcoin::util::address::Address;
use bitcoin::util::ecdsa::{PrivateKey as BtcPrivKey, PublicKey as BtcPubKey};
use electrum_client::{Client as ElectrumClient, ElectrumApi};
use log::*;

use secp256k1::key::{PublicKey, SecretKey};
use secp256k1::{rand::rngs::OsRng, Secp256k1};

use async_std::sync::Arc;
use std::str::FromStr;

// Swap out these types for any future non bitcoin-rs types
pub type PubAddress = Address;
pub type PubKey = BtcPubKey;
pub type PrivKey = BtcPrivKey;

const ELECTRUM_SERVER: &str = "ssl://blockstream.info:993";

pub struct BitcoinKeys {
    secret_key: SecretKey,
    public_key: PublicKey,
    _context: Secp256k1<secp256k1::All>,
    btc_privkey: BtcPrivKey,
    pub btc_pubkey: BtcPubKey,
    pub network: Network,
}

impl BitcoinKeys {
    pub fn new(network: Network) -> Result<Arc<BitcoinKeys>> {
        let secp = Secp256k1::new();
        let mut rng = OsRng::new().expect("OsRng");

        let (secret_key, public_key) = secp.generate_keypair(&mut rng);

        let btc_privkey = BtcPrivKey::new(secret_key, network);
        let btc_pubkey = btc_privkey.public_key(&secp);

        Ok(Arc::new(BitcoinKeys {
            secret_key,
            public_key,
            _context: secp,
            btc_privkey,
            btc_pubkey,
            network,
        }))
    }

    pub fn pubkey(&self) -> &PublicKey {
        &self.public_key
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
    _client: Arc<ElectrumClient>,
    _network: Network,
    _keypair: BitcoinKeys,
}

impl BtcClient {
    pub fn new(keypair: BitcoinKeys) -> Result<Arc<Self>> {
        let network = bitcoin::network::constants::Network::Testnet;
        let client = ElectrumClient::new(ELECTRUM_SERVER)
            .map_err(|err| crate::Error::from(super::BtcFailed::from(err)))?;
        Ok(Arc::new(Self {
            _client: Arc::new(client),
            _network: network,
            _keypair: keypair,
        }))
    }

    // pub async fn start_subscribe(self: Arc<Self>) -> BtcResult<(Txid, u64)> {
    //     debug!(target: "BTC CLIENT", "Subscribe to scriptpubkey");
    //     let client = &self.btc_client;
    //     // Check if script is already subscribed
    //     if let Some(status_start) = client.script_subscribe(&self.script)? {
    //         loop {
    //             match client.script_pop(&self.script)? {
    //                 Some(status) => {
    //                     // Script has a notification update
    //                     if status != status_start {
    //                         let balance = client.script_get_balance(&self.script)?;
    //                         if balance.confirmed > 0 {
    //                             debug!(target: "BTC CLIENT", "BTC Balance: Confirmed!");
    //                             let history = client.script_get_history(&self.script)?;
    //                             //return tx_hash of latest tx that created balance
    //                             return Ok((history[0].tx_hash, balance.confirmed));
    //                         } else {
    //                             debug!(target: "BTC CLIENT", "BTC Balance: Unconfirmed!");
    //                             continue;
    //                         }
    //                     } else {
    //                         debug!(target: "BTC CLIENT", "ScriptPubKey status has not changed");
    //                         continue;
    //                     }
    //                 }
    //                 None => {
    //                     debug!(target: "BTC CLIENT", "Scriptpubkey does not yet exist in script notifications!");
    //                     continue;
    //                 }
    //             };
    //         } // Endloop
    //     } else {
    //         return Err(BtcFailed::ElectrumError(
    //             "Did not subscribe to scriptpubkey".to_string(),
    //         ));
    //     }
    // }

    async fn handle_subscribe_request(self: Arc<Self>, keypair: Arc<BitcoinKeys>) -> Result<()> {
        debug!(
            target: "BTC BRIDGE",
            "Handle subscribe request"
        );

        // p2pkh script
        let _script = BitcoinKeys::derive_script(keypair.btc_pubkey_hash());

        //if self.client.script_subscribe(&script)?

        //let keypair = serialize(&keypair);

        Ok(())
    }
}

#[async_trait]
impl NetworkClient for BtcClient {
    async fn subscribe(self: Arc<Self>) -> Result<TokenSubscribtion> {
        // Generate bitcoin keys
        let btc_keys = BitcoinKeys::new(Network::Testnet)?;
        let btc_privkey = btc_keys.clone();
        let btc_privkey = btc_privkey.btc_privkey();
        let btc_pubkey = btc_keys.clone();
        let btc_pubkey = btc_pubkey.btc_pubkey();

        // start scheduler for checking balance
        debug!(target: "BRIDGE BITCOIN", "Subscribing for deposit");

        //let (_txid, _balance) = btc_keys.start_subscribe().await?;

        let self2 = self.clone();
        smol::spawn(self2.handle_subscribe_request(btc_keys)).detach();

        Ok(TokenSubscribtion {
            secret_key: serialize(&btc_privkey.to_bytes()),
            public_key: btc_pubkey.to_string(),
        })
    }

    async fn subscribe_with_keypair(
        self: Arc<Self>,
        _private_key: Vec<u8>,
        _public_key: Vec<u8>,
    ) -> Result<String> {
        // TODO this not implemented yet
        Ok(String::new())
    }

    async fn get_notifier(self: Arc<Self>) -> Result<async_channel::Receiver<TokenNotification>> {
        // TODO this not implemented yet
        let (_, notifier) = async_channel::unbounded();
        Ok(notifier)
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
        let key = self.to_bytes();
        let len = key.encode(s)?;
        Ok(len)
    }
}

impl Decodable for bitcoin::PrivateKey {
    fn decode<D: std::io::Read>(mut d: D) -> Result<Self> {
        let key: Vec<u8> = Decodable::decode(&mut d)?;
        let key = bitcoin::PrivateKey::from_slice(&key, Network::Testnet)
            .map_err(|err| crate::Error::from(BtcFailed::from(err)))?;
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
