use crate::{serial::deserialize, serial::serialize, Error, Result};

use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

use bitcoin::util::address::Address;
use bitcoin::util::ecdsa::{PrivateKey, PublicKey};
use secp256k1::key::SecretKey;

use bitcoin::network::constants::Network;

use async_executor::Executor;
use async_std::sync::Arc;

use clokwerk::AsyncScheduler;

// Swap out these types for any future non bitcoin-rs types
pub type PubAddress = Address;
pub type PubKey = PublicKey;
pub type PrivKey = PrivateKey;

pub struct BitcoinKeys {
    scheduler: AsyncScheduler,
    secret_key: SecretKey,
    bitcoin_private_key: PrivateKey,
    pub bitcoin_public_key: PublicKey,
    pub pub_address: Address,
}

impl BitcoinKeys {
    pub fn new() -> Result<BitcoinKeys> {
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

        // Create a scheduler for checking the address balance
        let scheduler = AsyncScheduler::new();

        Ok(Self {
            scheduler,
            secret_key,
            bitcoin_private_key,
            bitcoin_public_key,
            pub_address,
        })
    }
    pub fn start_scheduler(&self, executor: Arc<Executor<'_>>) -> Result<()> {
        //&self.scheduler.every(10.minutes()).run();

        Ok(())
    }

    async fn _watch_address(&self) -> Result<()> {

        Ok(())
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
}
