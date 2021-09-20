//use crate::cli::jubjub::Fr;
use crate::client::{Client, ClientFailed};
use crate::serial::{deserialize, serialize, Decodable};
use crate::service::CashierClient;
use crate::{Error, Result};

use jsonrpc_core::BoxFuture;
use jsonrpc_derive::rpc;
use log::*;

use async_std::sync::{Arc, Mutex};
use futures::FutureExt;

/// Rpc trait
#[rpc(server)]
pub trait RpcClient {
    /// say hello
    #[rpc(name = "say_hello")]
    fn say_hello(&self) -> Result<String>;

    /// get key
    #[rpc(name = "get_key")]
    fn get_key(&self) -> BoxFuture<Result<String>>;

    /// create wallet
    #[rpc(name = "create_wallet")]
    fn create_wallet(&self) -> BoxFuture<Result<String>>;

    /// key gen
    #[rpc(name = "key_gen")]
    fn key_gen(&self) -> BoxFuture<Result<String>>;

    /// transfer
    #[rpc(name = "transfer")]
    fn transfer(
        &self,
        asset_id: Vec<u8>,
        pub_key: Vec<u8>,
        amount: f64,
    ) -> BoxFuture<Result<String>>;

    /// withdraw
    #[rpc(name = "withdraw")]
    fn withdraw(
        &self,
        asset_id: Vec<u8>,
        pub_key: Vec<u8>,
        amount: f64,
    ) -> BoxFuture<Result<String>>;

    /// deposit
    #[rpc(name = "deposit")]
    fn deposit(&self, asset_id: Vec<u8>) -> BoxFuture<Result<String>>;
}

pub struct RpcClientAdapter {
    client: Arc<Mutex<Client>>,
    cashier_client: Arc<Mutex<CashierClient>>,
}

impl RpcClientAdapter {
    pub fn new(client: Arc<Mutex<Client>>, cashier_client: Arc<Mutex<CashierClient>>) -> Self {
        Self {
            client,
            cashier_client,
        }
    }

    async fn get_key_process(client: Arc<Mutex<Client>>) -> Result<String> {
        let key_public = client.lock().await.state.wallet.get_keypairs()?[0].public;
        let bs58_address = bs58::encode(serialize(&key_public)).into_string();
        Ok(bs58_address)
    }

    async fn create_wallet_process(client: Arc<Mutex<Client>>) -> Result<String> {
        client.lock().await.state.wallet.init_db()?;
        Ok("wallet creation successful".into())
    }

    async fn key_gen_process(client: Arc<Mutex<Client>>) -> Result<String> {
        debug!(target: "RPC USER ADAPTER", "Generating keypair...");
        debug!(target: "RPC USER ADAPTER", "Attempting to write to database...");
        client.lock().await.state.wallet.key_gen()?;
        Ok("key generation successful".into())
    }

    async fn transfer_process(
        client: Arc<Mutex<Client>>,
        asset_id: Vec<u8>,
        address: Vec<u8>,
        amount: f64,
    ) -> Result<String> {
        let pub_key: String = deserialize(&address)?;
        let address = bs58::decode(pub_key.clone())
            .into_vec()
            .map_err(|_| ClientFailed::InvalidAddress(pub_key.clone()))?;

        let address: jubjub::SubgroupPoint =
            deserialize(&address).map_err(|_| ClientFailed::InvalidAddress(pub_key))?;

        let asset_id: jubjub::Fr = deserialize(&asset_id)?;

        client
            .lock()
            .await
            .transfer(asset_id, address.clone(), amount)
            .await?;

        Ok(format!("transfered {} DRK to {}", amount, address))
    }

    async fn withdraw_process(
        client: Arc<Mutex<Client>>,
        cashier_client: Arc<Mutex<CashierClient>>,
        asset_id: Vec<u8>,
        address: Vec<u8>,
        amount: f64,
    ) -> Result<String> {
        let asset_id: jubjub::Fr = deserialize(&asset_id)?;

        let drk_public = cashier_client
            .lock()
            .await
            .withdraw(asset_id, address)
            .await
            .map_err(|err| ClientFailed::from(err))?;

        if let Some(drk_addr) = drk_public {
            client
                .lock()
                .await
                .transfer(asset_id, drk_addr.clone(), amount)
                .await?;

            return Ok(format!(
                "sending {} drk to provided address for withdrawing: {} ",
                amount, drk_addr
            ));
        } else {
            return Err(Error::from(ClientFailed::UnableToGetWithdrawAddress));
        }
    }

    async fn deposit_process<T>(
        client: Arc<Mutex<Client>>,
        cashier_client: Arc<Mutex<CashierClient>>,
        asset_id: Vec<u8>,
    ) -> Result<String>
    where
        T: Decodable + ToString,
    {
        let asset_id: jubjub::Fr = deserialize(&asset_id)?;
        let deposit_addr = client.lock().await.state.wallet.get_keypairs()?[0].public;
        let coin_public = cashier_client
            .lock()
            .await
            .get_address(asset_id, deposit_addr)
            .await
            .map_err(|err| ClientFailed::from(err))?;

        if let Some(coin_addr) = coin_public {
            let pub_k: T = deserialize(&coin_addr)?;
            return Ok(pub_k.to_string());
        } else {
            return Err(Error::from(ClientFailed::UnableToGetDepositAddress));
        }
    }
}

impl RpcClient for RpcClientAdapter {
    fn say_hello(&self) -> Result<String> {
        debug!(target: "RPC USER ADAPTER", "say_hello() [START]");
        Ok(String::from("hello world"))
    }

    fn get_key(&self) -> BoxFuture<Result<String>> {
        debug!(target: "RPC USER ADAPTER", "get_key() [START]");
        Self::get_key_process(self.client.clone()).boxed()
    }

    fn create_wallet(&self) -> BoxFuture<Result<String>> {
        debug!(target: "RPC USER ADAPTER", "create_wallet() [START]");
        Self::create_wallet_process(self.client.clone()).boxed()
    }

    fn key_gen(&self) -> BoxFuture<Result<String>> {
        debug!(target: "RPC USER ADAPTER", "key_gen() [START]");
        Self::key_gen_process(self.client.clone()).boxed()
    }

    fn transfer(
        &self,
        asset_id: Vec<u8>,
        pub_key: Vec<u8>,
        amount: f64,
    ) -> BoxFuture<Result<String>> {
        debug!(target: "RPC USER ADAPTER", "transfer() [START]");
        Self::transfer_process(self.client.clone(), asset_id, pub_key, amount).boxed()
    }

    fn withdraw(
        &self,
        asset_id: Vec<u8>,
        pub_key: Vec<u8>,
        amount: f64,
    ) -> BoxFuture<Result<String>> {
        debug!(target: "RPC USER ADAPTER", "withdraw() [START]");
        Self::withdraw_process(
            self.client.clone(),
            self.cashier_client.clone(),
            asset_id,
            pub_key,
            amount,
        )
        .boxed()
    }

    fn deposit(&self, asset_id: Vec<u8>) -> BoxFuture<Result<String>> {
        debug!(target: "RPC USER ADAPTER", "deposit() [START]");
        #[cfg(feature = "default")]
        Self::deposit_process::<bitcoin::PublicKey>(
            self.client.clone(),
            self.cashier_client.clone(),
            asset_id,
        )
        .boxed()
    }
}
