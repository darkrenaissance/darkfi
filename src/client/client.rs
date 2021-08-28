use crate::blockchain::{rocks::columns, Rocks, RocksColumn, Slab};
use crate::cli::TransferParams;
use crate::crypto::{
    load_params,
    merkle::{CommitmentTree, IncrementalWitness},
    merkle_node::MerkleNode,
    note::{EncryptedNote, Note},
    nullifier::Nullifier,
    save_params, setup_mint_prover, setup_spend_prover,
};
use crate::rpc::adapters::user_adapter::UserAdapter;
use crate::rpc::jsonserver;
use crate::serial::{deserialize, Decodable};
use crate::service::{CashierClient, GatewayClient, GatewaySlabsSubscriber};
use crate::state::{state_transition, ProgramState, StateUpdate};
use crate::util::prepare_transaction;
use crate::wallet::WalletPtr;
use crate::{tx, Result};

use async_executor::Executor;
use bellman::groth16;
use bls12_381::Bls12;
use log::*;
use rusqlite::Connection;

use async_std::sync::Arc;
use futures::FutureExt;
use std::net::SocketAddr;
use std::path::PathBuf;

pub struct Client {
    state: State,
    secret: jubjub::Fr,
    mint_params: bellman::groth16::Parameters<Bls12>,
    spend_params: bellman::groth16::Parameters<Bls12>,
    gateway: GatewayClient,
    connected_with_cashier: bool,
}

impl Client {
    pub fn new(
        secret: jubjub::Fr,
        rocks: Arc<Rocks>,
        gateway_addrs: (SocketAddr, SocketAddr),
        params_paths: (PathBuf, PathBuf),
        wallet_path: PathBuf,
    ) -> Result<Self> {
        let slabstore = RocksColumn::<columns::Slabs>::new(rocks.clone());
        let merkle_roots = RocksColumn::<columns::MerkleRoots>::new(rocks.clone());
        let nullifiers = RocksColumn::<columns::Nullifiers>::new(rocks);

        let mint_params_path = params_paths.0.to_str().unwrap_or("mint.params");
        let spend_params_path  = params_paths.1.to_str().unwrap_or("spend.params");

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
            wallet_path,
        };

        // create gateway client
        debug!(target: "Client", "Creating GatewayClient");
        let gateway = GatewayClient::new(gateway_addrs.0, gateway_addrs.1, slabstore)?;

        Ok(Self {
            state,
            secret,
            mint_params,
            spend_params,
            gateway,
            connected_with_cashier: false,
        })
    }

    pub async fn start(&mut self) -> Result<()> {
        self.gateway.start().await?;
        Ok(())
    }

    pub async fn connect_to_subscriber(
        &mut self,
        executor: Arc<Executor<'_>>,
        wallet: WalletPtr,
    ) -> Result<()> {
        if self.connected_with_cashier {
            warn!("The client already connected to the subscriber");
            return Ok(());
        }

        // start subscribing
        debug!(target: "Client", "Start subscriber");
        let gateway_slabs_sub: GatewaySlabsSubscriber =
            self.gateway.start_subscriber(executor.clone()).await?;

        loop {
            let slab = gateway_slabs_sub.recv().await?;
            let tx = tx::Transaction::decode(&slab.get_payload()[..])?;
            let update = state_transition(&self.state, tx)?;
            self.state.apply(update, wallet.clone()).await?;
        }
    }

    pub async fn connect_to_cashier(
        &mut self,
        executor: Arc<Executor<'_>>,
        wallet: WalletPtr,
        cashier_addr: SocketAddr,
        rpc_url: SocketAddr,
    ) -> Result<()> {
        self.connected_with_cashier = true;
        // create cashier client
        debug!(target: "Client", "Creating cashier client");
        let mut cashier_client = CashierClient::new(cashier_addr)?;

        // start subscribing
        debug!(target: "Client", "Start subscriber");
        let gateway_slabs_sub: GatewaySlabsSubscriber =
            self.gateway.start_subscriber(executor.clone()).await?;

        // channels to request transfer from adapter
        let (publish_tx_send, publish_tx_recv) = async_channel::unbounded::<TransferParams>();

        // channels to request deposit from adapter, send DRK key and receive BTC key
        let (deposit_req_send, deposit_req_recv) =
            async_channel::unbounded::<jubjub::SubgroupPoint>();
        let (deposit_rep_send, deposit_rep_recv) =
            async_channel::unbounded::<Option<bitcoin::util::address::Address>>();

        // channel to request withdraw from adapter, send BTC key and receive DRK key
        let (withdraw_req_send, withdraw_req_recv) = async_channel::unbounded::<String>();
        let (withdraw_rep_send, withdraw_rep_recv) =
            async_channel::unbounded::<Option<jubjub::SubgroupPoint>>();

        // start cashier_client
        cashier_client.start().await?;

        let adapter = Arc::new(UserAdapter::new(
            wallet.clone(),
            publish_tx_send.clone(),
            (deposit_req_send.clone(), deposit_rep_recv.clone()),
            (withdraw_req_send.clone(), withdraw_rep_recv.clone()),
        )?);

        // start the rpc server
        debug!(target: "Client", "Start RPC server");
        let io = Arc::new(adapter.handle_input()?);
        let _ = jsonserver::start(executor.clone(), rpc_url, io).await?;

        self.futures_broker(
            &mut cashier_client,
            wallet,
            gateway_slabs_sub.clone(),
            deposit_req_recv.clone(),
            deposit_rep_send.clone(),
            withdraw_req_recv.clone(),
            withdraw_rep_send.clone(),
            publish_tx_recv.clone(),
        )
        .await?;

        Ok(())
    }

    pub async fn futures_broker(
        &mut self,
        cashier_client: &mut CashierClient,
        wallet: WalletPtr,
        gateway_slabs_sub: async_channel::Receiver<Slab>,
        deposit_req: async_channel::Receiver<jubjub::SubgroupPoint>,
        deposit_rep: async_channel::Sender<Option<bitcoin::util::address::Address>>,
        withdraw_req: async_channel::Receiver<String>,
        withdraw_rep: async_channel::Sender<Option<jubjub::SubgroupPoint>>,
        publish_tx_recv: async_channel::Receiver<TransferParams>,
    ) -> Result<()> {
        loop {
            futures::select! {
                slab = gateway_slabs_sub.recv().fuse() => {
                    let slab = slab?;
                    let tx = tx::Transaction::decode(&slab.get_payload()[..])?;
                    let update = state_transition(&self.state, tx)?;
                    self.state.apply(update, wallet.clone()).await?;
                }
                deposit_addr = deposit_req.recv().fuse() => {
                    let btc_public = cashier_client.get_address(deposit_addr?).await?;
                    deposit_rep.send(btc_public).await?;
                }
                withdraw_addr = withdraw_req.recv().fuse() => {
                    let drk_public = cashier_client.withdraw(withdraw_addr?).await?;
                    withdraw_rep.send(drk_public).await?;
                }
                transfer_params = publish_tx_recv.recv().fuse() => {
                    let transfer_params = transfer_params?;

                    let address = bs58::decode(transfer_params.pub_key).into_vec()?;
                    let address: jubjub::SubgroupPoint = deserialize(&address)?;

                    let own_coins = wallet.get_own_coins()?;

                    let slab = prepare_transaction(
                        &self.state,
                        self.secret.clone(),
                        self.mint_params.clone(),
                        self.spend_params.clone(),
                        address,
                        transfer_params.amount,
                        own_coins
                    )?;

                    self.gateway.put_slab(slab).await.expect("put slab");
                }

            }
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
    // TODO: remove this
    wallet_path: PathBuf,
}

impl ProgramState for State {
    fn is_valid_cashier_public_key(&self, _public: &jubjub::SubgroupPoint) -> bool {
        // TODO: use walletdb instead of connecting with sqlite directly
        let conn =
            Connection::open(self.wallet_path.clone()).expect("Failed to connect to database");
        let mut stmt = conn
            .prepare("SELECT key_public FROM cashier WHERE key_public IN (SELECT key_public)")
            .expect("Cannot generate statement.");
        stmt.exists([1i32]).expect("Failed to read database")
        // do actual validity check
    }

    fn is_valid_merkle(&self, merkle_root: &MerkleNode) -> bool {
        self.merkle_roots
            .key_exist(*merkle_root)
            .expect("couldn't check if the merkle_root valid")
    }

    fn nullifier_exists(&self, nullifier: &Nullifier) -> bool {
        self.nullifiers
            .key_exist(nullifier.repr)
            .expect("couldn't check if nullifier exists")
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
    pub async fn apply(&mut self, update: StateUpdate, wallet: WalletPtr) -> Result<()> {
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
            for witness in wallet.witnesses.lock().await.iter_mut() {
                witness.append(node).expect("append to witness");
            }

            if let Some((note, secret)) = self.try_decrypt_note(wallet.clone(), enc_note).await {
                // We need to keep track of the witness for this coin.
                // This allows us to prove inclusion of the coin in the merkle tree with ZK.
                // Just as we update the merkle tree with every new coin, so we do the same with
                // the witness.

                // Derive the current witness from the current tree.
                // This is done right after we add our coin to the tree (but before any other
                // coins are added)

                // Make a new witness for this coin
                let witness = IncrementalWitness::from_tree(&self.tree);

                wallet.put_own_coins(coin.clone(), note.clone(), witness.clone(), secret)?;
            }
        }
        Ok(())
    }

    async fn try_decrypt_note(
        &self,
        wallet: WalletPtr,
        ciphertext: EncryptedNote,
    ) -> Option<(Note, jubjub::Fr)> {
        let secret = wallet.get_private().ok()?;
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
