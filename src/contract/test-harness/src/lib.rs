/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use std::{collections::HashMap, io::Cursor, time::Instant};

use darkfi::{
    blockchain::BlockInfo,
    tx::Transaction,
    util::{
        pcg::Pcg32,
        time::{TimeKeeper, Timestamp},
    },
    validator::{Validator, ValidatorConfig, ValidatorPtr},
    wallet::{WalletDb, WalletPtr},
    zk::{empty_witnesses, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_dao_contract::model::{DaoBulla, DaoProposalBulla};
use darkfi_money_contract::{
    client::{ConsensusNote, ConsensusOwnCoin, MoneyNote, OwnCoin},
    model::{ConsensusOutput, Output},
};
use darkfi_sdk::{
    blockchain::{PidOutput, PreviousSlot, Slot},
    bridgetree,
    crypto::{
        pasta_prelude::Field, poseidon_hash, Keypair, MerkleNode, MerkleTree, Nullifier, PublicKey,
        SecretKey, TokenId,
    },
    pasta::pallas,
};
use log::{info, warn};
use rand::rngs::OsRng;

mod benchmarks;
use benchmarks::TxActionBenchmarks;
pub mod vks;
use vks::{read_or_gen_vks_and_pks, Vks};

mod consensus_genesis_stake;
mod consensus_proposal;
mod consensus_stake;
mod consensus_unstake;
mod consensus_unstake_request;
mod dao_exec;
mod dao_mint;
mod dao_propose;
mod dao_vote;
mod money_airdrop;
mod money_genesis_mint;
mod money_otc_swap;
mod money_pow_reward;
mod money_token;
mod money_transfer;

pub fn init_logger() {
    let mut cfg = simplelog::ConfigBuilder::new();
    cfg.add_filter_ignore("sled".to_string());

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
        warn!(target: "test_harness", "Logger already initialized");
    }
}

/// Enum representing configured wallet holders
#[derive(Debug, Eq, Hash, PartialEq)]
pub enum Holder {
    Faucet,
    Alice,
    Bob,
    Charlie,
    Rachel,
    Dao,
}

/// Enum representing transaction actions
#[derive(Debug, Eq, Hash, PartialEq)]
pub enum TxAction {
    MoneyAirdrop,
    MoneyTokenMint,
    MoneyTokenFreeze,
    MoneyGenesisMint,
    MoneyTransfer,
    MoneyOtcSwap,
    MoneyPoWReward,
    ConsensusGenesisStake,
    ConsensusStake,
    ConsensusProposal,
    ConsensusUnstakeRequest,
    ConsensusUnstake,
    DaoMint,
    DaoPropose,
    DaoVote,
    DaoExec,
}

pub struct Wallet {
    pub keypair: Keypair,
    pub token_mint_authority: Keypair,
    pub validator: ValidatorPtr,
    pub money_merkle_tree: MerkleTree,
    pub consensus_staked_merkle_tree: MerkleTree,
    pub consensus_unstaked_merkle_tree: MerkleTree,
    pub dao_merkle_tree: MerkleTree,
    pub dao_proposals_tree: MerkleTree,
    pub wallet: WalletPtr,
    pub unspent_money_coins: Vec<OwnCoin>,
    pub spent_money_coins: Vec<OwnCoin>,
    pub dao_leafs: HashMap<DaoBulla, bridgetree::Position>,
    // Here the MerkleTree is the snapshotted Money tree at the time of proposal creation
    pub dao_prop_leafs: HashMap<DaoProposalBulla, (bridgetree::Position, MerkleTree)>,
}

impl Wallet {
    pub async fn new(
        keypair: Keypair,
        genesis_block: &BlockInfo,
        faucet_pubkeys: &[PublicKey],
        vks: &Vks,
    ) -> Result<Self> {
        let wallet = WalletDb::new(None, None)?;
        let sled_db = sled::Config::new().temporary(true).open()?;

        // Use pregenerated vks and get pregenerated pks
        vks::inject(&sled_db, vks)?;

        // Generate validator
        // NOTE: we are not using consensus constants here so we
        // don't get circular dependencies.
        let time_keeper = TimeKeeper::new(genesis_block.header.timestamp, 10, 90, 0);
        let config = ValidatorConfig::new(
            time_keeper,
            genesis_block.clone(),
            0,
            faucet_pubkeys.to_vec(),
            false,
        );
        let validator = Validator::new(&sled_db, config).await?;

        // Create necessary Merkle trees for tracking
        let mut money_merkle_tree = MerkleTree::new(100);
        money_merkle_tree.append(MerkleNode::from(pallas::Base::ZERO));
        let consensus_staked_merkle_tree = MerkleTree::new(100);
        let consensus_unstaked_merkle_tree = MerkleTree::new(100);

        let dao_merkle_tree = MerkleTree::new(100);
        let dao_proposals_tree = MerkleTree::new(100);

        let unspent_money_coins = vec![];
        let spent_money_coins = vec![];

        let token_mint_authority = Keypair::random(&mut OsRng);

        Ok(Self {
            keypair,
            token_mint_authority,
            validator,
            money_merkle_tree,
            consensus_staked_merkle_tree,
            consensus_unstaked_merkle_tree,
            dao_merkle_tree,
            dao_proposals_tree,
            wallet,
            unspent_money_coins,
            spent_money_coins,
            dao_leafs: HashMap::new(),
            dao_prop_leafs: HashMap::new(),
        })
    }
}

pub struct TestHarness {
    pub holders: HashMap<Holder, Wallet>,
    pub proving_keys: HashMap<String, (ProvingKey, ZkBinary)>,
    pub tx_action_benchmarks: HashMap<TxAction, TxActionBenchmarks>,
    pub genesis_block: blake3::Hash,
}

impl TestHarness {
    pub async fn new(_contracts: &[String]) -> Result<Self> {
        let mut holders = HashMap::new();
        let mut genesis_block = BlockInfo::default();
        genesis_block.header.timestamp = Timestamp(1689772567);

        // Deterministic PRNG
        let mut rng = Pcg32::new(42);

        // Build or read precompiled zk pks and vks
        let (pks, vks) = read_or_gen_vks_and_pks()?;

        let mut proving_keys = HashMap::new();
        for (bincode, namespace, pk) in pks {
            let mut reader = Cursor::new(pk);
            let zkbin = ZkBinary::decode(&bincode)?;
            let circuit = ZkCircuit::new(empty_witnesses(&zkbin)?, &zkbin);
            let _pk = ProvingKey::read(&mut reader, circuit)?;
            proving_keys.insert(namespace, (_pk, zkbin));
        }

        let faucet_kp = Keypair::random(&mut rng);
        let faucet_pubkeys = vec![faucet_kp.public];
        let faucet = Wallet::new(faucet_kp, &genesis_block, &faucet_pubkeys, &vks).await?;
        holders.insert(Holder::Faucet, faucet);

        let alice_kp = Keypair::random(&mut rng);
        let alice = Wallet::new(alice_kp, &genesis_block, &faucet_pubkeys, &vks).await?;
        holders.insert(Holder::Alice, alice);

        let bob_kp = Keypair::random(&mut rng);
        let bob = Wallet::new(bob_kp, &genesis_block, &faucet_pubkeys, &vks).await?;
        holders.insert(Holder::Bob, bob);

        let charlie_kp = Keypair::random(&mut rng);
        let charlie = Wallet::new(charlie_kp, &genesis_block, &faucet_pubkeys, &vks).await?;
        holders.insert(Holder::Charlie, charlie);

        let rachel_kp = Keypair::random(&mut rng);
        let rachel = Wallet::new(rachel_kp, &genesis_block, &faucet_pubkeys, &vks).await?;
        holders.insert(Holder::Rachel, rachel);

        let dao_kp = Keypair::random(&mut rng);
        let dao = Wallet::new(dao_kp, &genesis_block, &faucet_pubkeys, &vks).await?;
        holders.insert(Holder::Dao, dao);

        // Build benchmarks map
        let mut tx_action_benchmarks = HashMap::new();
        tx_action_benchmarks.insert(TxAction::MoneyAirdrop, TxActionBenchmarks::default());
        tx_action_benchmarks.insert(TxAction::MoneyTokenMint, TxActionBenchmarks::default());
        tx_action_benchmarks.insert(TxAction::MoneyTokenFreeze, TxActionBenchmarks::default());
        tx_action_benchmarks.insert(TxAction::MoneyGenesisMint, TxActionBenchmarks::default());
        tx_action_benchmarks.insert(TxAction::MoneyOtcSwap, TxActionBenchmarks::default());
        tx_action_benchmarks.insert(TxAction::MoneyTransfer, TxActionBenchmarks::default());
        tx_action_benchmarks.insert(TxAction::MoneyPoWReward, TxActionBenchmarks::default());
        tx_action_benchmarks.insert(TxAction::ConsensusGenesisStake, TxActionBenchmarks::default());
        tx_action_benchmarks.insert(TxAction::ConsensusStake, TxActionBenchmarks::default());
        tx_action_benchmarks.insert(TxAction::ConsensusProposal, TxActionBenchmarks::default());
        tx_action_benchmarks
            .insert(TxAction::ConsensusUnstakeRequest, TxActionBenchmarks::default());
        tx_action_benchmarks.insert(TxAction::ConsensusUnstake, TxActionBenchmarks::default());
        tx_action_benchmarks.insert(TxAction::DaoMint, TxActionBenchmarks::default());
        tx_action_benchmarks.insert(TxAction::DaoPropose, TxActionBenchmarks::default());
        tx_action_benchmarks.insert(TxAction::DaoVote, TxActionBenchmarks::default());
        tx_action_benchmarks.insert(TxAction::DaoExec, TxActionBenchmarks::default());

        Ok(Self {
            holders,
            proving_keys,
            tx_action_benchmarks,
            genesis_block: genesis_block.blockhash(),
        })
    }

    pub async fn execute_erroneous_txs(
        &mut self,
        action: TxAction,
        holder: &Holder,
        txs: &[Transaction],
        slot: u64,
        erroneous: usize,
    ) -> Result<()> {
        let wallet = self.holders.get(holder).unwrap();
        let tx_action_benchmark = self.tx_action_benchmarks.get_mut(&action).unwrap();
        let timer = Instant::now();

        let erroneous_txs = wallet
            .validator
            .read()
            .await
            .add_transactions(txs, slot, false)
            .await
            .err()
            .unwrap()
            .retrieve_erroneous_txs()?;
        assert_eq!(erroneous_txs.len(), erroneous);
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }

    pub fn gather_owncoin(
        &mut self,
        holder: &Holder,
        output: &Output,
        secret_key: Option<SecretKey>,
    ) -> Result<OwnCoin> {
        let wallet = self.holders.get_mut(holder).unwrap();
        let leaf_position = wallet.money_merkle_tree.mark().unwrap();
        let secret_key = match secret_key {
            Some(key) => key,
            None => wallet.keypair.secret,
        };

        let note: MoneyNote = output.note.decrypt(&secret_key)?;
        let oc = OwnCoin {
            coin: output.coin,
            note: note.clone(),
            secret: secret_key,
            nullifier: Nullifier::from(poseidon_hash([wallet.keypair.secret.inner(), note.serial])),
            leaf_position,
        };

        wallet.unspent_money_coins.push(oc.clone());

        Ok(oc)
    }

    /// This should be used after transfer call, so we can mark the merkle tree
    /// before each output coin. Assumes using wallet secret key.
    pub fn gather_multiple_owncoins(
        &mut self,
        holder: &Holder,
        outputs: &[Output],
    ) -> Result<Vec<OwnCoin>> {
        let wallet = self.holders.get_mut(holder).unwrap();
        let secret_key = wallet.keypair.secret;
        let mut owncoins = vec![];
        for output in outputs {
            wallet.money_merkle_tree.append(MerkleNode::from(output.coin.inner()));
            let leaf_position = wallet.money_merkle_tree.mark().unwrap();

            let note: MoneyNote = output.note.decrypt(&secret_key)?;
            let oc = OwnCoin {
                coin: output.coin,
                note: note.clone(),
                secret: secret_key,
                nullifier: Nullifier::from(poseidon_hash([
                    wallet.keypair.secret.inner(),
                    note.serial,
                ])),
                leaf_position,
            };

            wallet.unspent_money_coins.push(oc.clone());
            owncoins.push(oc);
        }

        Ok(owncoins)
    }

    pub fn gather_owncoin_at_index(
        &mut self,
        holder: &Holder,
        outputs: &[Output],
        index: usize,
    ) -> Result<OwnCoin> {
        let wallet = self.holders.get_mut(holder).unwrap();
        let secret_key = wallet.keypair.secret;
        let mut owncoin = None;
        for (i, output) in outputs.iter().enumerate() {
            wallet.money_merkle_tree.append(MerkleNode::from(output.coin.inner()));
            if i == index {
                let leaf_position = wallet.money_merkle_tree.mark().unwrap();

                let note: MoneyNote = output.note.decrypt(&secret_key)?;
                let oc = OwnCoin {
                    coin: output.coin,
                    note: note.clone(),
                    secret: secret_key,
                    nullifier: Nullifier::from(poseidon_hash([
                        wallet.keypair.secret.inner(),
                        note.serial,
                    ])),
                    leaf_position,
                };

                wallet.unspent_money_coins.push(oc.clone());
                owncoin = Some(oc);
            }
        }

        Ok(owncoin.unwrap())
    }

    pub fn gather_consensus_staked_owncoin(
        &mut self,
        holder: &Holder,
        output: &ConsensusOutput,
        secret_key: Option<SecretKey>,
    ) -> Result<ConsensusOwnCoin> {
        let wallet = self.holders.get_mut(holder).unwrap();
        let leaf_position = wallet.consensus_staked_merkle_tree.mark().unwrap();
        let secret_key = match secret_key {
            Some(key) => key,
            None => wallet.keypair.secret,
        };
        let note: ConsensusNote = output.note.decrypt(&secret_key)?;
        let oc = ConsensusOwnCoin {
            coin: output.coin,
            note: note.clone(),
            secret: secret_key,
            nullifier: Nullifier::from(poseidon_hash([wallet.keypair.secret.inner(), note.serial])),
            leaf_position,
        };

        Ok(oc)
    }

    pub fn gather_consensus_unstaked_owncoin(
        &mut self,
        holder: &Holder,
        output: &ConsensusOutput,
        secret_key: Option<SecretKey>,
    ) -> Result<ConsensusOwnCoin> {
        let wallet = self.holders.get_mut(holder).unwrap();
        let leaf_position = wallet.consensus_unstaked_merkle_tree.mark().unwrap();
        let secret_key = match secret_key {
            Some(key) => key,
            None => wallet.keypair.secret,
        };
        let note: ConsensusNote = output.note.decrypt(&secret_key)?;
        let oc = ConsensusOwnCoin {
            coin: output.coin,
            note: note.clone(),
            secret: secret_key,
            nullifier: Nullifier::from(poseidon_hash([wallet.keypair.secret.inner(), note.serial])),
            leaf_position,
        };

        Ok(oc)
    }

    pub async fn get_slot_by_slot(&self, slot: u64) -> Result<Slot> {
        let faucet = self.holders.get(&Holder::Faucet).unwrap();
        let slot =
            faucet.validator.read().await.blockchain.get_slots_by_id(&[slot])?[0].clone().unwrap();

        Ok(slot)
    }

    pub async fn generate_slot(&self, id: u64) -> Result<Slot> {
        // We grab the genesis slot to generate slot
        // using same consensus parameters
        let genesis_block = self.genesis_block;
        let genesis_slot = self.get_slot_by_slot(0).await?;
        let previous = PreviousSlot::new(0, vec![genesis_block], vec![genesis_block], 0.0);
        let pid = PidOutput::new(0.0, 0.0, genesis_slot.pid.sigma1, genesis_slot.pid.sigma2);
        let slot = Slot::new(id, previous, pid, genesis_slot.last_eta, 0, 0);

        // Store generated slot
        for wallet in self.holders.values() {
            wallet.validator.write().await.receive_test_slot(&slot).await?;
        }

        Ok(slot)
    }

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
        let consensus_stake_root = wallet.consensus_staked_merkle_tree.root(0).unwrap();
        let consensus_unstake_root = wallet.consensus_unstaked_merkle_tree.root(0).unwrap();
        for wallet in &wallets[1..] {
            assert!(money_root == wallet.money_merkle_tree.root(0).unwrap());
            assert!(consensus_stake_root == wallet.consensus_staked_merkle_tree.root(0).unwrap());
            assert!(
                consensus_unstake_root == wallet.consensus_unstaked_merkle_tree.root(0).unwrap()
            );
        }
    }

    pub fn token_id(&self, holder: &Holder) -> TokenId {
        let holder = self.holders.get(holder).unwrap();
        TokenId::derive_public(holder.token_mint_authority.public)
    }

    pub fn statistics(&self) {
        info!("==================== Statistics ====================");
        for (action, tx_action_benchmark) in &self.tx_action_benchmarks {
            tx_action_benchmark.statistics(action);
        }
        info!("====================================================");
    }
}
