use drk::blockchain::{rocks::columns, Rocks, RocksColumn};
use drk::cli::{DarkfidCliConfig, DarkfidCli, ClientCliConfig};
use drk::crypto::{
    load_params,
    merkle::{CommitmentTree, IncrementalWitness},
    merkle_node::MerkleNode,
    note::{EncryptedNote, Note},
    nullifier::Nullifier,
    save_params, setup_mint_prover, setup_spend_prover,
};
use drk::serial::Decodable;
use drk::service::{GatewayClient, GatewaySlabsSubscriber};
use drk::state::{state_transition, ProgramState, StateUpdate};
use drk::util::join_config_path;
use drk::wallet::{WalletDB, WalletPtr};
use drk::{tx, Result};
use log::*;
//use drk::rpc::
use drk::rpc::adapter::RpcAdapter;
use drk::rpc::jsonserver;

use async_executor::Executor;
use bellman::groth16;
use bls12_381::Bls12;
use easy_parallel::Parallel;
use ff::Field;
use rand::rngs::OsRng;
use rusqlite::Connection;

use async_std::sync::Arc;
use std::net::SocketAddr;
use std::path::Path;
use std::path::PathBuf;

#[allow(dead_code)]
pub struct State {
    // The entire merkle tree state
    tree: CommitmentTree<MerkleNode>,
    // List of all previous and the current merkle roots
    // This is the hashed value of all the children.
    merkle_roots: RocksColumn<columns::MerkleRoots>,
    // Nullifiers prevent double spending
    nullifiers: RocksColumn<columns::Nullifiers>,
    // All received coins
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

                self.wallet.put_own_coins(coin, note, witness, secret)?;
            }
        }
        Ok(())
    }

    async fn try_decrypt_note(&self, ciphertext: EncryptedNote) -> Option<(Note, jubjub::Fr)> {
        let vec = self.wallet.get_private().ok()?;
        let secret = self
            .wallet
            .get_value_deserialized::<jubjub::Fr>(vec)
            .expect("Deserialize failed");
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

pub async fn subscribe(gateway_slabs_sub: GatewaySlabsSubscriber, mut state: State) -> Result<()> {
    loop {
        let slab = gateway_slabs_sub.recv().await?;
        let tx = tx::Transaction::decode(&slab.get_payload()[..])?;

        let update = state_transition(&state, tx)?;
        state.apply(update).await?;
    }
}

async fn start(
    executor: Arc<Executor<'_>>,
    config: Arc<DarkfidCliConfig>,
) -> Result<()> {
    let connect_addr: SocketAddr = config.connect_url.parse()?;
    let sub_addr: SocketAddr = config.subscriber_url.parse()?;
    let database_path = config.database_path.clone();

    let database_path = join_config_path(&PathBuf::from(database_path))?;
    let rocks = Rocks::new(&database_path)?;

    let slabstore = RocksColumn::<columns::Slabs>::new(rocks.clone());

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

    //let cashier_secret = jubjub::Fr::random(&mut OsRng);
    //let cashier_public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * cashier_secret;

    // wallet secret key
    let secret = jubjub::Fr::random(&mut OsRng);
    // wallet public key
    let _public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret;

    let merkle_roots = RocksColumn::<columns::MerkleRoots>::new(rocks.clone());
    let nullifiers = RocksColumn::<columns::Nullifiers>::new(rocks);

    let wallet = Arc::new(WalletDB::new("wallet.db", config.password.clone())?);

    let ex = executor.clone();

    let state = State {
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

    debug!(target: "Gateway", "Start subscriber");
    // start subscribing
    let gateway_slabs_sub: GatewaySlabsSubscriber =
        client.start_subscriber(sub_addr, executor.clone()).await?;
    let subscribe_task = executor.spawn(subscribe(gateway_slabs_sub, state));

    // start gateway client
    debug!(target: "fn::start client", "start() Client started");
    client.start().await?;

    let adapter = RpcAdapter::new(wallet.clone())?;
    // start the rpc server
    jsonserver::start(ex.clone(), config.clone(), adapter).await?;

    subscribe_task.cancel().await;
    Ok(())
}

fn main() -> Result<()> {
    use simplelog::*;

    let mut config = DarkfidCliConfig::load(PathBuf::from("darkfid_config"))?;
    let options = Arc::new(DarkfidCli::load(&mut config)?);

    if options.change_config {
        config.save(PathBuf::from("darkfid_config"))?;
        return Ok(());
    }

    let config = Arc::new(config);

    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();

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

    let ex2 = ex.clone();

    let (_, result) = Parallel::new()
        // Run four executor threads.
        .each(0..3, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                start(ex2, config).await?;
                drop(signal);
                Ok::<(), drk::Error>(())
            })
        });

    result
}

//// $ cargo test test_ten_clients_simultaneously --bin darkfid
//this will run 10 clients simultaneously

//// $ cargo test test_subscriber --bin darkfid
// Run Client A and send 10 slabs
// Client B should receive 10 slabs from subscriber

//// $ cargo test test_deposit --bin darkfid
// Run Client A and send 10 slabs
// Client B should receive 10 slabs from subscriber
#[cfg(test)]
mod test {

    use std::net::SocketAddr;
    use std::path::Path;
    use std::path::PathBuf;
    use std::sync::Arc;

    use drk::blockchain::{rocks::columns, Rocks, RocksColumn, Slab};
    use drk::service::{GatewayClient, GatewaySlabsSubscriber};
    use drk::util::join_config_path;

    use async_executor::Executor;
    use easy_parallel::Parallel;
    use log::*;
    use rand::Rng;
    use simplelog::*;

    pub async fn subscribe(gateway_slabs_sub: GatewaySlabsSubscriber, id: String) {
        loop {
            gateway_slabs_sub.recv().await.unwrap();
            info!("Client {}: update state", id);
        }
    }

    fn setup_log() {
        let logger_config = ConfigBuilder::new().set_time_format_str("%T%.6f").build();

        CombinedLogger::init(vec![
            TermLogger::new(LevelFilter::Debug, logger_config, TerminalMode::Mixed).unwrap(),
            WriteLogger::new(
                LevelFilter::Debug,
                Config::default(),
                std::fs::File::create(Path::new("/tmp/dar.log")).unwrap(),
            ),
        ])
        .unwrap();
    }

    #[test]
    fn test_ten_clients_simultaneously() {
        setup_log();

        let mut thread_pools: Vec<std::thread::JoinHandle<()>> = vec![];

        for _ in 0..10 {
            let thread = std::thread::spawn(|| {
                let ex = Arc::new(Executor::new());
                let (signal, shutdown) = async_channel::unbounded::<()>();

                let ex2 = ex.clone();

                let (_, _) = Parallel::new()
                    // Run four executor threads.
                    .each(0..3, |_| smol::future::block_on(ex2.run(shutdown.recv())))
                    // Run the main future on the current thread.
                    .finish(|| {
                        smol::future::block_on(async move {
                            let connect_addr: SocketAddr = "127.0.0.1:3333".parse().unwrap();
                            let sub_addr: SocketAddr = "127.0.0.1:4444".parse().unwrap();

                            let mut rng = rand::thread_rng();
                            let rnd: u32 = rng.gen();
                            let path_str = format!("database_{}.db", rnd);

                            let database_path = PathBuf::from(path_str.as_str());
                            let database_path = join_config_path(&database_path).unwrap();
                            let rocks = Rocks::new(&database_path).unwrap();

                            let slabstore = RocksColumn::<columns::Slabs>::new(rocks.clone());

                            // create gateway client
                            let mut client = GatewayClient::new(connect_addr, slabstore).unwrap();

                            // start subscribing
                            let gateway_slabs_sub: GatewaySlabsSubscriber =
                                client.start_subscriber(sub_addr, ex.clone()).await.unwrap();
                            ex.clone()
                                .spawn(subscribe(gateway_slabs_sub, rnd.clone().to_string()))
                                .detach();

                            // start gateway client
                            client.start().await.unwrap();

                            let slab = Slab::new(rnd.to_le_bytes().to_vec());
                            client.put_slab(slab).await.unwrap();
                        });
                        drop(signal);
                        Ok::<(), drk::Error>(())
                    });
            });
            thread_pools.push(thread);
        }
        for t in thread_pools {
            t.join().unwrap();
        }
    }

    #[test]
    fn test_subscriber() {
        setup_log();

        let mut thread_pools: Vec<std::thread::JoinHandle<()>> = vec![];

        // Client A
        let thread = std::thread::spawn(|| {
            smol::future::block_on(async move {
                let connect_addr: SocketAddr = "127.0.0.1:3333".parse().unwrap();

                let mut rng = rand::thread_rng();
                let rnd: u32 = rng.gen();
                let path_str = format!("database_{}.db", rnd);

                let database_path = PathBuf::from(path_str.as_str());
                let database_path = join_config_path(&database_path).unwrap();
                let rocks = Rocks::new(&database_path).unwrap();

                let slabstore = RocksColumn::<columns::Slabs>::new(rocks.clone());

                // create gateway client
                let mut client = GatewayClient::new(connect_addr, slabstore).unwrap();

                // start gateway client
                client.start().await.unwrap();

                let slab = Slab::new(rnd.to_le_bytes().to_vec());

                client.put_slab(slab.clone()).await.unwrap();
                client.put_slab(slab.clone()).await.unwrap();
                client.put_slab(slab.clone()).await.unwrap();
                client.put_slab(slab.clone()).await.unwrap();
                client.put_slab(slab.clone()).await.unwrap();
                client.put_slab(slab.clone()).await.unwrap();
                client.put_slab(slab.clone()).await.unwrap();
                client.put_slab(slab.clone()).await.unwrap();
                client.put_slab(slab.clone()).await.unwrap();
                client.put_slab(slab.clone()).await.unwrap();
            });
        });
        // Client B
        let thread2 = std::thread::spawn(|| {
            let ex = Arc::new(Executor::new());
            let (signal, shutdown) = async_channel::unbounded::<()>();

            let ex2 = ex.clone();

            let (_, _) = Parallel::new()
                // Run four executor threads.
                .each(0..3, |_| smol::future::block_on(ex2.run(shutdown.recv())))
                // Run the main future on the current thread.
                .finish(|| {
                    smol::future::block_on(async move {
                        let connect_addr: SocketAddr = "127.0.0.1:3333".parse().unwrap();
                        let sub_addr: SocketAddr = "127.0.0.1:4444".parse().unwrap();

                        let mut rng = rand::thread_rng();
                        let rnd: u32 = rng.gen();
                        let path_str = format!("database_{}.db", rnd);

                        let database_path = PathBuf::from(path_str.as_str());
                        let database_path = join_config_path(&database_path).unwrap();
                        let rocks = Rocks::new(&database_path).unwrap();

                        let slabstore = RocksColumn::<columns::Slabs>::new(rocks.clone());

                        // create gateway client
                        let mut client = GatewayClient::new(connect_addr, slabstore).unwrap();

                        // start subscribing
                        let gateway_slabs_sub: GatewaySlabsSubscriber =
                            client.start_subscriber(sub_addr, ex.clone()).await.unwrap();

                        ex.clone()
                            .spawn(subscribe(gateway_slabs_sub, "B".to_string()))
                            .detach();

                        // start gateway client
                        client.start().await.unwrap();

                        // sleep for 2 seconds
                        std::thread::sleep(std::time::Duration::from_secs(2));
                    });
                    drop(signal);
                    Ok::<(), drk::Error>(())
                });
        });

        thread_pools.push(thread);
        thread_pools.push(thread2);

        for t in thread_pools {
            t.join().unwrap();
        }
    }

    #[test]
    fn test_deposit() {
        setup_log();

        let mut thread_pools: Vec<std::thread::JoinHandle<()>> = vec![];

        // Client A: User
        let thread = std::thread::spawn(|| {
            smol::future::block_on(async move {
                let connect_addr: SocketAddr = "127.0.0.1:3333".parse().unwrap();

                let mut rng = rand::thread_rng();
                let rnd: u32 = rng.gen();
                let path_str = format!("database_{}.db", rnd);

                let database_path = PathBuf::from(path_str.as_str());
                let database_path = join_config_path(&database_path).unwrap();
                let rocks = Rocks::new(&database_path).unwrap();

                let slabstore = RocksColumn::<columns::Slabs>::new(rocks.clone());

                // create gateway client
                let mut client = GatewayClient::new(connect_addr, slabstore).unwrap();

                // start gateway client
                client.start().await.unwrap();

                let slab = Slab::new(rnd.to_le_bytes().to_vec());

                client.put_slab(slab.clone()).await.unwrap();
                client.put_slab(slab.clone()).await.unwrap();
                client.put_slab(slab.clone()).await.unwrap();
                client.put_slab(slab.clone()).await.unwrap();
                client.put_slab(slab.clone()).await.unwrap();
                client.put_slab(slab.clone()).await.unwrap();
                client.put_slab(slab.clone()).await.unwrap();
                client.put_slab(slab.clone()).await.unwrap();
                client.put_slab(slab.clone()).await.unwrap();
                client.put_slab(slab.clone()).await.unwrap();
            });
        });
        // Client B: Cashier
        let thread2 = std::thread::spawn(|| {
            let ex = Arc::new(Executor::new());
            let (signal, shutdown) = async_channel::unbounded::<()>();

            let ex2 = ex.clone();

            let (_, _) = Parallel::new()
                // Run four executor threads.
                .each(0..3, |_| smol::future::block_on(ex2.run(shutdown.recv())))
                // Run the main future on the current thread.
                .finish(|| {
                    smol::future::block_on(async move {
                        let connect_addr: SocketAddr = "127.0.0.1:3333".parse().unwrap();
                        let sub_addr: SocketAddr = "127.0.0.1:4444".parse().unwrap();

                        let mut rng = rand::thread_rng();
                        let rnd: u32 = rng.gen();
                        let path_str = format!("database_{}.db", rnd);

                        let database_path = PathBuf::from(path_str.as_str());
                        let database_path = join_config_path(&database_path).unwrap();
                        let rocks = Rocks::new(&database_path).unwrap();

                        let slabstore = RocksColumn::<columns::Slabs>::new(rocks.clone());

                        // create gateway client
                        let mut client = GatewayClient::new(connect_addr, slabstore).unwrap();

                        // start subscribing
                        let gateway_slabs_sub: GatewaySlabsSubscriber =
                            client.start_subscriber(sub_addr, ex.clone()).await.unwrap();

                        ex.clone()
                            .spawn(subscribe(gateway_slabs_sub, "B".to_string()))
                            .detach();

                        // start gateway client
                        client.start().await.unwrap();

                        // sleep for 2 seconds
                        std::thread::sleep(std::time::Duration::from_secs(2));
                    });
                    drop(signal);
                    Ok::<(), drk::Error>(())
                });
        });

        thread_pools.push(thread);
        thread_pools.push(thread2);

        for t in thread_pools {
            t.join().unwrap();
        }
    }
}
