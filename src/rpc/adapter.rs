use std::path::{Path, PathBuf};
use log::*;
use std::sync::Arc;
use crate::wallet::WalletDB;
use crate::{Error, Result};

// Dummy adapter for now
pub struct RpcAdapter {}

impl RpcAdapter {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {})
    }

    pub async fn get_path(wallet: &str) -> Result<PathBuf> {
        debug!(target: "adapter", "TEST PATH [START]");
        let mut path = WalletDB::path().await.expect("Failed to get path");
        debug!(target: "adapter", "TEST PATH {:?}", path);
        path.push(wallet);
        Ok(path)
    }

    pub async fn key_gen() -> Result<PathBuf> {
        debug!(target: "adapter", "key_gen() [START]");
        let path = Self::get_path("wallet.db").await.expect("Failed to get path");
        //WalletDB::key_gen(path).await?;
        Ok(path)
    }

    pub async fn new_wallet() -> Result<()> {
        debug!(target: "adapter", "new_wallet() [START]");
        let path = Self::get_path("wallet.db").await.expect("Failed to get path");
        WalletDB::new(path).await?;
        Ok(())
    }

    pub async fn new_cashier_wallet() -> Result<()> {
        debug!(target: "adapter", "new_cashier_wallet() [START]");
        let path = Self::get_path("cashier.db").await.expect("Failed to get path");
        WalletDB::new(path).await?;
        Ok(())
    }

    pub async fn save_cash_key(pubkey: Vec<u8>) -> Result<()> {
        debug!(target: "adapter", "save_cash_key() [START]");
        let path = Self::get_path("cashier.db").await.expect("Failed to get path");
        WalletDB::save(path, pubkey).await?;
        Ok(())

    }

    pub async fn save_key(pubkey: Vec<u8>) -> Result<()> {
        debug!(target: "adapter", "save_key() [START]");
        let path = Self::get_path("wallet.db").await.expect("Failed to get path");
        WalletDB::save(path, pubkey).await?;
        Ok(())

    }

    pub async fn get_info() {}

    pub async fn say_hello() {}

    pub async fn stop() {}
}

