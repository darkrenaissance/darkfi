use crate::{Error, Result};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

use async_executor::Executor;
use async_std::sync::Arc;
use bitcoin::blockdata::script::Script;
use bitcoin::network::constants::Network;
use bitcoin::util::address::Address;
use bitcoin::util::ecdsa::{PrivateKey, PublicKey};
use electrum_client::{Client as ElectrumClient, ElectrumApi};
use log::*;
use secp256k1::key::SecretKey;


// Swap out these types for any future non bitcoin-rs types
pub type PubAddress = Address;
pub type PubKey = PublicKey;
pub type PrivKey = PrivateKey;

#[allow(dead_code)]
pub struct BitcoinKeys {
    secret_key: SecretKey,
    bitcoin_private_key: PrivateKey,
    btc_client: Arc<ElectrumClient>,
    pub bitcoin_public_key: PublicKey,
    pub pub_address: Address,
    pub script: Script,
}

impl BitcoinKeys {
    pub fn new(btc_client: Arc<ElectrumClient>) -> Result<Arc<BitcoinKeys>> {
        let context = secp256k1::Secp256k1::new();

        // Probably not good enough for release
        let rand: String = thread_rng()
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();

        let rand_hex = hex::encode(rand);

        // Generate simple byte array from rand
        let data_slice: &[u8] = rand_hex.as_bytes();

        let secret_key = SecretKey::from_slice(&hex::decode(data_slice).unwrap()).unwrap();

        // Use Testnet
        let bitcoin_private_key = PrivateKey::new(secret_key, Network::Testnet);

        let bitcoin_public_key = PublicKey::from_private_key(&context, &bitcoin_private_key);
        //let pubkey_serialized = bitcoin_public_key.to_bytes();

        let pub_address = Address::p2pkh(&bitcoin_public_key, Network::Testnet);

        let script = Script::new_p2pk(&bitcoin_public_key);

        Ok(Arc::new(BitcoinKeys {
            secret_key,
            bitcoin_private_key,
            btc_client,
            bitcoin_public_key,
            pub_address,
            script,
        }))
    }

    pub async fn start_subscribe(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "BTC", "Subscribe");

        // Check if script is already subscribed
        if let Some(status) = self.btc_client.script_subscribe(&self.script).unwrap() {
            let subscribe_status_task =
                executor.spawn(self.subscribe_status_loop(status, executor.clone()));
            debug!(target: "BTC", "Subscribed to scripthash");
            let _ = subscribe_status_task.cancel().await;
            Ok(())
        } else {
            return Err(Error::ServicesError("received wrong command"));
        }
    }

    async fn subscribe_status_loop(
        self: Arc<Self>,
        status_start: electrum_client::ScriptStatus,
        _executor: Arc<Executor<'_>>,
    ) -> Result<Option<electrum_client::GetBalanceRes>> {
        loop {
            let check = self.btc_client.script_pop(&self.script).unwrap();
            match check {

                Some(status) => {
                    // Script has a notification update
                    if status != status_start {
                        let balance = self.btc_client.script_get_balance(&self.script).unwrap();
                        if balance.confirmed > 0 {
                            return Ok(Some(balance))
                        } else {
                            continue
                        }

                    } else {
                        continue
                    }
                }

                None => break,
            }
        }
        Ok(None)
    }

    // This should do a db lookup to return the same obj
    pub fn address_from_slice(key: &[u8]) -> Result<Address> {
        let pub_key = PublicKey::from_slice(key).unwrap();
        let address = Address::p2pkh(&pub_key, Network::Testnet);

        Ok(address)
    }

    pub fn get_deposit_address(&self) -> Result<&Address> {
        Ok(&self.pub_address)
    }
    pub fn get_pubkey(&self) -> &PublicKey {
        &self.bitcoin_public_key
    }
    pub fn get_privkey(&self) -> &PrivateKey {
        &self.bitcoin_private_key
    }
    pub fn get_script(&self) -> &Script {
        &self.script
    }
}
