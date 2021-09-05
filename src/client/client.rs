use crate::blockchain::{rocks::columns, Rocks, RocksColumn, Slab};
use crate::crypto::{
    load_params,
    merkle::{CommitmentTree, IncrementalWitness},
    merkle_node::MerkleNode,
    note::{EncryptedNote, Note},
    nullifier::Nullifier,
    save_params, setup_mint_prover, setup_spend_prover,
};
use crate::rpc::adapters::{RpcClient, RpcClientAdapter};
use crate::rpc::jsonserver;
use crate::serial::Encodable;
use crate::serial::{deserialize, Decodable};
use crate::service::{CashierClient, GatewayClient, GatewaySlabsSubscriber};
use crate::state::{state_transition, ProgramState, StateUpdate};
use crate::wallet::WalletPtr;
use crate::{tx, Result};

use super::ClientFailed;

use async_executor::Executor;
use bellman::groth16;
use bls12_381::Bls12;
use log::*;

use jsonrpc_core::IoHandler;

use async_std::sync::{Arc, Mutex};
use std::net::SocketAddr;
use std::path::PathBuf;

pub struct Client {
    pub state: State,
    secret: jubjub::Fr,
    mint_params: bellman::groth16::Parameters<Bls12>,
    spend_params: bellman::groth16::Parameters<Bls12>,
    gateway: GatewayClient,
}

impl Client {
    pub fn new(
        secret: jubjub::Fr,
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

        let state = State {
            tree: CommitmentTree::empty(),
            merkle_roots,
            nullifiers,
            mint_pvk,
            spend_pvk,
            wallet,
        };

        // create gateway client
        debug!(target: "CLIENT", "Creating GatewayClient");
        let gateway = GatewayClient::new(gateway_addrs.0, gateway_addrs.1, slabstore)?;

        Ok(Self {
            state,
            secret,
            mint_params,
            spend_params,
            gateway,
        })
    }

    pub async fn start(&mut self) -> Result<()> {
        self.gateway.start().await?;
        Ok(())
    }

    pub async fn connect_to_cashier(
        client: Client,
        executor: Arc<Executor<'_>>,
        cashier_addr: SocketAddr,
        rpc_url: SocketAddr,
    ) -> Result<()> {
        // create cashier client
        debug!(target: "CLIENT", "Creating cashier client");
        let mut cashier_client = CashierClient::new(cashier_addr)?;

        // start cashier_client
        cashier_client.start().await?;

        let client_mutex = Arc::new(Mutex::new(client));
        let cashier_mutex = Arc::new(Mutex::new(cashier_client));

        let mut io = IoHandler::new();

        let rpc_client_adapter = RpcClientAdapter::new(client_mutex.clone(), cashier_mutex.clone());

        io.extend_with(rpc_client_adapter.to_delegate());

        let io = Arc::new(io);

        // start the rpc server
        debug!(target: "CLIENT", "Start RPC server");
        let _ = jsonserver::start(executor.clone(), rpc_url, io).await?;

        // start subscriber
        Client::connect_to_subscriber(client_mutex.clone(), executor.clone()).await?;

        Ok(())
    }

    pub async fn transfer(self: &mut Client, pub_key: String, amount: f64) -> Result<()> {
        let address = bs58::decode(pub_key.clone())
            .into_vec()
            .map_err(|_| ClientFailed::UnvalidAddress(pub_key.clone()))?;

        let address: jubjub::SubgroupPoint =
            deserialize(&address).map_err(|_| ClientFailed::UnvalidAddress(pub_key))?;

        if amount <= 0.0 {
            return Err(ClientFailed::UnvalidAmount(amount as u64).into());
        }

        // check if there are coins
        let own_coins = self.state.wallet.get_own_coins()?;

        if own_coins.is_empty() {
            return Err(ClientFailed::NotEnoughValue(0).into());
        }

        let witness = &own_coins[0].3;
        let merkle_path = witness.path().unwrap();

        // Construct a new tx spending the coin
        let builder = tx::TransactionBuilder {
            clear_inputs: vec![],
            inputs: vec![tx::TransactionBuilderInputInfo {
                merkle_path,
                secret: self.secret.clone(),
                note: own_coins[0].1.clone(),
            }],
            // We can add more outputs to this list.
            // The only constraint is that sum(value in) == sum(value out)
            outputs: vec![tx::TransactionBuilderOutputInfo {
                value: amount as u64,
                asset_id: 1,
                public: address,
            }],
        };
        // Build the tx
        let mut tx_data = vec![];
        {
            let tx = builder.build(&self.mint_params, &self.spend_params);
            tx.encode(&mut tx_data).expect("encode tx");
        }

        // build slab from the transaction
        let slab = Slab::new(tx_data);

        self.gateway.put_slab(slab).await?;

        Ok(())
    }

    pub async fn connect_to_subscriber(
        client: Arc<Mutex<Client>>,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        // start subscribing
        debug!(target: "CLIENT", "Start subscriber");
        let gateway_slabs_sub: GatewaySlabsSubscriber = client
            .lock()
            .await
            .gateway
            .start_subscriber(executor.clone())
            .await?;

        loop {
            let slab = gateway_slabs_sub.recv().await?;
            let tx = tx::Transaction::decode(&slab.get_payload()[..])?;
            let mut client = client.lock().await;
            let update = state_transition(&client.state, tx)?;
            client.state.apply(update).await?;
        }
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
    pub async fn apply(&mut self, update: StateUpdate) -> Result<()> {
        // Extend our list of nullifiers with the ones from the update
        for nullifier in update.nullifiers {
            self.nullifiers.put(nullifier, vec![] as Vec<u8>)?;
        }

        // Update merkle tree and witnesses
        for (coin, enc_note) in update.coins.into_iter().zip(update.enc_notes.into_iter()) {
            // Add the new coins to the merkle tree
            let node = MerkleNode::from_coin(&coin);
            self.tree.append(node).expect("Append to merkle tree");

            // Keep track of all merkle roots that have existed
            self.merkle_roots.put(self.tree.root(), vec![] as Vec<u8>)?;

            // Also update all the coin witnesses
            for (coin_id, witness) in self.wallet.get_witnesses()?.iter_mut() {
                witness.append(node).expect("Append to witness");
                self.wallet
                    .update_witness(coin_id.clone(), witness.clone())?;
            }

            if let Some((note, secret)) = self.try_decrypt_note(enc_note).await {
                // We need to keep track of the witness for this coin.
                // This allows us to prove inclusion of the coin in the merkle tree with ZK.
                // Just as we update the merkle tree with every new coin, so we do the same with
                // the witness.

                // Derive the current witness from the current tree.
                // This is done right after we add our coin to the tree (but before any other
                // coins are added)

                // Make a new witness for this coin
                let witness = IncrementalWitness::from_tree(&self.tree);

                self.wallet
                    .put_own_coins(coin.clone(), note.clone(), secret, witness.clone())?;
            }
        }
        Ok(())
    }

    async fn try_decrypt_note(&self, ciphertext: EncryptedNote) -> Option<(Note, jubjub::Fr)> {
        let secret = self.wallet.get_private().ok()?;
        match ciphertext.decrypt(&secret) {
            Ok(note) => {
                // ... and return the decrypted note for this coin.
                return Some((note, secret.clone()));
            }
            Err(_) => {}
        }
        // We weren't able to decrypt the note with our key.
        None
    }
}
