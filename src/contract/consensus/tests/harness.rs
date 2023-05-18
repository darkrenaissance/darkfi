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
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use darkfi::{
    consensus::{
        SlotCheckpoint, ValidatorState, ValidatorStatePtr, TESTNET_BOOTSTRAP_TIMESTAMP,
        TESTNET_GENESIS_HASH_BYTES, TESTNET_GENESIS_TIMESTAMP, TESTNET_INITIAL_DISTRIBUTION,
    },
    runtime::vm_runtime::SMART_CONTRACT_ZKAS_DB_NAME,
    tx::Transaction,
    wallet::{WalletDb, WalletPtr},
    zk::{empty_witnesses, halo2::Field, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_money_contract::client::OwnCoin;
use darkfi_sdk::{
    crypto::{
        merkle_prelude::*, Keypair, MerkleNode, MerkleTree, PublicKey, CONSENSUS_CONTRACT_ID,
        DARK_TOKEN_ID, MONEY_CONTRACT_ID,
    },
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable};
use log::{info, warn};
use rand::rngs::OsRng;

use darkfi_consensus_contract::{
    client::{
        proposal_v1::ConsensusProposalCallBuilder, stake_v1::ConsensusStakeCallBuilder,
        unstake_v1::ConsensusUnstakeCallBuilder,
    },
    model::ConsensusProposalMintParamsV1,
    ConsensusFunction,
};
use darkfi_money_contract::{
    client::{
        stake_v1::MoneyStakeCallBuilder, transfer_v1::TransferCallBuilder,
        unstake_v1::MoneyUnstakeCallBuilder,
    },
    model::{ConsensusStakeParamsV1, MoneyTransferParamsV1, MoneyUnstakeParamsV1},
    MoneyFunction, CONSENSUS_CONTRACT_ZKAS_PROPOSAL_MINT_NS_V1,
    CONSENSUS_CONTRACT_ZKAS_PROPOSAL_REWARD_NS_V1, MONEY_CONTRACT_ZKAS_BURN_NS_V1,
    MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};

pub fn init_logger() {
    let mut cfg = simplelog::ConfigBuilder::new();
    cfg.add_filter_ignore("sled".to_string());
    cfg.add_filter_ignore("blockchain::contractstore".to_string());
    // We check this error so we can execute same file tests in parallel,
    // otherwise second one fails to init logger here.
    if let Err(_) = simplelog::TermLogger::init(
        //simplelog::LevelFilter::Info,
        simplelog::LevelFilter::Debug,
        //simplelog::LevelFilter::Trace,
        cfg.build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    ) {
        warn!(target: "money_harness", "Logger already initialized");
    }
}

/// Enum representing configured wallet holders
#[derive(Debug, Eq, Hash, PartialEq)]
pub enum Holder {
    Faucet,
    Alice,
}

/// Enum representing transaction actions
#[derive(Debug, Eq, Hash, PartialEq)]
pub enum TxAction {
    Airdrop,
    Stake,
    Proposal,
    Unstake,
}

/// Auxiliary struct to calculate transaction actions benchmarks
pub struct TxActionBenchmarks {
    /// Vector holding each transaction size in Bytes
    pub sizes: Vec<usize>,
    /// Vector holding each transaction broadcasted size in Bytes
    pub broadcasted_sizes: Vec<usize>,
    /// Vector holding each transaction creation time
    pub creation_times: Vec<Duration>,
    /// Vector holding each transaction verify time
    pub verify_times: Vec<Duration>,
}

impl TxActionBenchmarks {
    pub fn new() -> Self {
        Self {
            sizes: vec![],
            broadcasted_sizes: vec![],
            creation_times: vec![],
            verify_times: vec![],
        }
    }

    pub fn statistics(&self, action: &TxAction) {
        let avg = self.sizes.iter().sum::<usize>();
        let avg = avg / self.sizes.len();
        info!("Average {:?} size: {:?} Bytes", action, avg);
        let avg = self.broadcasted_sizes.iter().sum::<usize>();
        let avg = avg / self.broadcasted_sizes.len();
        info!("Average {:?} broadcasted size: {:?} Bytes", action, avg);
        let avg = self.creation_times.iter().sum::<Duration>();
        let avg = avg / self.creation_times.len() as u32;
        info!("Average {:?} creation time: {:?}", action, avg);
        let avg = self.verify_times.iter().sum::<Duration>();
        let avg = avg / self.verify_times.len() as u32;
        info!("Average {:?} verification time: {:?}", action, avg);
    }
}

pub struct Wallet {
    pub keypair: Keypair,
    pub state: ValidatorStatePtr,
    pub merkle_tree: MerkleTree,
    pub consensus_merkle_tree: MerkleTree,
    pub wallet: WalletPtr,
    pub coins: Vec<OwnCoin>,
    pub spent_coins: Vec<OwnCoin>,
}

impl Wallet {
    async fn new(keypair: Keypair, faucet_pubkeys: &[PublicKey]) -> Result<Self> {
        let wallet = WalletDb::new("sqlite::memory:", "foo").await?;
        let sled_db = sled::Config::new().temporary(true).open()?;

        let state = ValidatorState::new(
            &sled_db,
            *TESTNET_BOOTSTRAP_TIMESTAMP,
            *TESTNET_GENESIS_TIMESTAMP,
            *TESTNET_GENESIS_HASH_BYTES,
            *TESTNET_INITIAL_DISTRIBUTION,
            wallet.clone(),
            faucet_pubkeys.to_vec(),
            false,
            false,
        )
        .await?;

        let merkle_tree = MerkleTree::new(100);
        let consensus_merkle_tree = MerkleTree::new(100);

        let coins = vec![];
        let spent_coins = vec![];

        Ok(Self { keypair, state, merkle_tree, consensus_merkle_tree, wallet, coins, spent_coins })
    }
}

pub struct ConsensusTestHarness {
    pub faucet: Wallet,
    pub alice: Wallet,
    pub proving_keys: HashMap<&'static str, (ProvingKey, ZkBinary)>,
    pub tx_action_benchmarks: HashMap<TxAction, TxActionBenchmarks>,
}

impl ConsensusTestHarness {
    pub async fn new() -> Result<Self> {
        let faucet_kp = Keypair::random(&mut OsRng);
        let faucet_pubkeys = vec![faucet_kp.public];
        let faucet = Wallet::new(faucet_kp, &faucet_pubkeys).await?;

        let alice_kp = Keypair::random(&mut OsRng);
        let alice = Wallet::new(alice_kp, &faucet_pubkeys).await?;

        // Get the zkas circuits and build proving keys
        let mut proving_keys = HashMap::new();
        let alice_sled = alice.state.read().await.blockchain.sled_db.clone();
        let mut db_handle = alice.state.read().await.blockchain.contracts.lookup(
            &alice_sled,
            &MONEY_CONTRACT_ID,
            SMART_CONTRACT_ZKAS_DB_NAME,
        )?;

        macro_rules! mkpk {
            ($ns:expr) => {
                let zkas_bytes = db_handle.get(&serialize(&$ns))?.unwrap();
                let (zkbin, _): (Vec<u8>, Vec<u8>) = deserialize(&zkas_bytes)?;
                let zkbin = ZkBinary::decode(&zkbin)?;
                let witnesses = empty_witnesses(&zkbin);
                let circuit = ZkCircuit::new(witnesses, zkbin.clone());
                let pk = ProvingKey::build(13, &circuit);
                proving_keys.insert($ns, (pk, zkbin));
            };
        }

        mkpk!(MONEY_CONTRACT_ZKAS_MINT_NS_V1);
        mkpk!(MONEY_CONTRACT_ZKAS_BURN_NS_V1);

        db_handle = alice.state.read().await.blockchain.contracts.lookup(
            &alice_sled,
            &CONSENSUS_CONTRACT_ID,
            SMART_CONTRACT_ZKAS_DB_NAME,
        )?;
        mkpk!(MONEY_CONTRACT_ZKAS_MINT_NS_V1);
        mkpk!(MONEY_CONTRACT_ZKAS_BURN_NS_V1);
        mkpk!(CONSENSUS_CONTRACT_ZKAS_PROPOSAL_REWARD_NS_V1);
        mkpk!(CONSENSUS_CONTRACT_ZKAS_PROPOSAL_MINT_NS_V1);

        // Build benchmarks map
        let mut tx_action_benchmarks = HashMap::new();
        tx_action_benchmarks.insert(TxAction::Airdrop, TxActionBenchmarks::new());
        tx_action_benchmarks.insert(TxAction::Stake, TxActionBenchmarks::new());
        tx_action_benchmarks.insert(TxAction::Proposal, TxActionBenchmarks::new());
        tx_action_benchmarks.insert(TxAction::Unstake, TxActionBenchmarks::new());

        Ok(Self { faucet, alice, proving_keys, tx_action_benchmarks })
    }

    pub fn airdrop_native(
        &mut self,
        value: u64,
        recipient: PublicKey,
    ) -> Result<(Transaction, MoneyTransferParamsV1)> {
        let (mint_pk, mint_zkbin) = self.proving_keys.get(&MONEY_CONTRACT_ZKAS_MINT_NS_V1).unwrap();
        let (burn_pk, burn_zkbin) = self.proving_keys.get(&MONEY_CONTRACT_ZKAS_BURN_NS_V1).unwrap();
        let tx_action_benchmark = self.tx_action_benchmarks.get_mut(&TxAction::Airdrop).unwrap();
        let timer = Instant::now();

        let builder = TransferCallBuilder {
            keypair: self.faucet.keypair,
            recipient,
            value,
            token_id: *DARK_TOKEN_ID,
            rcpt_spend_hook: pallas::Base::zero(),
            rcpt_user_data: pallas::Base::zero(),
            rcpt_user_data_blind: pallas::Base::random(&mut OsRng),
            change_spend_hook: pallas::Base::zero(),
            change_user_data: pallas::Base::zero(),
            change_user_data_blind: pallas::Base::random(&mut OsRng),
            coins: vec![],
            tree: self.faucet.merkle_tree.clone(),
            mint_zkbin: mint_zkbin.clone(),
            mint_pk: mint_pk.clone(),
            burn_zkbin: burn_zkbin.clone(),
            burn_pk: burn_pk.clone(),
            clear_input: true,
        };

        let debris = builder.build()?;

        let mut data = vec![MoneyFunction::TransferV1 as u8];
        debris.params.encode(&mut data)?;
        let calls = vec![ContractCall { contract_id: *MONEY_CONTRACT_ID, data }];
        let proofs = vec![debris.proofs];
        let mut tx = Transaction { calls, proofs, signatures: vec![] };
        let sigs = tx.create_sigs(&mut OsRng, &debris.signature_secrets)?;
        tx.signatures = vec![sigs];
        tx_action_benchmark.creation_times.push(timer.elapsed());

        // Calculate transaction sizes
        let encoded: Vec<u8> = serialize(&tx);
        let size = ::std::mem::size_of_val(&*encoded);
        tx_action_benchmark.sizes.push(size);
        let base58 = bs58::encode(&encoded).into_string();
        let size = ::std::mem::size_of_val(&*base58);
        tx_action_benchmark.broadcasted_sizes.push(size);

        Ok((tx, debris.params))
    }

    pub async fn execute_airdrop_native_tx(
        &mut self,
        holder: Holder,
        tx: Transaction,
        params: &MoneyTransferParamsV1,
        slot: u64,
    ) -> Result<()> {
        let wallet = match holder {
            Holder::Faucet => &mut self.faucet,
            Holder::Alice => &mut self.alice,
        };
        let tx_action_benchmark = self.tx_action_benchmarks.get_mut(&TxAction::Airdrop).unwrap();
        let timer = Instant::now();

        let erroneous_txs =
            wallet.state.read().await.verify_transactions(&[tx], slot, true).await?;
        assert!(erroneous_txs.is_empty());
        wallet.merkle_tree.append(&MerkleNode::from(params.outputs[0].coin.inner()));
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }

    pub fn stake_native(
        &mut self,
        holder: Holder,
        owncoin: OwnCoin,
    ) -> Result<(Transaction, ConsensusStakeParamsV1)> {
        let wallet = match holder {
            Holder::Faucet => &self.faucet,
            Holder::Alice => &self.alice,
        };
        let (mint_pk, mint_zkbin) = self.proving_keys.get(&MONEY_CONTRACT_ZKAS_MINT_NS_V1).unwrap();
        let (burn_pk, burn_zkbin) = self.proving_keys.get(&MONEY_CONTRACT_ZKAS_BURN_NS_V1).unwrap();
        let tx_action_benchmark = self.tx_action_benchmarks.get_mut(&TxAction::Stake).unwrap();
        let timer = Instant::now();

        // Building Money::Stake params
        let money_stake_call_debris = MoneyStakeCallBuilder {
            coin: owncoin.clone(),
            tree: wallet.merkle_tree.clone(),
            burn_zkbin: burn_zkbin.clone(),
            burn_pk: burn_pk.clone(),
        }
        .build()?;
        let (
            money_stake_params,
            money_stake_proofs,
            money_stake_secret_key,
            money_stake_value_blind,
        ) = (
            money_stake_call_debris.params,
            money_stake_call_debris.proofs,
            money_stake_call_debris.signature_secret,
            money_stake_call_debris.value_blind,
        );

        // Building Consensus::Stake params
        let consensus_stake_call_debris = ConsensusStakeCallBuilder {
            coin: owncoin,
            recipient: wallet.keypair.public,
            value_blind: money_stake_value_blind,
            token_blind: money_stake_params.token_blind,
            nullifier: money_stake_params.input.nullifier,
            merkle_root: money_stake_params.input.merkle_root,
            signature_public: money_stake_params.input.signature_public,
            mint_zkbin: mint_zkbin.clone(),
            mint_pk: mint_pk.clone(),
        }
        .build()?;
        let (consensus_stake_params, consensus_stake_proofs) =
            (consensus_stake_call_debris.params, consensus_stake_call_debris.proofs);

        // Building stake tx
        let mut data = vec![MoneyFunction::StakeV1 as u8];
        money_stake_params.encode(&mut data)?;
        let money_call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };

        let mut data = vec![ConsensusFunction::StakeV1 as u8];
        consensus_stake_params.encode(&mut data)?;
        let consensus_call = ContractCall { contract_id: *CONSENSUS_CONTRACT_ID, data };

        let calls = vec![money_call, consensus_call];
        let proofs = vec![money_stake_proofs, consensus_stake_proofs];
        let mut stake_tx = Transaction { calls, proofs, signatures: vec![] };
        let money_sigs = stake_tx.create_sigs(&mut OsRng, &[money_stake_secret_key])?;
        let consensus_sigs = stake_tx.create_sigs(&mut OsRng, &[money_stake_secret_key])?;
        stake_tx.signatures = vec![money_sigs, consensus_sigs];
        tx_action_benchmark.creation_times.push(timer.elapsed());

        // Calculate transaction sizes
        let encoded: Vec<u8> = serialize(&stake_tx);
        let size = ::std::mem::size_of_val(&*encoded);
        tx_action_benchmark.sizes.push(size);
        let base58 = bs58::encode(&encoded).into_string();
        let size = ::std::mem::size_of_val(&*base58);
        tx_action_benchmark.broadcasted_sizes.push(size);

        Ok((stake_tx, consensus_stake_params))
    }

    pub async fn execute_stake_native_tx(
        &mut self,
        holder: Holder,
        tx: Transaction,
        params: &ConsensusStakeParamsV1,
        slot: u64,
    ) -> Result<()> {
        let wallet = match holder {
            Holder::Faucet => &mut self.faucet,
            Holder::Alice => &mut self.alice,
        };
        let tx_action_benchmark = self.tx_action_benchmarks.get_mut(&TxAction::Stake).unwrap();
        let timer = Instant::now();

        let erroneous_txs =
            wallet.state.read().await.verify_transactions(&[tx], slot, true).await?;
        assert!(erroneous_txs.is_empty());
        wallet.consensus_merkle_tree.append(&MerkleNode::from(params.output.coin.inner()));
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }

    pub fn proposal(
        &mut self,
        holder: Holder,
        slot_checkpoint: SlotCheckpoint,
        staked_oc: OwnCoin,
    ) -> Result<(Transaction, ConsensusProposalMintParamsV1)> {
        let wallet = match holder {
            Holder::Faucet => &self.faucet,
            Holder::Alice => &self.alice,
        };
        let (burn_pk, burn_zkbin) = self.proving_keys.get(&MONEY_CONTRACT_ZKAS_BURN_NS_V1).unwrap();
        let (reward_pk, reward_zkbin) =
            self.proving_keys.get(&CONSENSUS_CONTRACT_ZKAS_PROPOSAL_REWARD_NS_V1).unwrap();
        let (mint_pk, mint_zkbin) =
            self.proving_keys.get(&CONSENSUS_CONTRACT_ZKAS_PROPOSAL_MINT_NS_V1).unwrap();
        let tx_action_benchmark = self.tx_action_benchmarks.get_mut(&TxAction::Proposal).unwrap();
        let timer = Instant::now();

        // Building Consensus::Unstake params
        let proposal_call_debris = ConsensusProposalCallBuilder {
            coin: staked_oc.clone(),
            recipient: wallet.keypair.public,
            slot_checkpoint,
            tree: wallet.consensus_merkle_tree.clone(),
            burn_zkbin: burn_zkbin.clone(),
            burn_pk: burn_pk.clone(),
            reward_zkbin: reward_zkbin.clone(),
            reward_pk: reward_pk.clone(),
            mint_zkbin: mint_zkbin.clone(),
            mint_pk: mint_pk.clone(),
        }
        .build()?;
        let (
            burn_params,
            burn_proofs,
            reward_params,
            reward_proofs,
            mint_params,
            mint_proofs,
            proposal_secret_key,
        ) = (
            proposal_call_debris.burn_params,
            proposal_call_debris.burn_proofs,
            proposal_call_debris.reward_params,
            proposal_call_debris.reward_proofs,
            proposal_call_debris.mint_params,
            proposal_call_debris.mint_proofs,
            proposal_call_debris.signature_secret,
        );

        // Building proposal tx
        let mut data = vec![ConsensusFunction::ProposalBurnV1 as u8];
        burn_params.encode(&mut data)?;
        let burn_call = ContractCall { contract_id: *CONSENSUS_CONTRACT_ID, data };

        let mut data = vec![ConsensusFunction::ProposalRewardV1 as u8];
        reward_params.encode(&mut data)?;
        let reward_call = ContractCall { contract_id: *CONSENSUS_CONTRACT_ID, data };

        let mut data = vec![ConsensusFunction::ProposalMintV1 as u8];
        mint_params.encode(&mut data)?;
        let mint_call = ContractCall { contract_id: *CONSENSUS_CONTRACT_ID, data };

        let calls = vec![burn_call, reward_call, mint_call];
        let proofs = vec![burn_proofs, reward_proofs, mint_proofs];
        let mut proposal_tx = Transaction { calls, proofs, signatures: vec![] };
        let burn_sigs = proposal_tx.create_sigs(&mut OsRng, &[proposal_secret_key])?;
        let reward_sigs = proposal_tx.create_sigs(&mut OsRng, &[proposal_secret_key])?;
        let mint_sigs = proposal_tx.create_sigs(&mut OsRng, &[proposal_secret_key])?;
        proposal_tx.signatures = vec![burn_sigs, reward_sigs, mint_sigs];
        tx_action_benchmark.creation_times.push(timer.elapsed());

        // Calculate transaction sizes
        let encoded: Vec<u8> = serialize(&proposal_tx);
        let size = ::std::mem::size_of_val(&*encoded);
        tx_action_benchmark.sizes.push(size);
        let base58 = bs58::encode(&encoded).into_string();
        let size = ::std::mem::size_of_val(&*base58);
        tx_action_benchmark.broadcasted_sizes.push(size);

        Ok((proposal_tx, mint_params))
    }

    pub async fn execute_proposal_tx(
        &mut self,
        holder: Holder,
        tx: Transaction,
        params: &ConsensusProposalMintParamsV1,
        slot: u64,
    ) -> Result<()> {
        let wallet = match holder {
            Holder::Faucet => &mut self.faucet,
            Holder::Alice => &mut self.alice,
        };
        let tx_action_benchmark = self.tx_action_benchmarks.get_mut(&TxAction::Proposal).unwrap();
        let timer = Instant::now();

        let erroneous_txs =
            wallet.state.read().await.verify_transactions(&[tx], slot, true).await?;
        assert!(erroneous_txs.is_empty());
        wallet.consensus_merkle_tree.append(&MerkleNode::from(params.output.coin.inner()));
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }

    pub fn unstake_native(
        &mut self,
        holder: Holder,
        staked_oc: OwnCoin,
    ) -> Result<(Transaction, MoneyUnstakeParamsV1)> {
        let wallet = match holder {
            Holder::Faucet => &self.faucet,
            Holder::Alice => &self.alice,
        };
        let (burn_pk, burn_zkbin) = self.proving_keys.get(&MONEY_CONTRACT_ZKAS_BURN_NS_V1).unwrap();
        let (mint_pk, mint_zkbin) = self.proving_keys.get(&MONEY_CONTRACT_ZKAS_MINT_NS_V1).unwrap();
        let tx_action_benchmark = self.tx_action_benchmarks.get_mut(&TxAction::Unstake).unwrap();
        let timer = Instant::now();

        // Building Consensus::Unstake params
        let consensus_unstake_call_debris = ConsensusUnstakeCallBuilder {
            coin: staked_oc.clone(),
            tree: wallet.consensus_merkle_tree.clone(),
            burn_zkbin: burn_zkbin.clone(),
            burn_pk: burn_pk.clone(),
        }
        .build()?;
        let (
            consensus_unstake_params,
            consensus_unstake_proofs,
            consensus_unstake_secret_key,
            consensus_unstake_value_blind,
        ) = (
            consensus_unstake_call_debris.params,
            consensus_unstake_call_debris.proofs,
            consensus_unstake_call_debris.signature_secret,
            consensus_unstake_call_debris.value_blind,
        );

        // Building Money::Unstake params
        let money_unstake_call_debris = MoneyUnstakeCallBuilder {
            coin: staked_oc,
            recipient: wallet.keypair.public,
            value_blind: consensus_unstake_value_blind,
            token_blind: consensus_unstake_params.token_blind,
            nullifier: consensus_unstake_params.input.nullifier,
            merkle_root: consensus_unstake_params.input.merkle_root,
            signature_public: consensus_unstake_params.input.signature_public,
            mint_zkbin: mint_zkbin.clone(),
            mint_pk: mint_pk.clone(),
        }
        .build()?;
        let (money_unstake_params, money_unstake_proofs) =
            (money_unstake_call_debris.params, money_unstake_call_debris.proofs);

        // Building unstake tx
        let mut data = vec![ConsensusFunction::UnstakeV1 as u8];
        consensus_unstake_params.encode(&mut data)?;
        let consensus_call = ContractCall { contract_id: *CONSENSUS_CONTRACT_ID, data };

        let mut data = vec![MoneyFunction::UnstakeV1 as u8];
        money_unstake_params.encode(&mut data)?;
        let money_call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };

        let calls = vec![consensus_call, money_call];
        let proofs = vec![consensus_unstake_proofs, money_unstake_proofs];
        let mut unstake_tx = Transaction { calls, proofs, signatures: vec![] };
        let consensus_sigs = unstake_tx.create_sigs(&mut OsRng, &[consensus_unstake_secret_key])?;
        let money_sigs = unstake_tx.create_sigs(&mut OsRng, &[consensus_unstake_secret_key])?;
        unstake_tx.signatures = vec![consensus_sigs, money_sigs];
        tx_action_benchmark.creation_times.push(timer.elapsed());

        // Calculate transaction sizes
        let encoded: Vec<u8> = serialize(&unstake_tx);
        let size = ::std::mem::size_of_val(&*encoded);
        tx_action_benchmark.sizes.push(size);
        let base58 = bs58::encode(&encoded).into_string();
        let size = ::std::mem::size_of_val(&*base58);
        tx_action_benchmark.broadcasted_sizes.push(size);

        Ok((unstake_tx, money_unstake_params))
    }

    pub async fn execute_unstake_native_tx(
        &mut self,
        holder: Holder,
        tx: Transaction,
        params: &MoneyUnstakeParamsV1,
        slot: u64,
    ) -> Result<()> {
        let wallet = match holder {
            Holder::Faucet => &mut self.faucet,
            Holder::Alice => &mut self.alice,
        };
        let tx_action_benchmark = self.tx_action_benchmarks.get_mut(&TxAction::Unstake).unwrap();
        let timer = Instant::now();

        let erroneous_txs =
            wallet.state.read().await.verify_transactions(&[tx], slot, true).await?;
        assert!(erroneous_txs.is_empty());
        wallet.merkle_tree.append(&MerkleNode::from(params.output.coin.inner()));
        tx_action_benchmark.verify_times.push(timer.elapsed());

        Ok(())
    }

    pub fn statistics(&self) {
        info!("==================== Statistics ====================");
        for (action, tx_action_benchmark) in &self.tx_action_benchmarks {
            tx_action_benchmark.statistics(action);
        }
        info!("====================================================");
    }
}
