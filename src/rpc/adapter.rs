use crate::wallet::WalletDB;
use crate::Result;
use log::*;
use std::sync::Arc;

// there should 
// Dummy adapter for now
pub struct RpcAdapter {
    wallet: Arc<WalletDB>
}

impl RpcAdapter {
    pub fn new(dbname: &str) -> Result<Arc<Self>> {
        let wallet = WalletDB::new(dbname)?;
        Ok(Arc::new(Self {
            wallet
        }))
    }

    pub async fn new_wallet() -> Result<()> {
        debug!(target: "adapter", "new_wallet() [START]");
        //let path = WalletDB::path("wallet.db")?;
        //WalletDB::new(path).await?;
        Ok(())
    }

    pub async fn new_cash_wallet() -> Result<()> {
        debug!(target: "adapter", "new_cashier_wallet() [START]");
        //let path = WalletDB::path("cashier.db")?;
        //WalletDB::new(path).await?;
        Ok(())
    }

    pub async fn key_gen() -> Result<()> {
        debug!(target: "adapter", "key_gen() [START]");
        //let (public, private) = WalletDB::create_key().await;
        //let path = WalletDB::path("wallet.db")?;
        ////self.wallet.save_key()
        //WalletDB::save_key(path, public, private)
        //    .await
        //    .expect("Failed to save key");
        Ok(())
    }

    pub async fn cash_key_gen() -> Result<()> {
        debug!(target: "adapter", "key_gen() [START]");
        //let (public, private) = WalletDB::create_key().await;
        //let path = WalletDB::path("cashier.db")?;
        //WalletDB::save_key(path, public, private)
        //    .await
        //    .expect("Failed to save key");
        Ok(())
    }

    pub async fn get_key() -> Result<()> {
        debug!(target: "adapter", "get_key() [START]");
        //let path = WalletDB::path("wallet.db")?;
        //WalletDB::get_public(path).await?;
        Ok(())
    }

    pub async fn get_cash_key() -> Result<()> {
        debug!(target: "adapter", "get_cash_key() [START]");
        //let path = WalletDB::path("cashier.db")?;
        //let key = WalletDB::get_public(path).await?;
        //println!("{:?}", key);
        Ok(())
    }
    pub async fn save_key(pubkey: Vec<u8>) -> Result<()> {
        debug!(target: "adapter", "save_key() [START]");
        //let path = WalletDB::path("wallet.db")?;
        //WalletDB::save(path, pubkey).await?;
        Ok(())
    }

    pub async fn save_cash_key(pubkey: Vec<u8>) -> Result<()> {
        debug!(target: "adapter", "save_cash_key() [START]");
        //let path = WalletDB::path("cashier.db")?;
        //WalletDB::save(path, pubkey).await?;
        Ok(())
    }

    pub async fn get_info() {}

    pub async fn say_hello() {}

    pub async fn stop() {}
}
