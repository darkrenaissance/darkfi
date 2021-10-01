use crate::{
    blockchain::{rocks::columns, Rocks, RocksColumn, Slab},
    crypto::{
        load_params,
        merkle::{CommitmentTree, IncrementalWitness},
        merkle_node::MerkleNode,
        note::{EncryptedNote, Note},
        nullifier::Nullifier,
        save_params, setup_mint_prover, setup_spend_prover, OwnCoin,
    },
    serial::{Decodable, Encodable},
    service::{GatewayClient, GatewaySlabsSubscriber},
    state::{state_transition, ProgramState, StateUpdate},
    tx,
    wallet::{CashierDbPtr, Keypair, WalletPtr},
    Result,
};

use super::ClientFailed;

use async_executor::Executor;
use bellman::groth16;
use bls12_381::Bls12;

use async_std::sync::{Arc, Mutex};
use log::*;
use std::net::SocketAddr;
use std::path::PathBuf;

pub struct Client {
    pub state: Arc<Mutex<State>>,
    mint_params: bellman::groth16::Parameters<Bls12>,
    spend_params: bellman::groth16::Parameters<Bls12>,
    gateway: GatewayClient,
    pub main_keypair: Keypair,
}

impl Client {
    pub async fn new(
        rocks: Arc<Rocks>,
        gateway_addrs: (SocketAddr, SocketAddr),
        params_paths: (PathBuf, PathBuf),
        wallet: WalletPtr,
    ) -> Result<Self> {
        let slabstore = RocksColumn::<columns::Slabs>::new(rocks.clone());
        let merkle_roots = RocksColumn::<columns::MerkleRoots>::new(rocks.clone());
        let nullifiers = RocksColumn::<columns::Nullifiers>::new(rocks);

        let mint_params_path = params_paths.0.to_str().unwrap_or("mint.params");
        let spend_params_path = params_paths.1.to_str().unwrap_or("spend.params");

        wallet.init_db().await?;

        if wallet.get_keypairs()?.len() == 0 {
            wallet.key_gen()?;
        }

        let main_keypair = wallet.get_keypairs()?[0].clone();

        // Auto create trusted ceremony parameters if they don't exist
        if !params_paths.0.exists() {
            let params = setup_mint_prover();
            save_params(mint_params_path, &params)?;
        }
        if !params_paths.1.exists() {
            let params = setup_spend_prover();
            save_params(spend_params_path, &params)?;
        }

        // Load trusted setup parameters
        let (mint_params, mint_pvk) = load_params(mint_params_path)?;
        let (spend_params, spend_pvk) = load_params(spend_params_path)?;

        let state = Arc::new(Mutex::new(State {
            tree: CommitmentTree::empty(),
            merkle_roots,
            nullifiers,
            mint_pvk,
            spend_pvk,
            wallet,
        }));

        // create gateway client
        debug!(target: "CLIENT", "Creating GatewayClient");
        let gateway = GatewayClient::new(gateway_addrs.0, gateway_addrs.1, slabstore)?;

        Ok(Self {
            state,
            mint_params,
            spend_params,
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
        asset_id: jubjub::Fr,
        pub_key: jubjub::SubgroupPoint,
        amount: u64,
    ) -> Result<()> {
        if amount <= 0 {
            return Err(ClientFailed::InvalidAmount(amount as u64).into());
        }

        self.send(pub_key, amount, asset_id, false).await?;

        Ok(())
    }

    pub async fn send(
        &mut self,
        pub_key: jubjub::SubgroupPoint,
        amount: u64,
        asset_id: jubjub::Fr,
        clear_input: bool,
    ) -> Result<()> {
        let slab = self
            .build_slab_from_tx(pub_key, amount, asset_id, clear_input)
            .await?;

        self.gateway.put_slab(slab).await?;

        Ok(())
    }

    async fn build_slab_from_tx(
        &self,
        pub_key: jubjub::SubgroupPoint,
        value: u64,
        asset_id: jubjub::Fr,
        clear_input: bool,
    ) -> Result<Slab> {
        let mut clear_inputs: Vec<tx::TransactionBuilderClearInputInfo> = vec![];
        let mut inputs: Vec<tx::TransactionBuilderInputInfo> = vec![];
        let mut outputs: Vec<tx::TransactionBuilderOutputInfo> = vec![];

        if clear_input {
            let signature_secret = self.main_keypair.private;
            let input = tx::TransactionBuilderClearInputInfo {
                value,
                asset_id,
                signature_secret,
            };
            clear_inputs.push(input);
        } else {
            inputs = self.build_inputs(value, asset_id, &mut outputs).await?;
        }

        outputs.push(tx::TransactionBuilderOutputInfo {
            value,
            asset_id,
            public: pub_key,
        });

        let builder = tx::TransactionBuilder {
            clear_inputs,
            inputs,
            outputs,
        };

        let mut tx_data = vec![];
        {
            let tx = builder.build(&self.mint_params, &self.spend_params);
            tx.encode(&mut tx_data).expect("encode tx");
        }

        let slab = Slab::new(tx_data);
        Ok(slab)
    }

    async fn build_inputs(
        &self,
        amount: u64,
        asset_id: jubjub::Fr,
        outputs: &mut Vec<tx::TransactionBuilderOutputInfo>,
    ) -> Result<Vec<tx::TransactionBuilderInputInfo>> {
        let mut inputs: Vec<tx::TransactionBuilderInputInfo> = vec![];
        let mut inputs_value: u64 = 0;

        let own_coins = self.state.lock().await.wallet.get_own_coins()?;

        for own_coin in own_coins.iter() {
            if inputs_value >= amount {
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
        }

        if inputs_value < amount {
            return Err(ClientFailed::NotEnoughValue(0).into());
        }

        if inputs_value > amount {
            let inputs_len = inputs.len();
            let input = &inputs[inputs_len - 1];

            let return_value: u64 = inputs_value - amount;

            let own_pub_key = zcash_primitives::constants::SPENDING_KEY_GENERATOR * input.secret;

            outputs.push(tx::TransactionBuilderOutputInfo {
                value: return_value,
                asset_id,
                public: own_pub_key,
            });
        }
        Ok(vec![])
    }

    pub async fn connect_to_subscriber_from_cashier(
        &self,
        cashier_wallet: CashierDbPtr,
        notify: async_channel::Sender<(jubjub::SubgroupPoint, u64)>,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        // start subscribing
        debug!(target: "CLIENT", "Start subscriber");
        let gateway_slabs_sub: GatewaySlabsSubscriber =
            self.gateway.start_subscriber(executor.clone()).await?;

        let secret_key = self.main_keypair.private;
        let state = self.state.clone();

        let task: smol::Task<Result<()>> = smol::spawn(async move {
            loop {
                let slab = gateway_slabs_sub.recv().await?;
                let tx = tx::Transaction::decode(&slab.get_payload()[..])?;

                let mut state = state.lock().await;

                let update = state_transition(&state, tx)?;

                let mut secret_keys: Vec<jubjub::Fr> = vec![secret_key];
                let mut withdraw_keys = cashier_wallet.get_withdraw_private_keys()?;
                secret_keys.append(&mut withdraw_keys);

                state
                    .apply(update, secret_keys.clone(), notify.clone())
                    .await?;
            }
        });

        task.detach();

        Ok(())
    }

    pub async fn connect_to_subscriber(&self, executor: Arc<Executor<'_>>) -> Result<()> {
        // start subscribing
        debug!(target: "CLIENT", "Start subscriber");
        let gateway_slabs_sub: GatewaySlabsSubscriber =
            self.gateway.start_subscriber(executor.clone()).await?;

        let (notify, _) = async_channel::unbounded::<(jubjub::SubgroupPoint, u64)>();


        let secret_key = self.main_keypair.private;
        let state = self.state.clone();

        let task: smol::Task<Result<()>> = smol::spawn(async move {
            loop {
                let slab = gateway_slabs_sub.recv().await?;
                let tx = tx::Transaction::decode(&slab.get_payload()[..])?;

                let mut state = state.lock().await;

                let update = state_transition(&state, tx)?;

                let secret_keys: Vec<jubjub::Fr> = vec![secret_key];

                state
                    .apply(update, secret_keys.clone(), notify.clone())
                    .await?;
            }
        });

        task.detach();

        Ok(())
    }

    pub async fn init_db(&self) -> Result<()> {
        self.state.lock().await.wallet.init_db().await
    }

    pub async fn key_gen(&self) -> Result<()> {
        self.state.lock().await.wallet.key_gen()
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
    pub wallet: WalletPtr,
}

impl ProgramState for State {
    fn is_valid_cashier_public_key(&self, public: &jubjub::SubgroupPoint) -> bool {
        self.wallet
            .get_cashier_public_keys()
            .expect("Get cashier public keys")
            .contains(public)
    }

    fn is_valid_merkle(&self, merkle_root: &MerkleNode) -> bool {
        self.merkle_roots
            .key_exist(*merkle_root)
            .expect("Check if the merkle_root valid")
    }

    fn nullifier_exists(&self, nullifier: &Nullifier) -> bool {
        self.nullifiers
            .key_exist(nullifier.repr)
            .expect("Check if nullifier exists")
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
        notify: async_channel::Sender<(jubjub::SubgroupPoint, u64)>,
    ) -> Result<()> {
        // Extend our list of nullifiers with the ones from the update
        for nullifier in update.nullifiers {
            self.nullifiers.put(nullifier, vec![] as Vec<u8>)?;
        }

        // Update merkle tree and witnesses
        for (coin, enc_note) in update.coins.into_iter().zip(update.enc_notes.iter()) {
            // Add the new coins to the merkle tree
            let node = MerkleNode::from_coin(&coin);
            self.tree.append(node).expect("Append to merkle tree");

            // Keep track of all merkle roots that have existed
            self.merkle_roots.put(self.tree.root(), vec![] as Vec<u8>)?;

            // Also update all the coin witnesses
            for (coin_id, witness) in self.wallet.get_witnesses()?.iter_mut() {
                witness.append(node).expect("Append to witness");
                self.wallet.update_witness(*coin_id, witness.clone())?;
            }

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

                    let own_coin = OwnCoin {
                        coin: coin.clone(),
                        note: note.clone(),
                        secret: *secret,
                        witness: witness.clone(),
                    };

                    self.wallet.put_own_coins(own_coin)?;
                    let pub_key = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;
                    notify.send((pub_key, note.value)).await?;
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
