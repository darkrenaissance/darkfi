use crate::wallet::WalletDb;
use crate::Result;
use async_std::sync::Arc;
use log::*;
//use std::sync::Arc;

pub type AdapterPtr = Arc<RpcAdapter>;
// Dummy adapter for now
pub struct RpcAdapter {
    pub wallet: Arc<WalletDb>,
}

impl RpcAdapter {
    pub fn new(wallet: Arc<WalletDb>) -> Result<Self> {
        debug!(target: "ADAPTER", "new() [CREATING NEW WALLET]");
        Ok(Self { wallet })
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
        let (public, private) = self.wallet.key_gen();
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

    pub fn deposit(&self) -> Result<()> {
        debug!(target: "deposit", "deposit: START");
        let (public, private) = self.wallet.key_gen();
        self.wallet.put_keypair(public, private)?;
        Ok(())
    }
    //pub async fn walletdb(&self) -> WalletPtr {
    //    self.wallet.clone();
    //}

    //pub async fn create_
    //pub async fn save_key(&self, pubkey: Vec<u8>) -> Result<()> {
    //    debug!(target: "adapter", "save_key() [START]");
    //    //let path = WalletDb::path("wallet.db")?;
    //    //WalletDb::save(path, pubkey).await?;
    //    Ok(())
    //}

    //pub async fn save_cash_key(&self, pubkey: Vec<u8>) -> Result<()> {
    //    debug!(target: "adapter", "save_cash_key() [START]");
    //    //let path = WalletDb::path("cashier.db")?;
    //    //WalletDb::save(path, pubkey).await?;
    //    Ok(())
    //}

    pub fn get_info(&self) {}

    pub fn say_hello(&self) {}

    pub fn stop(&self) {}
}
