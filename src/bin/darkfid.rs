use async_std::sync::Arc;
use rand::rngs::OsRng;
use std::net::SocketAddr;

use drk::blockchain::{rocks::columns, Rocks, RocksColumn, Slab, SlabStore};
use drk::crypto::{
    coin::Coin,
    load_params,
    merkle::{CommitmentTree, IncrementalWitness},
    merkle_node::MerkleNode,
    note::{EncryptedNote, Note},
    nullifier::Nullifier,
    save_params, setup_mint_prover, setup_spend_prover,
};
use drk::serial::Decodable;
use drk::service::{ClientProgramOptions, GatewayClient, Subscriber};
use drk::state::{state_transition, ProgramState, StateUpdate};
use drk::wallet::WalletDB;
use drk::{tx, Result};
use rusqlite::{named_params, Connection};

use async_executor::Executor;
use bellman::groth16;
use bls12_381::Bls12;
use easy_parallel::Parallel;
use ff::Field;
use std::path::Path;

pub struct State {
    // The entire merkle tree state
    tree: CommitmentTree<MerkleNode>,
    // List of all previous and the current merkle roots
    // This is the hashed value of all the children.
    merkle_roots: RocksColumn<columns::MerkleRoots>,
    // Nullifiers prevent double spending
    nullifiers: RocksColumn<columns::Nullifiers>,
    // All received coins
    own_coins: Vec<(Coin, Note, jubjub::Fr, IncrementalWitness<MerkleNode>)>,
    // Mint verifying key used by ZK
    mint_pvk: groth16::PreparedVerifyingKey<Bls12>,
    // Spend verifying key used by ZK
    spend_pvk: groth16::PreparedVerifyingKey<Bls12>,
    // Public key of the cashier
    cashier_public: jubjub::SubgroupPoint,
    // List of all our secret keys
    secrets: Vec<jubjub::Fr>,
}

impl ProgramState for State {
    fn is_valid_cashier_public_key(&self, _public: &jubjub::SubgroupPoint) -> bool {
        // Still needs to be tested
        let path = WalletDB::path("cashier.db").expect("Failed to get path");
        let connect = Connection::open(&path).expect("Failed to connect to database.");
        let mut stmt = connect
            .prepare("SELECT key_public FROM cashier WHERE key_public IN (SELECT key_public)")
            .expect("Cannot generate statement.");
        stmt.exists([1i32]).unwrap()
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

    fn mint_pvk(&self) -> &groth16::PreparedVerifyingKey<Bls12> {
        &self.mint_pvk
    }

    fn spend_pvk(&self) -> &groth16::PreparedVerifyingKey<Bls12> {
        &self.spend_pvk
    }
}

impl State {
    fn apply(&mut self, update: StateUpdate) -> Result<()> {
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

            // own coins is sql
            // Also update all the coin witnesses
            for (_, _, _, witness) in self.own_coins.iter_mut() {
                witness.append(node).expect("append to witness");
            }

            if let Some((note, secret)) = self.try_decrypt_note(enc_note) {
                // We need to keep track of the witness for this coin.
                // This allows us to prove inclusion of the coin in the merkle tree with ZK.
                // Just as we update the merkle tree with every new coin, so we do the same with
                // the witness.

                // Derive the current witness from the current tree.
                // This is done right after we add our coin to the tree (but before any other
                // coins are added)

                // Make a new witness for this coin
                let witness = IncrementalWitness::from_tree(&self.tree);
                self.own_coins.push((coin, note, secret, witness));
            }
        }
        Ok(())
    }

    // sql
    fn try_decrypt_note(&self, _ciphertext: EncryptedNote) -> Option<(Note, jubjub::Fr)> {
        // TODO
        None
    }
}

fn setup_addr(address: Option<SocketAddr>, default: SocketAddr) -> SocketAddr {
    match address {
        Some(addr) => addr,
        None => default,
    }
}

pub async fn subscribe(
    mut subscriber: Subscriber,
    slabstore: Arc<SlabStore>,
    mut state: State,
) -> Result<()> {
    loop {
        let slab = subscriber.fetch::<Slab>().await?;
        let tx = tx::Transaction::decode(&slab.get_payload()[..])?;

        let update = state_transition(&state, tx)?;
        state.apply(update)?;

        slabstore.put(slab)?;
    }
}

async fn start(executor: Arc<Executor<'_>>, options: ClientProgramOptions) -> Result<()> {
    let connect_addr: SocketAddr = setup_addr(options.connect_addr, "127.0.0.1:3333".parse()?);
    let sub_addr: SocketAddr = setup_addr(options.sub_addr, "127.0.0.1:4444".parse()?);
    let database_path = options.database_path.as_path();

    let rocks = Rocks::new(database_path)?;

    let slabstore = RocksColumn::<columns::Slabs>::new(rocks.clone());
    // create gateway client
    let mut client = GatewayClient::new(connect_addr, slabstore)?;

    // start gateway client
    client.start().await?;

    //
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
    let (_mint_params, mint_pvk) = load_params("mint.params")?;
    let (_spend_params, spend_pvk) = load_params("spend.params")?;

    let cashier_secret = jubjub::Fr::random(&mut OsRng);
    let cashier_public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * cashier_secret;

    // wallet secret key
    let secret = jubjub::Fr::random(&mut OsRng);
    // wallet public key
    let _public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;

    let merkle_roots = RocksColumn::<columns::MerkleRoots>::new(rocks.clone());
    let nullifiers = RocksColumn::<columns::Nullifiers>::new(rocks);

    let state = State {
        tree: CommitmentTree::empty(),
        merkle_roots,
        nullifiers,
        own_coins: vec![],
        mint_pvk,
        spend_pvk,
        cashier_public,
        secrets: vec![secret.clone()],
    };

    // start subscribe to gateway publisher
    let subscriber = GatewayClient::start_subscriber(sub_addr).await?;
    let slabstore = client.get_slabstore();
    let subscribe_task = executor.spawn(subscribe(subscriber, slabstore, state));

    subscribe_task.cancel().await;
    Ok(())
}

fn main() -> Result<()> {
    use simplelog::*;

    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let options = ClientProgramOptions::load()?;

    let logger_config = ConfigBuilder::new().set_time_format_str("%T%.6f").build();

    let debug_level = if options.verbose {
        LevelFilter::Debug
    } else {
        LevelFilter::Off
    };

    CombinedLogger::init(vec![
        TermLogger::new(debug_level, logger_config, TerminalMode::Mixed).unwrap(),
        WriteLogger::new(
            LevelFilter::Debug,
            Config::default(),
            std::fs::File::create(options.log_path.as_path()).unwrap(),
        ),
    ])
    .unwrap();

    let ex2 = ex.clone();

    let (_, result) = Parallel::new()
        // Run four executor threads.
        .each(0..3, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                start(ex2, options).await?;
                drop(signal);
                Ok::<(), drk::Error>(())
            })
        });

    result
}

//
//// $ cargo test --bin darkfid
//// run 10 clients simultaneously
//#[cfg(test)]
//mod test {
//
//    #[test]
//    fn test_darkfid_client() {
//        use std::path::Path;
//
//        use drk::blockchain::{Rocks, Slab};
//        use drk::service::GatewayClient;
//
//        use log::*;
//        use rand::Rng;
//        use simplelog::*;
//
//        let logger_config = ConfigBuilder::new().set_time_format_str("%T%.6f").build();
//
//        CombinedLogger::init(vec![
//            TermLogger::new(LevelFilter::Debug, logger_config, TerminalMode::Mixed).unwrap(),
//            WriteLogger::new(
//                LevelFilter::Debug,
//                Config::default(),
//                std::fs::File::create(Path::new("/tmp/dar.log")).unwrap(),
//            ),
//        ])
//        .unwrap();
//
//        let mut thread_pools: Vec<std::thread::JoinHandle<()>> = vec![];
//
//        for _ in 0..10 {
//            let thread = std::thread::spawn(|| {
//                smol::future::block_on(async move {
//                    let mut rng = rand::thread_rng();
//                    let rnd: u32 = rng.gen();
//
//                    let path_str = format!("database_{}.db", rnd);
//                    let database_path = Path::new(path_str.as_str());
//                    let rocks = Rocks::new(database_path.clone()).unwrap();
//
//                    // create new client and use different slabstore
//                    let mut client =
//                        GatewayClient::new("127.0.0.1:3333".parse().unwrap(), rocks).unwrap();
//
//                    // start client
//                    client.start().await.unwrap();
//
//                    // sending slab
//                    let _slab = Slab::new("testcoin".to_string(), rnd.to_le_bytes().to_vec());
//                    client.put_slab(_slab).await.unwrap();
//                })
//            });
//            thread_pools.push(thread);
//        }
//        for t in thread_pools {
//            t.join().unwrap();
//        }
//    }
//}
