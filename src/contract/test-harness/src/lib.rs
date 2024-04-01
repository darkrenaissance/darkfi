/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{
    collections::HashMap,
    io::{Cursor, Write},
};

use darkfi::{
    blockchain::{BlockInfo, BlockchainOverlay},
    runtime::vm_runtime::Runtime,
    tx::Transaction,
    util::{pcg::Pcg32, time::Timestamp},
    validator::{Validator, ValidatorConfig, ValidatorPtr},
    zk::{empty_witnesses, halo2::Field, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_dao_contract::model::{DaoBulla, DaoProposalBulla};
use darkfi_money_contract::client::OwnCoin;
use darkfi_sdk::{
    bridgetree,
    crypto::{Keypair, MerkleNode, MerkleTree},
    pasta::pallas,
};
use darkfi_serial::{Encodable, WriteExt};
use log::debug;
use num_bigint::BigUint;

/// Utility module for caching ZK proof PKs and VKs
pub mod vks;

/// `Money::PoWReward` functionality
mod money_pow_reward;

/// `Money::Fee` functionality
mod money_fee;

/// `Money::GenesisMint` functionality
mod money_genesis_mint;

/// `Money::Transfer` functionality
mod money_transfer;

/// `Money::TokenMint` functionality
mod money_token;

/// `Money::OtcSwap` functionality
mod money_otc_swap;

/// `Deployooor::Deploy` functionality
mod contract_deploy;

/// `Dao::Mint` functionality
mod dao_mint;

/// `Dao::Propose` functionality
mod dao_propose;

/// `Dao::Vote` functionality
mod dao_vote;

/// `Dao::Exec` functionality
mod dao_exec;

/// Initialize the logging mechanism
pub fn init_logger() {
    let mut cfg = simplelog::ConfigBuilder::new();
    cfg.add_filter_ignore("sled".to_string());
    //cfg.set_target_level(simplelog::LevelFilter::Error);

    // We check this error so we can execute same file tests in parallel,
    // otherwise second one fails to init logger here.
    if simplelog::TermLogger::init(
        simplelog::LevelFilter::Info,
        //simplelog::LevelFilter::Debug,
        //simplelog::LevelFilter::Trace,
        cfg.build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )
    .is_err()
    {
        debug!(target: "test_harness", "Logger initialized");
    }
}

/// Enum representing available wallet holders
#[derive(Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub enum Holder {
    Alice,
    Bob,
    Charlie,
    Dao,
    Rachel,
}

/// Wallet instance for a single [`Holder`]
pub struct Wallet {
    /// Main holder keypair
    pub keypair: Keypair,
    /// Keypair for arbitrary token minting
    pub token_mint_authority: Keypair,
    /// Keypair for arbitrary contract deployment
    pub contract_deploy_authority: Keypair,
    /// Holder's [`Validator`] instance
    pub validator: ValidatorPtr,
    /// Holder's instance of the Merkle tree for the `Money` contract
    pub money_merkle_tree: MerkleTree,
    /// Holder's instance of the Merkle tree for the `DAO` contract (holding DAO bullas)
    pub dao_merkle_tree: MerkleTree,
    /// Holder's instance of the Merkle tree for the `DAO` contract (holding DAO proposals)
    pub dao_proposals_tree: MerkleTree,
    /// Holder's set of unspent [`OwnCoin`]s from the `Money` contract
    pub unspent_money_coins: Vec<OwnCoin>,
    /// Holder's set of spent [`OwnCoin`]s from the `Money` contract
    pub spent_money_coins: Vec<OwnCoin>,
    /// Witnessed leaf positions of DAO bullas in the `dao_merkle_tree`
    pub dao_leafs: HashMap<DaoBulla, bridgetree::Position>,
    /// Dao Proposal snapshots
    pub dao_prop_leafs: HashMap<DaoProposalBulla, (bridgetree::Position, MerkleTree)>,
    /// Create bench.csv file
    pub bench_wasm: bool,
}

impl Wallet {
    /// Instantiate a new [`Wallet`] instance
    pub async fn new(
        keypair: Keypair,
        token_mint_authority: Keypair,
        contract_deploy_authority: Keypair,
        genesis_block: BlockInfo,
        vks: &vks::Vks,
        verify_fees: bool,
    ) -> Result<Self> {
        // Create an in-memory sled db instance for this wallet
        let sled_db = sled::Config::new().temporary(true).open()?;

        // Inject the cached VKs into the database
        vks::inject(&sled_db, vks)?;

        // Create the `Validator` instance
        let validator_config = ValidatorConfig {
            finalization_threshold: 3,
            pow_target: 90,
            pow_fixed_difficulty: Some(BigUint::from(1_u8)),
            genesis_block,
            verify_fees,
        };
        let validator = Validator::new(&sled_db, validator_config).await?;

        // The Merkle tree for the `Money` contract is initialized with a "null"
        // leaf at position 0.
        let mut money_merkle_tree = MerkleTree::new(100);
        money_merkle_tree.append(MerkleNode::from(pallas::Base::ZERO));
        money_merkle_tree.mark().unwrap();

        Ok(Self {
            keypair,
            token_mint_authority,
            contract_deploy_authority,
            validator,
            money_merkle_tree,
            dao_merkle_tree: MerkleTree::new(100),
            dao_proposals_tree: MerkleTree::new(100),
            unspent_money_coins: vec![],
            spent_money_coins: vec![],
            dao_leafs: HashMap::new(),
            dao_prop_leafs: HashMap::new(),
            bench_wasm: false,
        })
    }

    pub async fn add_transaction(
        &mut self,
        callname: &str,
        tx: Transaction,
        block_height: u64,
        verify_fees: bool,
    ) -> Result<()> {
        if self.bench_wasm {
            benchmark_wasm_calls(callname, &self.validator, &tx, block_height);
        }

        self.validator.add_transactions(&[tx], block_height, true, verify_fees).await?;
        Ok(())
    }
}

/// Native contract test harness instance
pub struct TestHarness {
    /// Initialized [`Holder`]s for this instance
    pub holders: HashMap<Holder, Wallet>,
    /// Cached [`ProvingKey`]s for native contract ZK proving
    pub proving_keys: HashMap<String, (ProvingKey, ZkBinary)>,
    /// The genesis block for this harness
    pub genesis_block: BlockInfo,
    /// Marker to know if we're supposed to include tx fees
    pub verify_fees: bool,
}

impl TestHarness {
    /// Instantiate a new [`TestHarness`] given a slice of [`Holder`]s.
    /// Additionally, a `verify_fees` boolean will enforce tx fee verification.
    pub async fn new(holders: &[Holder], verify_fees: bool) -> Result<Self> {
        // Create a genesis block
        let mut genesis_block = BlockInfo::default();
        genesis_block.header.timestamp = Timestamp::from_u64(1689772567);
        let producer_tx = genesis_block.txs.pop().unwrap();
        genesis_block.append_txs(vec![producer_tx]);

        // Deterministic PRNG
        let mut rng = Pcg32::new(42);

        // Build or read cached ZK PKs and VKs
        let (pks, vks) = vks::get_cached_pks_and_vks()?;
        let mut proving_keys = HashMap::new();
        for (bincode, namespace, pk) in pks {
            let mut reader = Cursor::new(pk);
            let zkbin = ZkBinary::decode(&bincode)?;
            let circuit = ZkCircuit::new(empty_witnesses(&zkbin)?, &zkbin);
            let proving_key = ProvingKey::read(&mut reader, circuit)?;
            proving_keys.insert(namespace, (proving_key, zkbin));
        }

        // Create `Wallet` instances
        let mut holders_map = HashMap::new();
        for holder in holders {
            let keypair = Keypair::random(&mut rng);
            let token_mint_authority = Keypair::random(&mut rng);
            let contract_deploy_authority = Keypair::random(&mut rng);

            let wallet = Wallet::new(
                keypair,
                token_mint_authority,
                contract_deploy_authority,
                genesis_block.clone(),
                &vks,
                verify_fees,
            )
            .await?;

            holders_map.insert(*holder, wallet);
        }

        Ok(Self { holders: holders_map, proving_keys, genesis_block, verify_fees })
    }

    /// Assert that all holders' trees are the same
    pub fn assert_trees(&self, holders: &[Holder]) {
        assert!(holders.len() > 1);
        // Gather wallets
        let mut wallets = vec![];
        for holder in holders {
            wallets.push(self.holders.get(holder).unwrap());
        }
        // Compare trees
        let wallet = wallets[0];
        let money_root = wallet.money_merkle_tree.root(0).unwrap();
        for wallet in &wallets[1..] {
            assert!(money_root == wallet.money_merkle_tree.root(0).unwrap());
        }
    }
}

fn benchmark_wasm_calls(
    callname: &str,
    validator: &Validator,
    tx: &Transaction,
    block_height: u64,
) {
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open("bench.csv").unwrap();

    for (idx, call) in tx.calls.iter().enumerate() {
        let overlay = BlockchainOverlay::new(&validator.blockchain).expect("blockchain overlay");
        let wasm = overlay.lock().unwrap().wasm_bincode.get(call.data.contract_id).unwrap();
        let mut runtime = Runtime::new(&wasm, overlay.clone(), call.data.contract_id, block_height)
            .expect("runtime");
        let mut payload = vec![];
        payload.write_u32(idx as u32).unwrap(); // Call index
        tx.calls.encode(&mut payload).unwrap(); // Actual call data

        let mut times = [0; 3];
        let now = std::time::Instant::now();
        let _metadata = runtime.metadata(&payload).expect("metadata");
        times[0] = now.elapsed().as_micros();

        let now = std::time::Instant::now();
        let update = runtime.exec(&payload).expect("exec");
        times[1] = now.elapsed().as_micros();

        let now = std::time::Instant::now();
        runtime.apply(&update).expect("update");
        times[2] = now.elapsed().as_micros();

        writeln!(
            file,
            "{}, {}, {}, {}, {}, {}",
            callname,
            tx.hash(),
            idx,
            times[0],
            times[1],
            times[2]
        )
        .unwrap();
    }
}
