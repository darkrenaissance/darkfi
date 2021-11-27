/*
use async_executor::Executor;
use async_std::sync::{Arc, Mutex};

use bellman::groth16;
use blake2s_simd::Params as Blake2sParams;
use bls12_381::Bls12;
use log::{debug, info, warn};
use url::Url;

use crate::{
    blockchain::{rocks::columns, Rocks, RocksColumn, Slab},
    crypto::{
        coin::Coin,
        merkle::{CommitmentTree, IncrementalWitness},
        merkle_node::MerkleNode,
        note::{EncryptedNote, Note},
        nullifier::Nullifier,
        OwnCoin,
    },
    serial::{serialize, Decodable, Encodable},
    service::{GatewayClient, GatewaySlabsSubscriber},
    state::{state_transition, ProgramState, StateUpdate},
    tx,
    wallet::{walletdb::Balances, CashierDbPtr, Keypair, WalletPtr},
    Result,
};
*/

#[derive(Debug, Clone)]
pub enum ClientFailed {
    NotEnoughValue(u64),
    InvalidAddress(String),
    InvalidAmount(u64),
    UnableToGetDepositAddress,
    UnableToGetWithdrawAddress,
    DoesNotHaveCashierPublicKey,
    DoesNotHaveKeypair,
    EmptyPassword,
    WalletInitialized,
    KeyExists,
    ClientError(String),
    VerifyError(String),
}

/*
pub struct Client {
    mint_params: bellman::groth16::Parameters<Bls12>,
    spend_params: bellman::groth16::Parameters<Bls12>,
    gateway: GatewayClient,
    wallet: WalletPtr,
    pub main_keypair: Keypair,
}

impl Client {
    pub async fn new(
        rocks: Arc<Rocks>,
        gateway_addrs: (Url, Url),
        wallet: WalletPtr,
        mint_params: bellman::groth16::Parameters<Bls12>,
        spend_params: bellman::groth16::Parameters<Bls12>,
    ) -> Result<Self> {
        wallet.init_db()?;

        if wallet.get_keypairs()?.is_empty() {
            wallet.key_gen()?;
        }

        let main_keypair = wallet.get_keypairs()?[0].clone();

        info!(
            target: "CLIENT", "Main Keypair: {}",
            bs58::encode(&serialize(&main_keypair.public)).into_string()
        );

        let slabstore = RocksColumn::<columns::Slabs>::new(rocks);

        // create gateway client
        debug!(target: "CLIENT", "Creating GatewayClient");
        let gateway = GatewayClient::new(gateway_addrs.0, gateway_addrs.1, slabstore)?;

        Ok(Self {
            mint_params,
            spend_params,
            wallet,
            gateway,
            main_keypair,
        })
    }

    pub async fn start(&mut self) -> Result<()> {
        self.gateway.start().await?;
        Ok(())
    }

    pub async fn transfer(
        &mut self,
        token_id: jubjub::Fr,
        pub_key: jubjub::SubgroupPoint,
        amount: u64,
        state: Arc<Mutex<State>>,
    ) -> ClientResult<()> {
        debug!(target: "CLIENT", "Start transfer {}", amount);

        let token_id_exists = self.wallet.token_id_exists(&token_id)?;

        if token_id_exists {
            self.send(pub_key, amount, token_id, false, state).await?;
        } else {
            return Err(ClientFailed::NotEnoughValue(amount));
        }

        debug!(target: "CLIENT", "End transfer {}", amount);

        Ok(())
    }

    pub async fn send(
        &mut self,
        pub_key: jubjub::SubgroupPoint,
        amount: u64,
        token_id: jubjub::Fr,
        clear_input: bool,
        state: Arc<Mutex<State>>,
    ) -> ClientResult<()> {
        debug!(target: "CLIENT", "Start send {}", amount);

        if amount == 0 {
            return Err(ClientFailed::InvalidAmount(amount as u64));
        }

        let coins = self
            .build_slab_from_tx(pub_key, amount, token_id, clear_input, state)
            .await?;

        for coin in coins.iter() {
            self.wallet.confirm_spend_coin(coin)?;
        }

        debug!(target: "CLIENT", "End send {}", amount);

        Ok(())
    }

    async fn build_slab_from_tx(
        &mut self,
        pub_key: jubjub::SubgroupPoint,
        value: u64,
        token_id: jubjub::Fr,
        clear_input: bool,
        state: Arc<Mutex<State>>,
    ) -> ClientResult<Vec<Coin>> {
        debug!(target: "CLIENT", "Start build slab from tx");

        let mut clear_inputs: Vec<tx::TransactionBuilderClearInputInfo> = vec![];
        let mut inputs: Vec<tx::TransactionBuilderInputInfo> = vec![];
        let mut outputs: Vec<tx::TransactionBuilderOutputInfo> = vec![];
        let mut coins: Vec<Coin> = vec![];

        if clear_input {
            let signature_secret = self.main_keypair.private;
            let input = tx::TransactionBuilderClearInputInfo {
                value,
                token_id,
                signature_secret,
            };
            clear_inputs.push(input);
        } else {
            debug!(target: "CLIENT", "Start build inputs");

            let mut inputs_value: u64 = 0;

            let own_coins = self.wallet.get_own_coins()?;

            for own_coin in own_coins.iter() {
                if inputs_value >= value {
                    break;
                }
                let witness = &own_coin.witness;
                let merkle_path = witness.path().unwrap();
                inputs_value += own_coin.note.value;

                let input = tx::TransactionBuilderInputInfo {
                    merkle_path,
                    secret: own_coin.secret,
                    note: own_coin.note.clone(),
                };

                inputs.push(input);
                coins.push(own_coin.coin.clone());
            }

            if inputs_value < value {
                return Err(ClientFailed::NotEnoughValue(inputs_value));
            }

            if inputs_value > value {
                let return_value: u64 = inputs_value - value;

                outputs.push(tx::TransactionBuilderOutputInfo {
                    value: return_value,
                    token_id,
                    public: self.main_keypair.public,
                });
            }

            debug!(target: "CLIENT", "End build inputs");
        }

        outputs.push(tx::TransactionBuilderOutputInfo {
            value,
            token_id,
            public: pub_key,
        });

        let builder = tx::TransactionBuilder {
            clear_inputs,
            inputs,
            outputs,
        };

        let tx: tx::Transaction;

        let mut tx_data = vec![];
        {
            tx = builder.build(&self.mint_params, &self.spend_params);
            tx.encode(&mut tx_data).expect("encode tx");
        }

        let slab = Slab::new(tx_data);

        debug!(target: "CLIENT", "End build slab from tx");

        // check if it's valid before send it to gateway
        let state = &*state.lock().await;

        state_transition(state, tx)?;

        self.gateway.put_slab(slab).await?;

        Ok(coins)
    }

    pub async fn connect_to_subscriber_from_cashier(
        &self,
        state: Arc<Mutex<State>>,
        cashier_wallet: CashierDbPtr,
        notify: async_channel::Sender<(jubjub::SubgroupPoint, u64)>,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        // start subscribing
        debug!(target: "CLIENT", "Start subscriber for cashier");
        let gateway_slabs_sub: GatewaySlabsSubscriber =
            self.gateway.start_subscriber(executor.clone()).await?;

        let secret_key = self.main_keypair.private;
        let wallet = self.wallet.clone();

        let task: smol::Task<Result<()>> = executor.spawn(async move {
            loop {
                let slab = gateway_slabs_sub.recv().await?;

                debug!(target: "CLIENT", "Received new slab");

                let mut secret_keys: Vec<jubjub::Fr> = vec![secret_key];
                let mut withdraw_keys = cashier_wallet.get_withdraw_private_keys()?;
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
                    warn!("Update state: {}", e.to_string());
                    continue;
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
        // start subscribing
        debug!(target: "CLIENT", "Start subscriber");
        let gateway_slabs_sub: GatewaySlabsSubscriber =
            self.gateway.start_subscriber(executor.clone()).await?;

        let secret_key = self.main_keypair.private;
        let wallet = self.wallet.clone();

        let task: smol::Task<Result<()>> = executor.spawn(async move {
            loop {
                let slab = gateway_slabs_sub.recv().await?;

                debug!(target: "CLIENT", "Received new slab");

                let update_state = Self::update_state(
                    vec![secret_key],
                    &slab,
                    state.clone(),
                    wallet.clone(),
                    None,
                )
                    .await;

                if let Err(e) = update_state {
                    warn!("Update state: {}", e.to_string());
                    continue;
                }
            }
        });

        task.detach();

        Ok(())
    }

    async fn update_state(
        secret_keys: Vec<jubjub::Fr>,
        slab: &Slab,
        state: Arc<Mutex<State>>,
        wallet: WalletPtr,
        notify: Option<async_channel::Sender<(jubjub::SubgroupPoint, u64)>>,
    ) -> Result<()> {
        debug!(target: "CLIENT", "Build tx from slab and update the state");

        let tx = tx::Transaction::decode(&slab.get_payload()[..])?;


        let update: StateUpdate;

        {
            let state = &*state.lock().await;
            update = state_transition(state, tx)?;
        }

        let mut state = state.lock().await;

        state
            .apply(update, secret_keys.clone(), notify, wallet)
            .await?;

        Ok(())
    }

    pub fn init_db(&self) -> Result<()> {
        self.wallet.init_db()
    }

    pub fn get_own_coins(&self) -> Result<Vec<OwnCoin>> {
        self.wallet.get_own_coins()
    }

    pub fn confirm_spend_coin(&self, coin: &Coin) -> Result<()> {
        self.wallet.confirm_spend_coin(coin)
    }

    pub fn key_gen(&self) -> Result<()> {
        self.wallet.key_gen()
    }

    pub fn get_balances(&self) -> Result<Balances> {
        self.wallet.get_balances()
    }

    pub fn token_id_exists(&self, token_id: &jubjub::Fr) -> Result<bool> {
        self.wallet.token_id_exists(token_id)
    }

    pub fn get_token_id(&self) -> Result<Vec<jubjub::Fr>> {
        self.wallet.get_token_id()
    }
}

pub struct State {
    // The entire merkle tree state
    pub tree: CommitmentTree<MerkleNode>,
    // List of all previous and the current merkle roots
    // This is the hashed value of all the children.
    pub merkle_roots: RocksColumn<columns::MerkleRoots>,
    // Nullifiers prevent double spending
    pub nullifiers: RocksColumn<columns::Nullifiers>,
    // Mint verifying key used by ZK
    pub mint_pvk: groth16::PreparedVerifyingKey<Bls12>,
    // Spend verifying key used by ZK
    pub spend_pvk: groth16::PreparedVerifyingKey<Bls12>,
    // List of cashier public keys
    pub public_keys: Vec<jubjub::SubgroupPoint>,
}

impl ProgramState for State {
    fn is_valid_cashier_public_key(&self, public: &jubjub::SubgroupPoint) -> bool {
        debug!(target: "CLIENT STATE", "Check if it is valid cashier public key");
        self.public_keys.contains(public)
    }

    fn is_valid_merkle(&self, merkle_root: &MerkleNode) -> bool {
        debug!(target: "CLIENT STATE", "Check if it is valid merkle");

        if let Ok(mr) = self.merkle_roots.key_exist(*merkle_root) {
            return mr;
        }
        false
    }

    fn nullifier_exists(&self, nullifier: &Nullifier) -> bool {
        debug!(target: "CLIENT STATE", "Check if nullifier exists");

        if let Ok(nl) = self.nullifiers.key_exist(nullifier.repr) {
            return nl;
        }
        false
    }

    // load from disk
    fn mint_pvk(&self) -> &groth16::PreparedVerifyingKey<Bls12> {
        &self.mint_pvk
    }

    fn spend_pvk(&self) -> &groth16::PreparedVerifyingKey<Bls12> {
        &self.spend_pvk
    }
}

impl State {
    pub async fn apply(
        &mut self,
        update: StateUpdate,
        secret_keys: Vec<jubjub::Fr>,
        notify: Option<async_channel::Sender<(jubjub::SubgroupPoint, u64)>>,
        wallet: WalletPtr,
    ) -> Result<()> {
        // Extend our list of nullifiers with the ones from the update

        debug!(target: "CLIENT STATE", "Extend nullifiers");
        for nullifier in update.nullifiers {
            self.nullifiers.put(nullifier, vec![] as Vec<u8>)?;
        }

        debug!(target: "CLIENT STATE", "Update merkle tree and witness ");
        // Update merkle tree and witnesses
        for (coin, enc_note) in update.coins.into_iter().zip(update.enc_notes.iter()) {
            // Add the new coins to the merkle tree
            let node = MerkleNode::from_coin(&coin);
            self.tree.append(node).expect("Append to merkle tree");

            debug!(target: "CLIENT STATE", "Keep track of all merkle roots");

            // Keep track of all merkle roots that have existed
            self.merkle_roots.put(self.tree.root(), vec![] as Vec<u8>)?;

            debug!(target: "CLIENT STATE", "Update witness");

            // Also update all the coin witnesses
            let mut updated_witnesses = wallet.get_witnesses()?;

            updated_witnesses.iter_mut().for_each(|(_, witness)| {
                witness.append(node).expect("Append to witness");
            });

            wallet.update_witnesses(updated_witnesses)?;

            debug!(target: "CLIENT STATE", "iterate over secret_keys to decrypt note");

            for secret in secret_keys.iter() {
                if let Some(note) = Self::try_decrypt_note(enc_note, *secret) {
                    // We need to keep track of the witness for this coin.
                    // This allows us to prove inclusion of the coin in the merkle tree with ZK.
                    // Just as we update the merkle tree with every new coin, so we do the same with
                    // the witness.

                    // Derive the current witness from the current tree.
                    // This is done right after we add our coin to the tree (but before any other
                    // coins are added)

                    // Make a new witness for this coin
                    let witness = IncrementalWitness::from_tree(&self.tree);

                    let mut nullifier = [0; 32];
                    nullifier.copy_from_slice(
                        Blake2sParams::new()
                        .hash_length(32)
                        .personal(zcash_primitives::constants::PRF_NF_PERSONALIZATION)
                        .to_state()
                        .update(&secret.to_bytes())
                        .update(&note.serial.to_bytes())
                        .finalize()
                        .as_bytes(),
                    );

                    let nullifier = Nullifier::new(nullifier);

                    let own_coin = OwnCoin {
                        coin: coin.clone(),
                        note: note.clone(),
                        secret: *secret,
                        witness: witness.clone(),
                        nullifier,
                    };

                    wallet.put_own_coins(own_coin)?;
                    let pub_key = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;

                    debug!(target: "CLIENT STATE", "Received a coin: amount {} ", note.value);

                    debug!(target: "CLIENT STATE", "Send a notification");

                    if let Some(ch) = notify.clone() {
                        ch.send((pub_key, note.value)).await?
                    }
                }
            }
        }
        Ok(())
    }

    fn try_decrypt_note(ciphertext: &EncryptedNote, secret: jubjub::Fr) -> Option<Note> {
        match ciphertext.decrypt(&secret) {
            // ... and return the decrypted note for this coin.
            Ok(note) => Some(note),
            // We weren't able to decrypt the note with our key.
            Err(_) => None,
        }
    }
}
*/

impl std::error::Error for ClientFailed {}

impl std::fmt::Display for ClientFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ClientFailed::NotEnoughValue(i) => {
                write!(f, "There is no enough value {}", i)
            }
            ClientFailed::InvalidAddress(i) => {
                write!(f, "Invalid Address {}", i)
            }
            ClientFailed::InvalidAmount(i) => {
                write!(f, "Invalid Amount {}", i)
            }
            ClientFailed::UnableToGetDepositAddress => f.write_str("Unable to get deposit address"),
            ClientFailed::UnableToGetWithdrawAddress => {
                f.write_str("Unable to get withdraw address")
            }
            ClientFailed::DoesNotHaveCashierPublicKey => {
                f.write_str("Does not have cashier public key")
            }
            ClientFailed::DoesNotHaveKeypair => f.write_str("Does not have keypair"),
            ClientFailed::EmptyPassword => f.write_str("Password is empty. Cannot create database"),
            ClientFailed::WalletInitialized => f.write_str("Wallet already initalized"),
            ClientFailed::KeyExists => f.write_str("Keypair already exists"),

            ClientFailed::ClientError(i) => {
                write!(f, "{}", i)
            }

            ClientFailed::VerifyError(i) => {
                write!(f, "Verify error: {}", i)
            }
        }
    }
}

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

pub type ClientResult<T> = std::result::Result<T, ClientFailed>;
