use async_std::sync::{Arc, Mutex};
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use lazy_init::Lazy;
use log::{debug, error, info};

use super::state::{state_transition, State};
use crate::{
    crypto::{
        address::Address,
        coin::Coin,
        keypair::{Keypair, PublicKey},
        merkle_node::MerkleNode,
        proof::ProvingKey,
        types::DrkTokenId,
        OwnCoin,
    },
    tx::{
        builder::{
            TransactionBuilder, TransactionBuilderClearInputInfo, TransactionBuilderInputInfo,
            TransactionBuilderOutputInfo,
        },
        Transaction,
    },
    util::serial::Encodable,
    wallet::walletdb::{Balances, WalletPtr},
    zk::circuit::MintContract,
    ClientFailed, ClientResult, Result,
};

/// The Client structure, used for transaction operations.
/// This includes, receiving, broadcasting, and building.
pub struct Client {
    pub main_keypair: Mutex<Keypair>,
    pub wallet: WalletPtr,
    mint_pk: Lazy<ProvingKey>,
    burn_pk: Lazy<ProvingKey>,
}

impl Client {
    pub async fn new(wallet: WalletPtr) -> Result<Self> {
        // Initialize or load the wallet
        wallet.init_db().await?;

        // Get default keypair or create one
        let main_keypair = wallet.get_default_keypair_or_create_one().await?;
        info!(target: "client", "Main keypair: {}", Address::from(main_keypair.public));

        // Generate merkle tree if we don't have one.
        // TODO: See what to do about this
        if wallet.get_tree().await.is_err() {
            wallet.tree_gen().await?;
        }

        Ok(Self {
            main_keypair: Mutex::new(main_keypair),
            wallet,
            mint_pk: Lazy::new(),
            burn_pk: Lazy::new(),
        })
    }

    // TODO: Better function name
    async fn build_slab_from_tx(
        &self,
        pubkey: PublicKey,
        value: u64,
        token_id: DrkTokenId,
        clear_input: bool,
        state: Arc<Mutex<State>>,
    ) -> ClientResult<(Transaction, Vec<Coin>)> {
        debug!("build_slab_from_tx(): Begin building slab from tx");
        let mut clear_inputs = vec![];
        let mut inputs = vec![];
        let mut outputs = vec![];
        let mut coins = vec![];

        if clear_input {
            debug!("build_slab_from_tx(): Building clear input");
            let signature_secret = self.main_keypair.lock().await.secret;
            let input = TransactionBuilderClearInputInfo { value, token_id, signature_secret };
            clear_inputs.push(input);
        } else {
            debug!("build_slab_from_tx(): Building tx inputs");
            let mut inputs_value = 0;
            let state_m = state.lock().await;
            let own_coins = self.wallet.get_own_coins().await?;

            for own_coin in own_coins.iter() {
                if inputs_value >= value {
                    debug!("build_slab_from_tx(): inputs_value >= value");
                    break
                }

                let leaf_position = own_coin.leaf_position;
                let merkle_path = state_m.tree.authentication_path(leaf_position).unwrap();
                inputs_value += own_coin.note.value;

                let input = TransactionBuilderInputInfo {
                    leaf_position,
                    merkle_path,
                    secret: own_coin.secret,
                    note: own_coin.note,
                };

                inputs.push(input);
                coins.push(own_coin.coin);
            }
            // Release state lock
            drop(state_m);

            if inputs_value < value {
                error!("build_slab_from_tx(): Not enough value to build tx inputs");
                return Err(ClientFailed::NotEnoughValue(inputs_value))
            }

            if inputs_value > value {
                let return_value = inputs_value - value;
                outputs.push(TransactionBuilderOutputInfo {
                    value: return_value,
                    token_id,
                    public: self.main_keypair.lock().await.public,
                });
            }

            debug!("build_slab_from_tx(): Finished building inputs");
        }

        outputs.push(TransactionBuilderOutputInfo { value, token_id, public: pubkey });
        let builder = TransactionBuilder { clear_inputs, inputs, outputs };
        let mut tx_data = vec![];

        let mint_pk = self.mint_pk.get_or_create(Client::build_mint_pk);
        let burn_pk = self.burn_pk.get_or_create(Client::build_burn_pk);
        let tx = builder.build(mint_pk, burn_pk)?;
        tx.encode(&mut tx_data)?;

        // Check if state transition is valid before broadcasting
        debug!("build_slab_from_tx(): Checking if state transition is valid");
        let state = &*state.lock().await;
        debug!("build_slab_from_tx(): Got state lock");
        state_transition(state, tx.clone())?;
        debug!("build_slab_from_tx(): Successful state transition");

        Ok((tx, coins))
    }

    /// Build a transaction given the required parameters and state machine.
    pub async fn build_transaction(
        &self,
        pubkey: PublicKey,
        amount: u64,
        token_id: DrkTokenId,
        clear_input: bool,
        state: Arc<Mutex<State>>,
    ) -> ClientResult<Transaction> {
        // TODO: Token id debug
        debug!("send(): Sending {}", amount);

        if amount == 0 {
            return Err(ClientFailed::InvalidAmount(0))
        }

        if !self.wallet.token_id_exists(token_id).await? && !clear_input {
            return Err(ClientFailed::NotEnoughValue(amount))
        }

        let (tx, coins) =
            self.build_slab_from_tx(pubkey, amount, token_id, clear_input, state).await?;
        for coin in coins.iter() {
            // TODO: This should be more robust. In case our transaction is denied,
            // we want to revert to be able to send again.
            self.wallet.confirm_spend_coin(coin).await?;
        }

        debug!("send(): Sent {}", amount);
        Ok(tx)
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

    pub async fn get_keypairs(&self) -> Result<Vec<Keypair>> {
        self.wallet.get_keypairs().await
    }

    pub async fn put_keypair(&self, keypair: &Keypair) -> Result<()> {
        self.wallet.put_keypair(keypair).await
    }

    pub async fn set_default_keypair(&self, public: &PublicKey) -> Result<()> {
        let kp = self.wallet.set_default_keypair(public).await?;
        let mut mk = self.main_keypair.lock().await;
        *mk = kp;
        drop(mk);
        Ok(())
    }

    pub async fn keygen(&self) -> Result<Address> {
        let kp = self.wallet.keygen().await?;
        Ok(Address::from(kp.public))
    }

    pub async fn get_balances(&self) -> Result<Balances> {
        self.wallet.get_balances().await
    }

    pub async fn get_tree(&self) -> Result<BridgeTree<MerkleNode, 32>> {
        self.wallet.get_tree().await
    }

    fn build_mint_pk() -> ProvingKey {
        ProvingKey::build(11, &MintContract::default())
    }

    fn build_burn_pk() -> ProvingKey {
        ProvingKey::build(11, &MintContract::default())
    }
}
