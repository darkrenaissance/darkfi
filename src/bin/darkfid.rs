use drk::blockchain::{rocks::columns, Rocks, RocksColumn, Slab};
use drk::cli::{Config, DarkfidCli, DarkfidConfig};
use drk::cli::{TransferParams, WithdrawParams};
use drk::crypto::{
    load_params,
    merkle::{CommitmentTree, IncrementalWitness},
    merkle_node::MerkleNode,
    note::{EncryptedNote, Note},
    nullifier::Nullifier,
    save_params, setup_mint_prover, setup_spend_prover,
};
use drk::rpc::adapter::RpcAdapter;
use drk::rpc::jsonserver;
use drk::serial::{deserialize, Decodable, Encodable};
use drk::service::{CashierClient, GatewayClient, GatewaySlabsSubscriber};
use drk::state::{state_transition, ProgramState, StateUpdate};
use drk::util::join_config_path;
use drk::wallet::{WalletDb, WalletPtr};
use drk::{tx, Result};

use async_executor::Executor;
use bellman::groth16;
use bls12_381::Bls12;
use easy_parallel::Parallel;
use ff::Field;
use log::*;
use rand::rngs::OsRng;
use rusqlite::Connection;

use async_std::sync::Arc;
use futures::FutureExt;
use std::net::SocketAddr;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

pub struct State {
    // The entire merkle tree state
    tree: CommitmentTree<MerkleNode>,
    // List of all previous and the current merkle roots
    // This is the hashed value of all the children.
    merkle_roots: RocksColumn<columns::MerkleRoots>,
    // Nullifiers prevent double spending
    nullifiers: RocksColumn<columns::Nullifiers>,
    // Mint verifying key used by ZK
    mint_pvk: groth16::PreparedVerifyingKey<Bls12>,
    // Spend verifying key used by ZK
    spend_pvk: groth16::PreparedVerifyingKey<Bls12>,
    // Public key of the cashier
    // List of all our secret keys
    wallet: WalletPtr,
}

impl ProgramState for State {
    fn is_valid_cashier_public_key(&self, _public: &jubjub::SubgroupPoint) -> bool {
        let conn = Connection::open(&self.wallet.path).expect("Failed to connect to database");
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
    async fn apply(&mut self, update: StateUpdate) -> Result<()> {
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
            for witness in self.wallet.witnesses.lock().await.iter_mut() {
                witness.append(node).expect("append to witness");
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
                    .put_own_coins(coin.clone(), note.clone(), witness.clone(), secret)?;
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

//pub async fn subscribe(
//    gateway_slabs_sub: GatewaySlabsSubscriber,
//    mut state: State,
//) -> Result<()> {
//}

pub async fn futures_broker(
    client: &mut GatewayClient,
    cashier_client: &mut CashierClient,
    state: &mut State,
    secret: jubjub::Fr,
    mint_params: bellman::groth16::Parameters<Bls12>,
    spend_params: bellman::groth16::Parameters<Bls12>,
    gateway_slabs_sub: async_channel::Receiver<Slab>,
    deposit_recv: async_channel::Receiver<jubjub::SubgroupPoint>,
    cashier_deposit_addr_send: async_channel::Sender<Option<bitcoin::util::address::Address>>,
    withdraw_recv: async_channel::Receiver<WithdrawParams>,
    cashier_withdraw_send: async_channel::Sender<jubjub::SubgroupPoint>,
    publish_tx_recv: async_channel::Receiver<TransferParams>,
) -> Result<()> {
    loop {
        futures::select! {
            slab = gateway_slabs_sub.recv().fuse() => {
                let slab = slab?;
                let tx = tx::Transaction::decode(&slab.get_payload()[..])?;
                let update = state_transition(state, tx)?;
                state.apply(update).await?;
            }
            deposit_addr = deposit_recv.recv().fuse() => {
                let cashier_public =  cashier_client.get_address(deposit_addr?).await?;
                cashier_deposit_addr_send.send(cashier_public).await?;
            }
            withdraw_params = withdraw_recv.recv().fuse() => {
                let withdraw_params = withdraw_params?;

                let btc_address = bitcoin::util::address::Address::from_str(&withdraw_params.pub_key);
                let amount = withdraw_params.amount;

                // cashier_withdraw_send.send(address).await?;

            }
            transfer_params = publish_tx_recv.recv().fuse() => {
                let transfer_params = transfer_params?;

                let merkle_path = {
                    let (_coin, _, _, witness) = &mut state.wallet.get_own_coins()?[0];

                    let merkle_path = witness.path().unwrap();

                    merkle_path
                };

                let address = bs58::decode(transfer_params.pub_key).into_vec()?;
                let address: jubjub::SubgroupPoint = deserialize(&address)?;

                // Make a spend tx

                // Construct a new tx spending the coin
                let builder = tx::TransactionBuilder {
                    clear_inputs: vec![],
                    inputs: vec![tx::TransactionBuilderInputInfo {
                        merkle_path,
                        secret: secret.clone(),
                        note: state.wallet.get_own_coins()?[0].1.clone(),
                    }],
                    // We can add more outputs to this list.
                    // The only constraint is that sum(value in) == sum(value out)
                    outputs: vec![tx::TransactionBuilderOutputInfo {
                        value: transfer_params.amount as u64,
                        asset_id: 1,
                        public: address,
                    }],
                };
                // Build the tx
                let mut tx_data = vec![];
                {
                    let tx = builder.build(&mint_params, &spend_params);
                    tx.encode(&mut tx_data).expect("encode tx");
                }

                let slab = Slab::new(tx_data);

                client.put_slab(slab).await.expect("put slab");
            }

        }
    }
}

async fn start(executor: Arc<Executor<'_>>, config: Arc<&DarkfidConfig>) -> Result<()> {
    let connect_addr: SocketAddr = config.connect_url.parse()?;
    let sub_addr: SocketAddr = config.subscriber_url.parse()?;
    let cashier_addr: SocketAddr = config.cashier_url.parse()?;
    let database_path = config.database_path.clone();
    let walletdb_path = config.walletdb_path.clone();

    let database_path = join_config_path(&PathBuf::from(database_path))?;
    let walletdb_path = join_config_path(&PathBuf::from(walletdb_path))?;

    let rocks = Rocks::new(&database_path)?;

    let rocks2 = rocks.clone();
    let slabstore = RocksColumn::<columns::Slabs>::new(rocks2.clone());

    // Auto create trusted ceremony parameters if they don't exist
    if !Path::new("mint.params").exists() {
        let params = setup_mint_prover();
        save_params("mint.params", &params)?;
    }
    if !Path::new("spend.params").exists() {
        let params = setup_spend_prover();
        save_params("spend.params", &params)?;
    }

    // Load trusted setup parameters
    let (mint_params, mint_pvk) = load_params("mint.params")?;
    let (spend_params, spend_pvk) = load_params("spend.params")?;

    //let cashier_secret = jubjub::Fr::random(&mut OsRng);
    //let cashier_public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * cashier_secret;

    // wallet secret key
    let secret = jubjub::Fr::random(&mut OsRng);
    // wallet public key
    let _public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;

    let merkle_roots = RocksColumn::<columns::MerkleRoots>::new(rocks.clone());
    let nullifiers = RocksColumn::<columns::Nullifiers>::new(rocks);

    let wallet = Arc::new(WalletDb::new(&walletdb_path, config.password.clone())?);

    let ex = executor.clone();

    let mut state = State {
        tree: CommitmentTree::empty(),
        merkle_roots,
        nullifiers,
        mint_pvk,
        spend_pvk,
        wallet: wallet.clone(),
    };

    // create gateway client
    debug!(target: "Client", "Creating client");
    let mut client = GatewayClient::new(connect_addr, slabstore)?;

    // create cashier client
    debug!(target: "Cashier Client", "Creating cashier client");
    let mut cashier_client = CashierClient::new(cashier_addr)?;

    debug!(target: "Gateway", "Start subscriber");
    // start subscribing
    let gateway_slabs_sub: GatewaySlabsSubscriber =
        client.start_subscriber(sub_addr, executor.clone()).await?;

    // channels to request transfer from adapter
    let (publish_tx_send, publish_tx_recv) = async_channel::unbounded::<TransferParams>();

    // channels to request deposit from adapter and receive cashier public key
    let (deposit_send, deposit_recv) = async_channel::unbounded::<jubjub::SubgroupPoint>();
    let (cashier_deposit_addr_send, cashier_deposit_addr_recv) =
        async_channel::unbounded::<Option<bitcoin::util::address::Address>>();

    // channel to request withdraw from adapter
    let (withdraw_send, withdraw_recv) = async_channel::unbounded::<WithdrawParams>();
    let (cashier_withdraw_send, cashier_withdraw_recv) =
        async_channel::unbounded::<jubjub::SubgroupPoint>();

    // start gateway client
    debug!(target: "fn::start client", "start() Client started");
    client.start().await?;
    cashier_client.start().await?;

    let futures_broker_task = executor.spawn(async move {
        futures_broker(
            &mut client,
            &mut cashier_client,
            &mut state,
            secret.clone(),
            mint_params.clone(),
            spend_params.clone(),
            gateway_slabs_sub.clone(),
            deposit_recv.clone(),
            cashier_deposit_addr_send.clone(),
            withdraw_recv.clone(),
            cashier_withdraw_send.clone(),
            publish_tx_recv.clone(),
        )
        .await?;
        Ok::<(), drk::Error>(())
    });

    let adapter = RpcAdapter::new(
        wallet.clone(),
        publish_tx_send,
        (deposit_send, cashier_deposit_addr_recv),
        (withdraw_send, cashier_withdraw_recv),
    )?;

    // start the rpc server
    jsonserver::start(ex.clone(), config.clone(), adapter).await?;

    futures_broker_task.cancel().await;
    Ok(())
}

fn main() -> Result<()> {
    let options = Arc::new(DarkfidCli::load()?);

    let config_path: PathBuf;

    match options.config.as_ref() {
        Some(path) => {
            config_path = path.to_owned();
        }
        None => {
            config_path = join_config_path(&PathBuf::from("darkfid.toml"))?;
        }
    }

    let config: DarkfidConfig = if Path::new(&config_path).exists() {
        Config::<DarkfidConfig>::load(config_path)?
    } else {
        Config::<DarkfidConfig>::load_default(config_path)?
    };

    let config_ptr = Arc::new(&config);

    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();

    {
        use simplelog::*;
        let logger_config = ConfigBuilder::new().set_time_format_str("%T%.6f").build();

        let debug_level = if options.verbose {
            LevelFilter::Debug
        } else {
            LevelFilter::Off
        };

        let log_path = config.log_path.clone();
        CombinedLogger::init(vec![
            TermLogger::new(debug_level, logger_config, TerminalMode::Mixed).unwrap(),
            WriteLogger::new(
                LevelFilter::Debug,
                Config::default(),
                std::fs::File::create(log_path).unwrap(),
            ),
        ])
        .unwrap();
    }

    let ex2 = ex.clone();

    let (_, result) = Parallel::new()
        // Run four executor threads.
        .each(0..3, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                start(ex2, config_ptr).await?;
                drop(signal);
                Ok::<(), drk::Error>(())
            })
        });

    result
}
