use crate::Result;
use log::*;
use std::sync::Arc;
use crate::wallet::walletdb::DBInterface;

// Dummy adapter for now
pub struct RpcAdapter {}

impl RpcAdapter {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {})
    }

    pub async fn key_gen() -> Result<()> {
        debug!(target: "adapter", "key_gen() [START]");
        DBInterface::own_key_gen().await?;
        Ok(())
    }

    pub async fn new_wallet() -> Result<()> {
        debug!(target: "adapter", "new_wallet() [START]");
        DBInterface::new_wallet().await?;
        Ok(())
    }

    pub async fn new_cashier_wallet() -> Result<()> {
        debug!(target: "adapter", "new_wallet() [START]");
        DBInterface::new_cashier_wallet().await?;
        Ok(())
    }

    pub async fn save_key(pubkey: Vec<u8>) -> Result<()> {
        debug!(target: "adapter", "save_key() [START]");
        DBInterface::save_key(pubkey).await?;
        Ok(())

    }

    pub async fn get_info() {}

    pub async fn say_hello() {}

    pub async fn stop() {}
}

