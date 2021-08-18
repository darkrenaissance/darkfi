use crate::cli::{TransferParams, WithdrawParams};
use crate::serial::serialize;
use crate::service::btc::PubAddress;
use crate::wallet::WalletDb;
use crate::{Error, Result};
use std::string::ToString;

use log::*;

use async_std::sync::Arc;

pub type UserAdapterPtr = Arc<UserAdapter>;
pub type DepositChannel = (
    async_channel::Sender<jubjub::SubgroupPoint>,
    async_channel::Receiver<Option<bitcoin::util::address::Address>>,
);
pub type WithdrawChannel = (
    async_channel::Sender<String>,
    async_channel::Receiver<Option<jubjub::SubgroupPoint>>,
);

pub struct UserAdapter {
    pub wallet: Arc<WalletDb>,
    publish_tx_send: async_channel::Sender<TransferParams>,
    deposit_channel: DepositChannel,
    withdraw_channel: WithdrawChannel,
}

impl UserAdapter {
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

    pub fn handle_input(
        self: Arc<Self>,
        mut io: jsonrpc_core::IoHandler,
    ) -> Result<jsonrpc_core::IoHandler> {
        io.add_sync_method("say_hello", |_| {
            Ok(jsonrpc_core::Value::String("hello world!".into()))
        });
        let self1 = self.clone();
        io.add_method("get_key", move |_| {
            let self2 = self1.clone();
            async move {
                let pub_key = self2.get_key()?;
                Ok(jsonrpc_core::Value::String(pub_key))
            }
        });

        let self1 = self.clone();
        io.add_method("get_cash_public", move |_| {
            let self2 = self1.clone();
            async move {
                let cash_key = self2.get_cash_public()?;
                Ok(jsonrpc_core::Value::String(cash_key))
            }
        });

        let self1 = self.clone();
        io.add_method("get_info", move |_| {
            let self2 = self1.clone();
            async move {
                self2.get_info();
                Ok(jsonrpc_core::Value::Null)
            }
        });

        let self1 = self.clone();
        io.add_method("stop", move |_| {
            let self2 = self1.clone();
            async move {
                self2.stop();
                Ok(jsonrpc_core::Value::Null)
            }
        });
        let self1 = self.clone();

        io.add_method("create_wallet", move |_| {
            let self2 = self1.clone();
            async move {
                self2.init_db()?;
                Ok(jsonrpc_core::Value::String(
                    "wallet creation successful".into(),
                ))
            }
        });

        let self1 = self.clone();
        io.add_method("key_gen", move |_| {
            let self2 = self1.clone();
            async move {
                self2.key_gen()?;
                Ok(jsonrpc_core::Value::String(
                    "key generation successful".into(),
                ))
            }
        });

        let self1 = self.clone();
        io.add_method("deposit", move |_| {
            let self2 = self1.clone();
            async move {
                let btckey = self2.deposit().await?;
                Ok(jsonrpc_core::Value::String(format!("{}", btckey)))
            }
        });

        let self1 = self.clone();
        io.add_method("transfer", move |params: jsonrpc_core::Params| {
            let self2 = self1.clone();
            async move {
                let parsed: TransferParams = params.parse().unwrap();
                let amount = parsed.amount.clone();
                let address = parsed.pub_key.clone();
                self2.transfer(parsed).await?;
                Ok(jsonrpc_core::Value::String(format!(
                    "transfered {} DRK to {}",
                    amount, address
                )))
            }
        });

        let self1 = self.clone();
        io.add_method("withdraw", move |params: jsonrpc_core::Params| {
            let self2 = self1.clone();
            async move {
                let parsed: WithdrawParams = params.parse().unwrap();
                let amount = parsed.amount.clone();
                let address = parsed.pub_key.clone();
                self2.withdraw(parsed).await?;
                Ok(jsonrpc_core::Value::String(format!(
                    "withdrawing {} BTC to {}...",
                    amount, address
                )))
            }
        });

        Ok(io)
    }

    pub fn init_db(&self) -> Result<()> {
        debug!(target: "adapter", "init_db() [START]");
        self.wallet.init_db()?;
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

    pub fn get_key(&self) -> Result<String> {
        debug!(target: "adapter", "get_key() [START]");
        let key_public = self.wallet.get_public()?;
        let bs58_address = bs58::encode(serialize(&key_public)).into_string();
        Ok(bs58_address)
    }

    pub fn get_cash_public(&self) -> Result<String> {
        debug!(target: "adapter", "get_cash_public() [START]");
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
        debug!(target: "withdraw", "withdraw: START");
        // do the key exchange
        self.withdraw_channel.0.send(withdraw_params.pub_key).await?;
        // send the drk
        if let Some(key) = self.withdraw_channel.1.recv().await? {
            let mut transfer_params = TransferParams::new();
            transfer_params.pub_key = key.to_string();
            transfer_params.amount = withdraw_params.amount;
            self.publish_tx_send.send(transfer_params).await?;
        }
        Ok(())
    }

    pub fn get_info(&self) {}

    pub fn say_hello(&self) {}

    pub fn stop(&self) {}
}
