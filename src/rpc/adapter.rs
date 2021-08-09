use crate::cli::{TransferParams, WithdrawParams};
use crate::serial::serialize;
use crate::service::btc::PubAddress;
use crate::wallet::WalletDb;
use crate::{Error, Result};

use log::*;

use async_std::sync::Arc;

pub type AdapterPtr = Arc<RpcAdapter>;
pub type DepositChannel = (
    async_channel::Sender<jubjub::SubgroupPoint>,
    async_channel::Receiver<Option<bitcoin::util::address::Address>>,
);
pub type WithdrawChannel = (
    async_channel::Sender<WithdrawParams>,
    async_channel::Receiver<jubjub::SubgroupPoint>,
);

pub struct RpcAdapter {
    pub wallet: Arc<WalletDb>,
    publish_tx_send: async_channel::Sender<TransferParams>,
    deposit_channel: DepositChannel,
    withdraw_channel: WithdrawChannel,
}

impl RpcAdapter {
    pub fn new(
        wallet: Arc<WalletDb>,
        publish_tx_send: async_channel::Sender<TransferParams>,
        deposit_channel: DepositChannel,
        withdraw_channel: WithdrawChannel,
    ) -> Result<Self> {
        debug!(target: "ADAPTER", "new() [CREATING NEW WALLET]");
        Ok(Self {
            wallet,
            publish_tx_send,
            deposit_channel,
            withdraw_channel,
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

    pub fn get_key(&self) -> Result<String> {
        debug!(target: "adapter", "get_key() [START]");
        let key_public = self.wallet.get_public()?;
        let bs58_address = bs58::encode(serialize(&key_public)).into_string();
        Ok(bs58_address)
    }

    pub fn get_cash_key(&self) -> Result<String> {
        debug!(target: "adapter", "get_cash_key() [START]");
        let cashier_public = self.wallet.get_cashier_public()?;
        let bs58_address = bs58::encode(serialize(&cashier_public)).into_string();
        Ok(bs58_address)
    }

    pub async fn deposit(&self) -> Result<PubAddress> {
        debug!(target: "deposit", "deposit: START");
        let (public, private) = self.wallet.key_gen();
        self.wallet.put_keypair(public, private)?;
        let dkey = self.wallet.get_public()?;
        self.deposit_channel.0.send(dkey).await?;
        match self.deposit_channel.1.recv().await? {
            Some(key) => Ok(key),
            None => Err(Error::CashierNoReply),
        }
    }

    pub async fn transfer(&self, transfer_params: TransferParams) -> Result<()> {
        self.publish_tx_send.send(transfer_params).await?;
        Ok(())
    }

    pub async fn withdraw(&self, withdraw_params: WithdrawParams) -> Result<()> {
        self.withdraw_channel.0.send(withdraw_params).await?;
        Ok(())
    }

    pub fn get_info(&self) {}

    pub fn say_hello(&self) {}

    pub fn stop(&self) {}
}
