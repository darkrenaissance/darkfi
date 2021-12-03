use async_std::sync::{Arc, Mutex};
use incrementalmerkletree::Tree;
use log::{debug, info, warn};
use smol::Executor;
use url::Url;

use crate::{
    blockchain::{rocks::columns, Rocks, RocksColumn, Slab},
    circuit::{MintContract, SpendContract},
    crypto::{
        coin::Coin,
        keypair::{Keypair, PublicKey, SecretKey},
        merkle_node::MerkleNode,
        proof::ProvingKey,
        OwnCoin,
    },
    serial::{serialize, Decodable, Encodable},
    service::GatewayClient,
    state::{state_transition, State, StateUpdate},
    tx,
    types::DrkTokenId,
    wallet::{
        cashierdb::CashierDbPtr,
        walletdb::{Balances, WalletPtr},
    },
    Result,
};

#[derive(Debug, Clone, thiserror::Error)]
pub enum ClientFailed {
    #[error("Here is not enough value {0}")]
    NotEnoughValue(u64),
    #[error("Invalid Address  {0}")]
    InvalidAddress(String),
    #[error("Invalid Amount {0}")]
    InvalidAmount(u64),
    #[error("Unable to get deposit address")]
    UnableToGetDepositAddress,
    #[error("Unable to get withdraw address")]
    UnableToGetWithdrawAddress,
    #[error("Does not have cashier public key")]
    DoesNotHaveCashierPublicKey,
    #[error("Does not have keypair")]
    DoesNotHaveKeypair,
    #[error("Password is empty. Cannot create database")]
    EmptyPassword,
    #[error("Wallet already initalized")]
    WalletInitialized,
    #[error("Keypair already exists")]
    KeyExists,
    #[error("{0}")]
    ClientError(String),
    #[error("Verify error: {0}")]
    VerifyError(String),
}

pub type ClientResult<T> = std::result::Result<T, ClientFailed>;

impl From<super::error::Error> for ClientFailed {
    fn from(err: super::error::Error) -> ClientFailed {
        ClientFailed::ClientError(err.to_string())
    }
}

impl From<crate::state::VerifyFailed> for ClientFailed {
    fn from(err: crate::state::VerifyFailed) -> ClientFailed {
        ClientFailed::VerifyError(err.to_string())
    }
}

pub struct Client {
    pub main_keypair: Keypair,
    gateway: GatewayClient,
    wallet: WalletPtr,
    mint_pk: ProvingKey,
    spend_pk: ProvingKey,
}

impl Client {
    pub async fn new(
        rocks: Arc<Rocks>,
        gateway_addrs: (Url, Url),
        wallet: WalletPtr,
    ) -> Result<Self> {
        wallet.init_db().await?;

        // Generate a new keypair if we don't have any.
        if wallet.get_keypairs().await.is_err() {
            wallet.key_gen().await?;
        }

        // TODO: Think about multiple keypairs
        let main_keypair = wallet.get_keypairs().await?[0];
        info!("Main keypair: {}", bs58::encode(&serialize(&main_keypair.public)).into_string());

        debug!("Creating GatewayClient");
        let slabstore = RocksColumn::<columns::Slabs>::new(rocks);
        let gateway = GatewayClient::new(gateway_addrs.0, gateway_addrs.1, slabstore)?;

        // TODO: These should go to a better place.
        debug!("Building proving key for the mint contract...");
        let mint_pk = ProvingKey::build(11, MintContract::default());
        debug!("Building proving key for the spend contract...");
        let spend_pk = ProvingKey::build(11, SpendContract::default());

        let client = Client { main_keypair, gateway, wallet, mint_pk, spend_pk };
        Ok(client)
    }

    pub async fn start(&mut self) -> Result<()> {
        self.gateway.start().await
    }

    async fn build_slab_from_tx(
        &mut self,
        pubkey: PublicKey,
        value: u64,
        token_id: DrkTokenId,
        clear_input: bool,
        state: Arc<Mutex<State>>,
    ) -> ClientResult<Vec<Coin>> {
        debug!("Start build slab from tx");
        let mut clear_inputs: Vec<tx::TransactionBuilderClearInputInfo> = vec![];
        let mut inputs: Vec<tx::TransactionBuilderInputInfo> = vec![];
        let mut outputs: Vec<tx::TransactionBuilderOutputInfo> = vec![];
        let mut coins: Vec<Coin> = vec![];

        if clear_input {
            // TODO: FIXME:
            let signature_secret = self.main_keypair.secret;
            let input = tx::TransactionBuilderClearInputInfo { value, token_id, signature_secret };
            clear_inputs.push(input);
        } else {
            debug!("Start build inputs");
            let mut inputs_value = 0_u64;
            let state_m = state.lock().await;
            let own_coins = self.wallet.get_own_coins().await?;

            for own_coin in own_coins.iter() {
                if inputs_value >= value {
                    break
                }

                let node = MerkleNode(own_coin.coin.0);
                let (leaf_position, merkle_path) = state_m.tree.authentication_path(&node).unwrap();
                // TODO: What is this counting? Is it everything or does it know to separate
                // different tokens?
                inputs_value += own_coin.note.value;

                let input = tx::TransactionBuilderInputInfo {
                    leaf_position,
                    merkle_path,
                    secret: own_coin.secret,
                    note: own_coin.note,
                };

                inputs.push(input);
                coins.push(own_coin.coin);
            }

            if inputs_value < value {
                return Err(ClientFailed::NotEnoughValue(inputs_value))
            }

            if inputs_value > value {
                let return_value: u64 = inputs_value - value;

                outputs.push(tx::TransactionBuilderOutputInfo {
                    value: return_value,
                    token_id,
                    public: self.main_keypair.public,
                });
            }

            debug!("End build inputs");
        }

        outputs.push(tx::TransactionBuilderOutputInfo { value, token_id, public: pubkey });

        let builder = tx::TransactionBuilder { clear_inputs, inputs, outputs };
        let tx: tx::Transaction;
        let mut tx_data = vec![];
        tx = builder.build(&self.mint_pk, &self.spend_pk)?;
        tx.encode(&mut tx_data).expect("encode tx");

        let slab = Slab::new(tx_data);
        debug!("End build slab from tx");

        // Check if it's valid before sending to gateway
        let state = &*state.lock().await;
        state_transition(state, tx)?;

        debug!("Sending slab to gateway");
        self.gateway.put_slab(slab).await?;
        debug!("Sent successfully");
        Ok(coins)
    }

    pub async fn send(
        &mut self,
        pubkey: PublicKey,
        amount: u64,
        token_id: DrkTokenId,
        clear_input: bool,
        state: Arc<Mutex<State>>,
    ) -> ClientResult<()> {
        // TODO: TOKEN debug
        debug!("Start send {}", amount);

        if amount == 0 {
            return Err(ClientFailed::InvalidAmount(0))
        }

        let coins = self.build_slab_from_tx(pubkey, amount, token_id, clear_input, state).await?;
        for coin in coins.iter() {
            self.wallet.confirm_spend_coin(coin).await?;
        }

        debug!("End send {}", amount);
        Ok(())
    }

    pub async fn transfer(
        &mut self,
        token_id: DrkTokenId,
        pubkey: PublicKey,
        amount: u64,
        state: Arc<Mutex<State>>,
    ) -> ClientResult<()> {
        debug!("Start transfer {}", amount);
        let token_id_exists = self.wallet.token_id_exists(token_id).await?;

        if token_id_exists {
            self.send(pubkey, amount, token_id, false, state).await?;
        } else {
            return Err(ClientFailed::NotEnoughValue(amount))
        }

        debug!("End transfer {}", amount);
        Ok(())
    }

    async fn update_state(
        secret_keys: Vec<SecretKey>,
        slab: &Slab,
        state: Arc<Mutex<State>>,
        wallet: WalletPtr,
        notify: Option<async_channel::Sender<(PublicKey, u64)>>,
    ) -> Result<()> {
        debug!("Build tx from slab and update the state");
        let payload = slab.get_payload();
        /*
        use std::io::Write;
        let mut file = std::fs::File::create("/tmp/payload.txt")?;
        file.write_all(&payload)?;
        */
        debug!("Decoding payload");
        let tx = tx::Transaction::decode(&payload[..])?;

        let update: StateUpdate;

        // This is separate because otherwise the mutex is never unlocked.
        {
            debug!("Acquiring state lock");
            let state = &*state.lock().await;
            update = state_transition(state, tx)?;
            debug!("Successfully passed state_transition");
        }

        debug!("Acquiring state lock");
        let mut state = state.lock().await;
        debug!("Trying to apply the new state");
        state.apply(update, secret_keys, notify, wallet).await?;
        debug!("Successfully passed state.apply");

        Ok(())
    }

    pub async fn connect_to_subscriber_from_cashier(
        &self,
        state: Arc<Mutex<State>>,
        cashier_wallet: CashierDbPtr,
        notify: async_channel::Sender<(PublicKey, u64)>,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        debug!("Start subscriber for cashier");
        let gateway_slabs_sub = self.gateway.start_subscriber(executor.clone()).await?;

        let secret_key = self.main_keypair.secret;
        let wallet = self.wallet.clone();

        let task: smol::Task<Result<()>> = executor.spawn(async move {
            loop {
                let slab = gateway_slabs_sub.recv().await?;
                debug!("Received new slab");

                let mut secret_keys = vec![secret_key];
                let mut withdraw_keys = cashier_wallet.get_withdraw_private_keys().await?;
                secret_keys.append(&mut withdraw_keys);

                let update_state = Self::update_state(
                    secret_keys,
                    &slab,
                    state.clone(),
                    wallet.clone(),
                    Some(notify.clone()),
                )
                .await;

                if let Err(e) = update_state {
                    warn!("Update state: {}", e);
                    continue
                }
            }
        });

        task.detach();
        Ok(())
    }

    pub async fn connect_to_subscriber(
        &self,
        state: Arc<Mutex<State>>,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        debug!("Start subscriber for darkfid");
        let gateway_slabs_sub = self.gateway.start_subscriber(executor.clone()).await?;

        let secret_key = self.main_keypair.secret;
        let wallet = self.wallet.clone();

        let task: smol::Task<Result<()>> = executor.spawn(async move {
            loop {
                let slab = gateway_slabs_sub.recv().await?;
                debug!("Received new slab");

                let update_state = Self::update_state(
                    vec![secret_key],
                    &slab,
                    state.clone(),
                    wallet.clone(),
                    None,
                )
                .await;

                if let Err(e) = update_state {
                    warn!("Update state: {}", e);
                    continue
                }
            }
        });

        task.detach();
        Ok(())
    }

    pub async fn init_db(&self) -> Result<()> {
        self.wallet.init_db().await
    }

    pub async fn get_own_coins(&self) -> Result<Vec<OwnCoin>> {
        self.wallet.get_own_coins().await
    }

    pub async fn confirm_spend_coin(&self, coin: &Coin) -> Result<()> {
        self.wallet.confirm_spend_coin(coin).await
    }

    pub async fn key_gen(&self) -> Result<()> {
        self.wallet.key_gen().await
    }

    pub async fn get_balances(&self) -> Result<Balances> {
        self.wallet.get_balances().await
    }
}
