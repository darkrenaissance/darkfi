use crate::Result;
use std::path::PathBuf;
use log::*;
use std::sync::Arc;
use crate::wallet::walletdb::WalletDB;

// Dummy adapter for now
pub struct RpcAdapter {}

impl RpcAdapter {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {})
    }

    pub async fn key_gen() -> Result<()> {
        debug!(target: "adapter", "key_gen() [START]");
        let path = dirs::home_dir()
            .expect("cannot find home directory.")
            .as_path()
            .join(".config/darkfi/wallet.db");
        WalletDB::key_gen(path).await?;
        Ok(())
    }

    // user input should define wallet path
    pub async fn new_wallet() -> Result<()> {
        debug!(target: "adapter", "new_wallet() [START]");
        let path = dirs::home_dir()
            .expect("cannot find home directory.")
            .as_path()
            .join(".config/darkfi/wallet.db");
        WalletDB::new(path).await?;
        Ok(())
    }

    pub async fn new_cashier_wallet() -> Result<()> {
        let path = dirs::home_dir()
            .expect("cannot find home directory.")
            .as_path()
            .join(".config/darkfi/cashier.db");
        debug!(target: "adapter", "new_wallet() [START]");
        WalletDB::new(path).await?;
        Ok(())
    }

    pub async fn save_cash_key(pubkey: Vec<u8>) -> Result<()> {
        let path = dirs::home_dir()
            .expect("cannot find home directory.")
            .as_path()
            .join(".config/darkfi/cashier.db");
        debug!(target: "adapter", "new_wallet() [START]");
        WalletDB::save(path, pubkey).await?;
        Ok(())

    }

    pub async fn save_key(pubkey: Vec<u8>) -> Result<()> {
        let path = dirs::home_dir()
            .expect("cannot find home directory.")
            .as_path()
            .join(".config/darkfi/wallet.db");
        debug!(target: "adapter", "new_wallet() [START]");
        WalletDB::save(path, pubkey).await?;
        Ok(())

    }

    pub fn wallet_path() -> PathBuf {
        debug!(target: "wallet_path", "Finding wallet path...");
        let path = dirs::home_dir()
            .expect("cannot find home directory.")
            .as_path()
            .join(".config/darkfi/wallet.db");
        path
    }

    pub async fn get_info() {}

    pub async fn say_hello() {}

    pub async fn stop() {}
}

