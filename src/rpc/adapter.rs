use crate::wallet::WalletDB;
use crate::Result;
use log::*;
use std::sync::Arc;

// there should
// Dummy adapter for now
pub struct RpcAdapter {
    wallet: Arc<WalletDB>,
}

impl RpcAdapter {
    pub fn new(dbname: &str) -> Result<Arc<Self>> {
        let wallet = WalletDB::new(dbname)?;
        Ok(Arc::new(Self { wallet }))
    }

    //pub async fn get_path(&self) -> Result<PathBuf> {
    //}

    pub async fn key_gen(&self) -> Result<()> {
        debug!(target: "adapter", "key_gen() [START]");
        let (public, private) = self.wallet.key_gen().await;
        self.wallet.put_key(public, private).await?;
        Ok(())
    }

    pub async fn cash_key_gen(&self) -> Result<()> {
        debug!(target: "adapter", "key_gen() [START]");
        //let (public, private) = WalletDB::create_key().await;
        //let path = WalletDB::path("cashier.db")?;
        //WalletDB::save_key(path, public, private)
        //    .await
        //    .expect("Failed to save key");
        Ok(())
    }

    pub async fn get_key(&self) -> Result<()> {
        debug!(target: "adapter", "get_key() [START]");
        //let path = WalletDB::path("wallet.db")?;
        //WalletDB::get_public(path).await?;
        Ok(())
    }

    pub async fn get_cash_key(&self) -> Result<()> {
        debug!(target: "adapter", "get_cash_key() [START]");
        //let path = WalletDB::path("cashier.db")?;
        //let key = WalletDB::get_public(path).await?;
        //println!("{:?}", key);
        Ok(())
    }
    pub async fn save_key(&self, pubkey: Vec<u8>) -> Result<()> {
        debug!(target: "adapter", "save_key() [START]");
        //let path = WalletDB::path("wallet.db")?;
        //WalletDB::save(path, pubkey).await?;
        Ok(())
    }

    pub async fn save_cash_key(&self, pubkey: Vec<u8>) -> Result<()> {
        debug!(target: "adapter", "save_cash_key() [START]");
        //let path = WalletDB::path("cashier.db")?;
        //WalletDB::save(path, pubkey).await?;
        Ok(())
    }

    pub async fn get_info(&self) {}

    pub async fn say_hello(&self) {}

    pub async fn stop(&self) {}
}
