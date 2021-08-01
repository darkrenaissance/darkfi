use crate::service::btc::PubAddress;
use crate::service::cashier::CashierClient;
use crate::wallet::WalletDb;
use crate::{Error, Result};
use crate::tx;

use log::*;

use async_std::sync::Arc;
use std::net::SocketAddr;

pub type AdapterPtr = Arc<RpcAdapter>;
// Dummy adapter for now
pub struct RpcAdapter {
    pub wallet: Arc<WalletDb>,
    pub cashier_client: CashierClient,
    pub connect_url: String,

}

impl RpcAdapter {
    pub fn new(wallet: Arc<WalletDb>, connect_url: String) -> Result<Self> {
        debug!(target: "ADAPTER", "new() [CREATING NEW WALLET]");
        let connect_addr: SocketAddr = connect_url.parse().unwrap();
        let cashier_client = CashierClient::new(connect_addr)?;
        Ok(Self {
            wallet,
            cashier_client,
            connect_url,
        })
    }

    pub fn init_db(&self) -> Result<()> {
        debug!(target: "adapter", "init_db() [START]");
        self.wallet.init_db()?;
        Ok(())
    }

    pub fn init_cashier_db(&self) -> Result<()> {
        debug!(target: "adapter", "init_cashier_db() [START]");
        self.wallet.init_cashier_db()?;
        Ok(())
    }

    pub fn key_gen(&self) -> Result<()> {
        debug!(target: "adapter", "key_gen() [START]");
        let (public, private) = self.wallet.key_gen();
        debug!(target: "adapter", "Created keypair...");
        debug!(target: "adapter", "Attempting to write to database...");
        self.wallet.put_keypair(public, private)?;
        Ok(())
    }

    pub fn cash_key_gen(&self) -> Result<()> {
        debug!(target: "adapter", "key_gen() [START]");
        let (public, private) = self.wallet.cash_key_gen();
        self.wallet.put_keypair(public, private)?;
        Ok(())
    }

    pub fn get_key(&self) -> Result<()> {
        debug!(target: "adapter", "get_key() [START]");
        let key_public = self.wallet.get_public()?;
        println!("{:?}", key_public);
        Ok(())
    }

    pub fn get_cash_key(&self) -> Result<()> {
        debug!(target: "adapter", "get_cash_key() [START]");
        let cashier_public = self.wallet.get_cashier_public()?;
        println!("{:?}", cashier_public);
        Ok(())
    }

    pub fn test_wallet(&self) -> Result<()> {
        self.wallet.test_wallet()?;
        debug!(target: "adapter", "test wallet: START");
        Ok(())
    }

    pub async fn deposit(&mut self) -> Result<PubAddress> {
        debug!(target: "deposit", "deposit: START");
        let (public, private) = self.wallet.key_gen();
        self.wallet.put_keypair(public, private)?;
        let dkey = self.wallet.get_public()?;
        match self.cashier_client.get_address(dkey).await? {
            Some(key) => Ok(key),
            None => Err(Error::CashierNoReply),
        }
    }



    pub fn get_info(&self) {}

    pub fn say_hello(&self) {}

    pub fn stop(&self) {}
}
