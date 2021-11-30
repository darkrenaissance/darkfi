use async_std::sync::{Arc, Mutex};
use incrementalmerkletree::Tree;
use log::{debug, info, warn};
use pasta_curves::pallas;
use smol::Executor;
use url::Url;

use crate::{
    blockchain::{rocks::columns, Rocks, RocksColumn, Slab},
    crypto::{coin::Coin, merkle_node::MerkleNode, schnorr, util::mod_r_p},
    serial::{serialize, Decodable, Encodable},
    service::GatewayClient,
    state::{state_transition, State},
    tx,
    wallet::{CashierDbPtr, Keypair, WalletPtr},
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
}

impl Client {
    pub async fn new(
        rocks: Arc<Rocks>,
        gateway_addrs: (Url, Url),
        wallet: WalletPtr,
    ) -> Result<Self> {
        wallet.init_db().await?;

        // Generate a new keypair if we don't have any.
        if wallet.get_keypairs().await?.is_empty() {
            wallet.key_gen().await?;
        }

        // TODO: Think about multiple keypairs
        let main_keypair = wallet.get_keypairs().await?[0].clone();
        info!("Main keypair: {}", bs58::encode(&serialize(&main_keypair.public)).into_string());

        debug!("Creating GatewayClient");
        let slabstore = RocksColumn::<columns::Slabs>::new(rocks);
        let gateway = GatewayClient::new(gateway_addrs.0, gateway_addrs.1, slabstore)?;

        let client = Client { main_keypair, gateway, wallet };
        Ok(client)
    }

    pub async fn start(&mut self) -> Result<()> {
        self.gateway.start().await
    }

    async fn build_slab_from_tx(
        &mut self,
        pubkey: pallas::Point,
        value: u64,
        token_id: pallas::Base,
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
            let base_secret = self.main_keypair.private;
            let signature_secret = schnorr::SecretKey(mod_r_p(base_secret));
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

                let node = MerkleNode(own_coin.coin.inner());
                let (leaf_position, merkle_path) = state_m.tree.authentication_path(&node).unwrap();
                // TODO: What is this counting? Is it everything or does it know to separate
                // different tokens?
                inputs_value += own_coin.note.value;

                let input = tx::TransactionBuilderInputInfo {
                    leaf_position,
                    merkle_path,
                    secret: own_coin.secret,
                    note: own_coin.note.clone(),
                };

                inputs.push(input);
                coins.push(own_coin.coin.clone());
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
        tx = builder.build()?;
        tx.encode(&mut tx_data).expect("encode tx");

        let slab = Slab::new(tx_data);
        debug!("End build slab from tx");

        // Check if it's valid before sending to gateway
        let state = &*state.lock().await;
        state_transition(state, tx)?;

        self.gateway.put_slab(slab).await?;
        Ok(coins)
    }

    pub async fn send(
        &mut self,
        pubkey: pallas::Point,
        amount: u64,
        token_id: pallas::Base,
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

    async fn update_state(
        secret_keys: Vec<pallas::Base>,
        slab: &Slab,
        state: Arc<Mutex<State>>,
        wallet: WalletPtr,
        notify: Option<async_channel::Sender<(pallas::Point, u64)>>,
    ) -> Result<()> {
        debug!("Build tx from slab and update the state");
        let tx = tx::Transaction::decode(&slab.get_payload()[..])?;

        let st = &*state.lock().await;
        let update = state_transition(st, tx)?;
        let mut st = state.lock().await;
        st.apply(update, secret_keys, notify, wallet).await?;
        Ok(())
    }

    pub async fn connect_to_subscriber_from_cashier(
        &self,
        state: Arc<Mutex<State>>,
        cashier_wallet: CashierDbPtr,
        notify: async_channel::Sender<(pallas::Point, u64)>,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        debug!("Start subscriber for cashier");
        let gateway_slabs_sub = self.gateway.start_subscriber(executor.clone()).await?;

        let secret_key = self.main_keypair.private;
        let wallet = self.wallet.clone();

        //let task: smol::Task<Result<()>> = executor.spawn(async move {
        let task: smol::Task<Result<()>> = executor.spawn(async move {
            loop {
                let slab = gateway_slabs_sub.recv().await?;
                debug!("Received new slab");

                let mut secret_keys: Vec<pallas::Base> = vec![secret_key];
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
}
