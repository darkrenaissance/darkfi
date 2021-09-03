use crate::cli::{TransferParams, WithdrawParams};
use crate::client::ClientResult;
use crate::serial::serialize;
use crate::service::btc::PubAddress;
use crate::wallet::WalletDb;
use crate::{Error, Result};

use log::*;

use async_std::sync::Arc;

pub type UserAdapterPtr = Arc<UserAdapter>;

pub type TransferChannel = (
    async_channel::Sender<TransferParams>,
    async_channel::Receiver<ClientResult<()>>,
);

pub type DepositChannel = (
    async_channel::Sender<jubjub::SubgroupPoint>,
    async_channel::Receiver<ClientResult<bitcoin::util::address::Address>>,
);

pub type WithdrawChannel = (
    async_channel::Sender<String>,
    async_channel::Receiver<ClientResult<jubjub::SubgroupPoint>>,
);

pub struct UserAdapter {
    pub wallet: Arc<WalletDb>,
    transfer_channel: TransferChannel,
    deposit_channel: DepositChannel,
    withdraw_channel: WithdrawChannel,
}

impl UserAdapter {
    pub fn new(
        wallet: Arc<WalletDb>,
        transfer_channel: TransferChannel,
        deposit_channel: DepositChannel,
        withdraw_channel: WithdrawChannel,
    ) -> Result<Self> {
        debug!(target: "RPC USER ADAPTER", "new() [CREATING NEW WALLET]");
        Ok(Self {
            wallet,
            transfer_channel,
            deposit_channel,
            withdraw_channel,
        })
    }

    pub fn handle_input(self: Arc<Self>) -> Result<jsonrpc_core::IoHandler> {
        let mut io = jsonrpc_core::IoHandler::new();

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
                let amount = parsed.amount;
                let address = self2.withdraw(parsed).await?;
                Ok(jsonrpc_core::Value::String(format!(
                    "sending {} dbtc to provided address for withdrawing: {} ",
                    amount, address
                )))
            }
        });

        Ok(io)
    }

    pub fn init_db(&self) -> Result<()> {
        debug!(target: "RPC USER ADAPTER", "init_db() [START]");
        self.wallet.init_db()?;
        Ok(())
    }

    pub fn key_gen(&self) -> Result<()> {
        debug!(target: "RPC USER ADAPTER", "key_gen() [START]");
        let (public, private) = self.wallet.key_gen();
        debug!(target: "RPC USER ADAPTER", "Created keypair...");
        debug!(target: "RPC USER ADAPTER", "Attempting to write to database...");
        self.wallet.put_keypair(public, private)?;
        Ok(())
    }

    pub fn get_key(&self) -> Result<String> {
        debug!(target: "RPC USER ADAPTER", "get_key() [START]");
        let key_public = self.wallet.get_public()?;
        let bs58_address = bs58::encode(serialize(&key_public)).into_string();
        Ok(bs58_address)
    }

    pub fn get_cash_public(&self) -> Result<String> {
        debug!(target: "RPC USER ADAPTER", "get_cash_public() [START]");
        let cashier_public = self.wallet.get_cashier_public()?;
        let bs58_address = bs58::encode(serialize(&cashier_public)).into_string();
        Ok(bs58_address)
    }

    pub async fn deposit(&self) -> Result<PubAddress> {
        debug!(target: "RPC USER ADAPTER", "deposit: START");
        let (public, private) = self.wallet.key_gen();
        self.wallet.put_keypair(public, private)?;
        let dkey = self.wallet.get_public()?;
        self.deposit_channel.0.send(dkey).await?;
        self.deposit_channel
            .1
            .recv()
            .await?
            .map_err(|err| Error::from(err))
    }

    async fn transfer(&self, transfer_params: TransferParams) -> Result<()> {
        self.transfer_channel.0.send(transfer_params).await?;

        self.transfer_channel
            .1
            .recv()
            .await?
            .map_err(|err| Error::from(err))
    }

    async fn withdraw(&self, withdraw_params: WithdrawParams) -> Result<String> {
        debug!(target: "RPC USER ADAPTER", "withdraw: START");
        self.withdraw_channel
            .0
            .send(withdraw_params.pub_key)
            .await?;

        // receive dbtc address
        let key = self
            .withdraw_channel
            .1
            .recv()
            .await?
            .map_err(|err| Error::from(err))?;

        // transfer the dbtc
        let key = bs58::encode(serialize(&key)).into_string();
        let mut transfer_params = TransferParams::new();
        transfer_params.pub_key = key.clone();
        transfer_params.amount = withdraw_params.amount;
        self.transfer_channel.0.send(transfer_params).await?;
        self.transfer_channel
            .1
            .recv()
            .await?
            .map_err(|err| Error::from(err))?;

        Ok(key)
    }

    pub fn get_info(&self) {}

    pub fn say_hello(&self) {}

    pub fn stop(&self) {}
}
