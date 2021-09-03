use crate::client::{Client, ClientFailed};
use crate::serial::serialize;
use crate::service::CashierClient;
use crate::wallet::WalletPtr;
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
    fn get_key(&self) -> Result<String>;

    /// create wallet
    #[rpc(name = "create_wallet")]
    fn create_wallet(&self) -> Result<String>;

    /// key gen
    #[rpc(name = "key_gen")]
    fn key_gen(&self) -> Result<String>;

    /// transfer
    #[rpc(name = "transfer")]
    fn transfer(&self, pub_key: String, amount: f64) -> BoxFuture<Result<String>>;

    /// withdraw
    #[rpc(name = "withdraw")]
    fn withdraw(&self, pub_key: String, amount: f64) -> BoxFuture<Result<String>>;

    /// deposit
    #[rpc(name = "deposit")]
    fn deposit(&self) -> BoxFuture<Result<String>>;
}

pub struct RpcClientAdapter {
    wallet: WalletPtr,
    client: Arc<Mutex<Client>>,
    cashier_client: Arc<Mutex<CashierClient>>,
}

impl RpcClientAdapter {
    pub fn new(
        wallet: WalletPtr,
        client: Arc<Mutex<Client>>,
        cashier_client: Arc<Mutex<CashierClient>>,
    ) -> Self {
        Self {
            wallet,
            client,
            cashier_client,
        }
    }

    async fn transfer_process(
        client: Arc<Mutex<Client>>,
        wallet: WalletPtr,
        address: String,
        amount: f64,
    ) -> Result<String> {
        client
            .lock()
            .await
            .transfer(address.clone(), amount, wallet.clone())
            .await?;

        Ok(format!("transfered {} DRK to {}", amount, address))
    }

    async fn withdraw_process(
        client: Arc<Mutex<Client>>,
        cashier_client: Arc<Mutex<CashierClient>>,
        wallet: WalletPtr,
        address: String,
        amount: f64,
    ) -> Result<String> {
        let drk_public = cashier_client
            .lock()
            .await
            .withdraw(address)
            .await
            .map_err(|err| ClientFailed::from(err))?;

        if let Some(drk_addr) = drk_public {
            let drk_addr = bs58::encode(serialize(&drk_addr)).into_string();

            client
                .lock()
                .await
                .transfer(drk_addr.clone(), amount, wallet.clone())
                .await?;

            return Ok(format!(
                "sending {} dbtc to provided address for withdrawing: {} ",
                amount, drk_addr
            ));
        } else {
            return Err(Error::from(ClientFailed::UnableToGetWithdrawAddress));
        }
    }

    async fn deposit_process(
        cashier_client: Arc<Mutex<CashierClient>>,
        wallet: WalletPtr,
    ) -> Result<String> {
        let deposit_addr = wallet.get_public()?;
        let btc_public = cashier_client
            .lock()
            .await
            .get_address(deposit_addr)
            .await
            .map_err(|err| ClientFailed::from(err))?;

        if let Some(btc_addr) = btc_public {
            return Ok(btc_addr.to_string());
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

    fn get_key(&self) -> Result<String> {
        debug!(target: "RPC USER ADAPTER", "get_key() [START]");
        let key_public = self.wallet.get_public()?;
        let bs58_address = bs58::encode(serialize(&key_public)).into_string();
        Ok(bs58_address)
    }

    fn create_wallet(&self) -> Result<String> {
        debug!(target: "RPC USER ADAPTER", "create_wallet() [START]");
        self.wallet.init_db()?;
        Ok("wallet creation successful".into())
    }

    fn key_gen(&self) -> Result<String> {
        debug!(target: "RPC USER ADAPTER", "key_gen() [START]");
        let (public, private) = self.wallet.key_gen();
        debug!(target: "RPC USER ADAPTER", "Created keypair...");
        debug!(target: "RPC USER ADAPTER", "Attempting to write to database...");
        self.wallet.put_keypair(public, private)?;
        Ok("key generation successful".into())
    }

    fn transfer(&self, pub_key: String, amount: f64) -> BoxFuture<Result<String>> {
        debug!(target: "RPC USER ADAPTER", "transfer() [START]");
        Self::transfer_process(self.client.clone(), self.wallet.clone(), pub_key, amount).boxed()
    }

    fn withdraw(&self, pub_key: String, amount: f64) -> BoxFuture<Result<String>> {
        debug!(target: "RPC USER ADAPTER", "withdraw() [START]");
        Self::withdraw_process(
            self.client.clone(),
            self.cashier_client.clone(),
            self.wallet.clone(),
            pub_key,
            amount,
        )
        .boxed()
    }

    fn deposit(&self) -> BoxFuture<Result<String>> {
        debug!(target: "RPC USER ADAPTER", "deposit() [START]");
        Self::deposit_process(self.cashier_client.clone(), self.wallet.clone()).boxed()
    }
}
