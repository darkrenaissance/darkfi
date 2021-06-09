use crate::wallet::WalletDB;
use crate::Result;
use log::*;
use std::path::PathBuf;
use std::sync::Arc;

// Dummy adapter for now
pub struct RpcAdapter {}

impl RpcAdapter {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {})
    }

    pub async fn key_gen() -> Result<()> {
        debug!(target: "adapter", "key_gen() [START]");
        let (public, private) = WalletDB::create_key().await;
        let path = WalletDB::path("wallet.db").expect("Failed to get path");
        WalletDB::save_key(path, public, private)
            .await
            .expect("Failed to save key");
        Ok(())
    }

    pub async fn new_wallet() -> Result<()> {
        debug!(target: "adapter", "new_wallet() [START]");
        let path = WalletDB::path("wallet.db").expect("Failed to get path");
        WalletDB::new(path).await?;
        Ok(())
    }

    pub async fn new_cashier_wallet() -> Result<()> {
        debug!(target: "adapter", "new_cashier_wallet() [START]");
        let path = WalletDB::path("cashier.db").expect("Failed to get path");
        WalletDB::new(path).await?;
        Ok(())
    }

    pub async fn save_cash_key(pubkey: Vec<u8>) -> Result<()> {
        debug!(target: "adapter", "save_cash_key() [START]");
        let path = WalletDB::path("cashier.db").expect("Failed to get path");
        WalletDB::save(path, pubkey).await?;
        Ok(())
    }

    pub async fn get_key() -> Result<()> {
        debug!(target: "adapter", "get_key() [START]");
        let path = WalletDB::path("wallet.db").expect("Failed to get path");
        WalletDB::get(path).await?;
        Ok(())
    }

    pub async fn get_cash_key() -> Result<()> {
        debug!(target: "adapter", "get_cash_key() [START]");
        let path = WalletDB::path("cashier.db").expect("Failed to get path");
        WalletDB::get(path).await?;
        Ok(())
    }
    pub async fn save_key(pubkey: Vec<u8>) -> Result<()> {
        debug!(target: "adapter", "save_key() [START]");
        let path = WalletDB::path("wallet.db").expect("Failed to get path");
        WalletDB::save(path, pubkey).await?;
        Ok(())
    }

    pub async fn get_info() {}

    pub async fn say_hello() {}

    pub async fn stop() {}
}
