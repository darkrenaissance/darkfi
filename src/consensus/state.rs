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

use std::time::Duration;

use chrono::{NaiveDateTime, Utc};
use darkfi_sdk::{
    crypto::{constants::MERKLE_DEPTH, MerkleNode},
    incrementalmerkletree::bridgetree::BridgeTree,
    pasta::{group::ff::PrimeField, pallas},
};
use darkfi_serial::{deserialize, serialize, SerialDecodable, SerialEncodable};
use log::info;
use rand::{thread_rng, Rng};
use sqlx::Row;

use super::{
    constants,
    lead_coin::{LeadCoin, LeadCoinSecrets},
    utils::fbig2base,
    Block, BlockProposal, Float10,
};
use crate::{
    blockchain::Blockchain, net, tx::Transaction, util::time::Timestamp, wallet::WalletPtr, Error,
    Result,
};

use std::{
    fs::File,
    io::{prelude::*, BufWriter},
};

/// This struct represents the information required by the consensus algorithm
pub struct ConsensusState {
    /// Wallet interface
    pub wallet: WalletPtr,
    /// Canonical (finalized) blockchain
    pub blockchain: Blockchain,
    /// Network bootstrap timestamp
    pub bootstrap_ts: Timestamp,
    /// Genesis block creation timestamp
    pub genesis_ts: Timestamp,
    /// Genesis block hash
    pub genesis_block: blake3::Hash,
    /// Total sum of initial staking coins
    pub initial_distribution: u64,
    /// Flag to enable single-node mode
    pub single_node: bool,
    /// Slot the network was bootstrapped
    pub bootstrap_slot: u64,
    /// Participating start slot
    pub participating: Option<u64>,
    /// Node is able to propose proposals
    pub proposing: bool,
    /// Last slot node check for finalization
    pub checked_finalization: u64,
    /// Fork chains containing block proposals
    pub forks: Vec<Fork>,
    /// Current epoch
    pub epoch: u64,
    /// Hot/live slot checkpoints
    pub slot_checkpoints: Vec<SlotCheckpoint>,
    /// Last slot leaders count
    pub previous_leaders: u64,
    /// Controller output history
    pub f_history: Vec<Float10>,
    /// Controller proportional error history
    pub err_history: Vec<Float10>,
    // TODO: Aren't these already in db after finalization?
    /// Canonical competing coins
    pub coins: Vec<LeadCoin>,
    /// Canonical coin commitments tree
    pub coins_tree: BridgeTree<MerkleNode, MERKLE_DEPTH>,
    /// Canonical seen nullifiers from proposals
    pub nullifiers: Vec<pallas::Base>,
}

impl ConsensusState {
    pub fn new(
        wallet: WalletPtr,
        blockchain: Blockchain,
        bootstrap_ts: Timestamp,
        genesis_ts: Timestamp,
        genesis_data: blake3::Hash,
        initial_distribution: u64,
        single_node: bool,
    ) -> Result<Self> {
        let genesis_block = Block::genesis_block(genesis_ts, genesis_data).blockhash();
        Ok(Self {
            wallet,
            blockchain,
            bootstrap_ts,
            genesis_ts,
            genesis_block,
            initial_distribution,
            single_node,
            bootstrap_slot: 0,
            participating: None,
            proposing: false,
            checked_finalization: 0,
            forks: vec![],
            epoch: 0,
            slot_checkpoints: vec![],
            previous_leaders: 0,
            f_history: vec![constants::FLOAT10_ZERO.clone()],
            err_history: vec![constants::FLOAT10_ZERO.clone(), constants::FLOAT10_ZERO.clone()],
            coins: vec![],
            coins_tree: BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(constants::EPOCH_LENGTH * 100),
            nullifiers: vec![],
        })
    }

    /// Calculates current epoch.
    pub fn current_epoch(&self) -> u64 {
        self.slot_epoch(self.current_slot())
    }

    /// Calculates the epoch of the provided slot.
    /// Epoch duration is configured using the `EPOCH_LENGTH` value.
    pub fn slot_epoch(&self, slot: u64) -> u64 {
        slot / constants::EPOCH_LENGTH as u64
    }

    /// Calculates current slot, based on elapsed time from the genesis block.
    /// Slot duration is configured using the `SLOT_TIME` constant.
    pub fn current_slot(&self) -> u64 {
        self.genesis_ts.elapsed() / constants::SLOT_TIME
    }

    /// Calculates the relative number of the provided slot.
    pub fn relative_slot(&self, slot: u64) -> u64 {
        slot % constants::EPOCH_LENGTH as u64
    }

    /// Finds the last slot a proposal or block was generated.
    pub fn last_slot(&self) -> Result<u64> {
        let mut slot = 0;
        for chain in &self.forks {
            for state_checkpoint in &chain.sequence {
                if state_checkpoint.proposal.block.header.slot > slot {
                    slot = state_checkpoint.proposal.block.header.slot;
                }
            }
        }

        // We return here in case proposals exist,
        // so we don't query the sled database.
        if slot > 0 {
            return Ok(slot)
        }

        let (last_slot, _) = self.blockchain.last()?;
        Ok(last_slot)
    }

    /// Calculates seconds until next Nth slot starting time.
    /// Slots duration is configured using the SLOT_TIME constant.
    pub fn next_n_slot_start(&self, n: u64) -> Duration {
        assert!(n > 0);
        let start_time = NaiveDateTime::from_timestamp_opt(self.genesis_ts.0, 0).unwrap();
        let current_slot = self.current_slot() + n;
        let next_slot_start =
            (current_slot * constants::SLOT_TIME) + (start_time.timestamp() as u64);
        let next_slot_start = NaiveDateTime::from_timestamp_opt(next_slot_start as i64, 0).unwrap();
        let current_time = NaiveDateTime::from_timestamp_opt(Utc::now().timestamp(), 0).unwrap();
        let diff = next_slot_start - current_time;

        Duration::new(diff.num_seconds().try_into().unwrap(), 0)
    }

    /// Calculate slots until next Nth epoch.
    /// Epoch duration is configured using the EPOCH_LENGTH value.
    pub fn slots_to_next_n_epoch(&self, n: u64) -> u64 {
        assert!(n > 0);
        let slots_till_next_epoch =
            constants::EPOCH_LENGTH as u64 - self.relative_slot(self.current_slot());
        ((n - 1) * constants::EPOCH_LENGTH as u64) + slots_till_next_epoch
    }

    /// Calculates seconds until next Nth epoch starting time.
    pub fn next_n_epoch_start(&self, n: u64) -> Duration {
        self.next_n_slot_start(self.slots_to_next_n_epoch(n))
    }

    /// Set participating slot to next.
    pub fn set_participating(&mut self) -> Result<()> {
        self.participating = Some(self.current_slot() + 1);
        Ok(())
    }

    /// Generate current slot checkpoint
    fn generate_slot_checkpoint(&mut self, sigma1: pallas::Base, sigma2: pallas::Base) {
        let slot = self.current_slot();
        let eta = self.get_eta();
        info!(target: "consensus::state", "generate_slot_checkpoint: slot: {:?}, eta: {:?}", slot, eta);
        let checkpoint = SlotCheckpoint { slot, eta, sigma1, sigma2 };
        self.slot_checkpoints.push(checkpoint);
    }

    // Initialize node lead coins and set current epoch and eta.
    pub async fn init_coins(&mut self) -> Result<()> {
        self.epoch = self.current_epoch();
        self.coins = self.create_coins().await?;
        self.update_forks_checkpoints();
        Ok(())
    }

    /// Check if new epoch has started and generate slot checkpoint.
    /// Returns flag to signify if epoch has changed.
    pub async fn epoch_changed(
        &mut self,
        sigma1: pallas::Base,
        sigma2: pallas::Base,
    ) -> Result<bool> {
        self.generate_slot_checkpoint(sigma1, sigma2);
        let epoch = self.current_epoch();
        if epoch <= self.epoch {
            return Ok(false)
        }
        self.epoch = epoch;

        Ok(true)
    }

    /// Return 2-term target approximation sigma coefficients.
    pub fn sigmas(&mut self) -> (pallas::Base, pallas::Base) {
        let f = self.win_inv_prob_with_full_stake();
        let total_stake = self.total_stake();
        let total_sigma = Float10::try_from(total_stake).unwrap();
        self.calc_sigmas(f, total_sigma)
    }

    fn calc_sigmas(&self, f: Float10, total_sigma: Float10) -> (pallas::Base, pallas::Base) {
        info!(target: "consensus::state", "sigmas(): f: {}", f);
        info!(target: "consensus::state", "sigmas(): total network stake: {:}", total_sigma);

        let one = constants::FLOAT10_ONE.clone();
        let neg_one = constants::FLOAT10_NEG_ONE.clone();
        let two = constants::FLOAT10_TWO.clone();

        let field_p = Float10::try_from(constants::P).unwrap();

        let x = one - f;
        let c = x.ln();
        let neg_c = neg_one * c;

        let sigma1_fbig = neg_c.clone() /
            (total_sigma.clone() + constants::FLOAT10_EPSILON.clone()) *
            field_p.clone();
        info!(target: "consensus::state", "sigma1_fbig: {:}", sigma1_fbig);
        let sigma1 = fbig2base(sigma1_fbig);

        let sigma2_fbig = (neg_c / (total_sigma + constants::FLOAT10_EPSILON.clone()))
            .powf(two.clone()) *
            (field_p / two);
        info!(target: "consensus::state", "sigma2_fbig: {:}", sigma2_fbig);
        let sigma2 = fbig2base(sigma2_fbig);

        (sigma1, sigma2)
    }

    /// Generate coins for provided sigmas.
    /// NOTE: The strategy here is having a single competing coin per slot.
    // TODO: DRK coin need to be burned, and consensus coin to be minted.
    async fn create_coins(&mut self) -> Result<Vec<LeadCoin>> {
        // TODO: cleanup LeadCoinSecrets, no need to keep a vector
        let (seeds, epoch_secrets) = {
            let mut rng = thread_rng();
            let mut seeds: Vec<u64> = Vec::with_capacity(constants::EPOCH_LENGTH);
            for _ in 0..constants::EPOCH_LENGTH {
                seeds.push(rng.gen());
            }
            (seeds, LeadCoinSecrets::generate())
        };

        // LeadCoin matrix containing node competing coins.
        let mut coins: Vec<LeadCoin> = Vec::with_capacity(constants::EPOCH_LENGTH);

        // Retrieve coin from wallet
        // NOTE: In future this will be retrieved from the money contract.
        // Get a wallet connection
        let mut conn = self.wallet.conn.acquire().await?;

        // Execute the query and see if we find any rows
        let query_str = format!("SELECT * FROM {}", constants::CONSENSUS_COIN_TABLE);
        let mut query = sqlx::query(&query_str);
        let coin = match query.fetch_one(&mut conn).await {
            Ok(row) => {
                let bytes: Vec<u8> = row.try_get(constants::CONSENSUS_COIN_COL)?;
                deserialize(&bytes)?
            }
            Err(_) => {
                // If no records are found, we generate a new coin and save it to the database
                info!(target: "consensus::state", "create_coins(): No LeadCoin was found in DB, generating new one...");
                // Temporarily, we compete with fixed stake.
                // This stake should be based on how many nodes we want to run, and they all
                // must sum to initial distribution total coins.
                //let stake = self.initial_distribution;
                let c = LeadCoin::new(
                    0,
                    self.current_slot(),
                    epoch_secrets.secret_keys[0].inner(),
                    epoch_secrets.merkle_roots[0],
                    0,
                    epoch_secrets.merkle_paths[0].clone(),
                    pallas::Base::from(seeds[0]),
                    &mut self.coins_tree,
                );
                let query_str = format!(
                    "INSERT INTO {} ({}) VALUES (?1);",
                    constants::CONSENSUS_COIN_TABLE,
                    constants::CONSENSUS_COIN_COL
                );
                query = sqlx::query(&query_str);
                query = query.bind(serialize(&c));
                query.execute(&mut conn).await?;
                c
            }
        };
        info!(target: "consensus::state", "create_coins(): Will use LeadCoin with value: {}", coin.value);
        coins.push(coin);

        Ok(coins)
    }

    /// Leadership reward, assuming constant reward
    /// TODO (res) implement reward mechanism with accord to DRK,DARK token-economics
    fn reward(&self) -> u64 {
        constants::REWARD
    }

    /// Auxillary function to calculate total slot rewards.
    fn slot_rewards(&self) -> u64 {
        // Retrieve existing blocks excluding genesis
        let blocks = (self.blockchain.len() as u64) - 1;
        // Retrieve longest fork length, to include those proposals in the calculation
        let max_fork_length = self.longest_chain_length() as u64;
        // Calculate rewarded slots
        let rewarded_slots = blocks + max_fork_length;

        rewarded_slots * self.reward()
    }

    /// Network total stake, assuming constant reward.
    /// Only used for fine-tuning. At genesis epoch first slot, of absolute index 0,
    /// if no stake was distributed, the total stake would be 0.
    /// To avoid division by zero, we asume total stake at first division is GENESIS_TOTAL_STAKE(1).
    fn total_stake(&self) -> u64 {
        let total_stake = self.slot_rewards() + self.initial_distribution;
        if total_stake == 0 {
            return constants::GENESIS_TOTAL_STAKE
        }
        total_stake
    }

    fn f_err(&mut self) -> Float10 {
        info!(target: "consensus::state", "Previous leaders: {}", self.previous_leaders);
        // Write counter to file
        let mut count_str: String = self.previous_leaders.to_string();
        count_str.push_str(",");
        let f =
            File::options().append(true).create(true).open(constants::LEADER_HISTORY_LOG).unwrap();
        {
            let mut writer = BufWriter::new(f);
            writer.write(&count_str.into_bytes()).unwrap();
        }
        // Calculate feedback
        let feedback = Float10::try_from(self.previous_leaders as i64).unwrap();
        // Reset previous leaders counter
        self.previous_leaders = 0;
        let target = constants::FLOAT10_ONE.clone();
        target - feedback
    }

    fn discrete_pid(&mut self) -> Float10 {
        let k1 = constants::KP.clone() + constants::KI.clone() + constants::KD.clone();
        let k2 = constants::FLOAT10_NEG_ONE.clone() * constants::KP.clone() +
            constants::FLOAT10_NEG_TWO.clone() * constants::KD.clone();
        let k3 = constants::KD.clone();
        let f_len = self.f_history.len();
        let err = self.f_err();
        let err_len = self.err_history.len();
        let ret = self.f_history[f_len - 1].clone() +
            k1.clone() * err.clone() +
            k2.clone() * self.err_history[err_len - 1].clone() +
            k3.clone() * self.err_history[err_len - 2].clone();
        info!(target: "consensus::state", "pid::f-1: {:}", self.f_history[f_len - 1].clone());
        info!(target: "consensus::state", "pid::err: {:}", err);
        info!(target: "consensus::state", "pid::err-1: {}", self.err_history[err_len - 1].clone());
        info!(target: "consensus::state", "pid::err-2: {}", self.err_history[err_len - 2].clone());
        info!(target: "consensus::state", "pid::k1: {}", k1.clone());
        info!(target: "consensus::state", "pid::k2: {}", k2.clone());
        info!(target: "consensus::state", "pid::k3: {}", k3.clone());
        self.err_history.push(err.clone());
        ret
    }
    /// the probability inverse of winnig lottery having all the stake
    /// returns f
    fn win_inv_prob_with_full_stake(&mut self) -> Float10 {
        let mut f = self.discrete_pid();
        if f <= constants::FLOAT10_ZERO.clone() {
            f = constants::MIN_F.clone()
        } else if f >= constants::FLOAT10_ONE.clone() {
            f = constants::MAX_F.clone()
        }
        // log f history
        let file =
            File::options().append(true).create(true).open(constants::F_HISTORY_LOG).unwrap();
        {
            let mut f_history = format!("{:}", f);
            f_history.push_str(",");
            let mut writer = BufWriter::new(file);
            writer.write(&f_history.into_bytes()).unwrap();
        }
        self.f_history.push(f.clone());
        f
    }

    /// Check that the participant/stakeholder coins win the slot lottery.
    /// If the stakeholder has multiple competing winning coins, only the highest value
    /// coin is selected, since the stakeholder can't give more than one proof per block/slot.
    /// * 'sigma1', 'sigma2': slot sigmas
    /// Returns: (check: bool, idx: usize) where idx is the winning coin's index
    pub fn is_slot_leader(
        &mut self,
        sigma1: pallas::Base,
        sigma2: pallas::Base,
    ) -> (bool, i64, usize) {
        // Check if node can produce proposals
        if !self.proposing {
            return (false, 0, 0)
        }

        let fork_index = self.longest_chain_index();
        let competing_coins = if fork_index == -1 {
            self.coins.clone()
        } else {
            self.forks[fork_index as usize].sequence.last().unwrap().coins.clone()
        };

        // If on single-node mode, node always proposes by extending the
        // single fork it holds.
        if self.single_node {
            return (true, fork_index, 0)
        }

        let mut won = false;
        let mut highest_stake = 0;
        let mut highest_stake_idx = 0;
        let total_stake = self.total_stake();
        for (winning_idx, coin) in competing_coins.iter().enumerate() {
            info!(target: "consensus::state", "is_slot_leader: coin stake: {:?}", coin.value);
            info!(target: "consensus::state", "is_slot_leader: total stake: {}", total_stake);
            info!(target: "consensus::state", "is_slot_leader: relative stake: {}", (coin.value as f64) / total_stake as f64);

            let first_winning = coin.is_leader(
                sigma1,
                sigma2,
                self.get_eta(),
                pallas::Base::from(self.current_slot()),
            );

            if first_winning && !won {
                highest_stake_idx = winning_idx;
            }

            won |= first_winning;
            if won && coin.value > highest_stake {
                highest_stake = coin.value;
                highest_stake_idx = winning_idx;
            }
        }

        (won, fork_index, highest_stake_idx)
    }

    /// Finds the longest forkchain the node holds and
    /// returns its index.
    pub fn longest_chain_index(&self) -> i64 {
        let mut length = 0;
        let mut index = -1;

        if !self.forks.is_empty() {
            for (i, chain) in self.forks.iter().enumerate() {
                if chain.sequence.len() > length {
                    length = chain.sequence.len();
                    index = i as i64;
                }
            }
        }

        index
    }

    /// Finds the length of longest fork chain the node holds.
    pub fn longest_chain_length(&self) -> usize {
        let mut max = 0;
        for fork in &self.forks {
            if fork.sequence.len() > max {
                max = fork.sequence.len();
            }
        }

        max
    }

    /// Given a proposal, find the index of the fork chain it extends.
    pub fn find_extended_chain_index(&mut self, proposal: &BlockProposal) -> Result<i64> {
        // We iterate through all forks to find which fork to extend
        let mut chain_index = -1;
        let mut state_checkpoint_index = 0;
        for (c_index, chain) in self.forks.iter().enumerate() {
            // Traverse sequence in reverse
            for (sc_index, state_checkpoint) in chain.sequence.iter().enumerate().rev() {
                if proposal.block.header.previous == state_checkpoint.proposal.hash {
                    chain_index = c_index as i64;
                    state_checkpoint_index = sc_index;
                    break
                }
            }
            if chain_index != -1 {
                break
            }
        }

        // If no fork was found, we check with canonical
        if chain_index == -1 {
            let (last_slot, last_block) = self.blockchain.last()?;
            if proposal.block.header.previous != last_block ||
                proposal.block.header.slot <= last_slot
            {
                info!(target: "consensus::state", "find_extended_chain_index(): Proposal doesn't extend any known chain");
                return Ok(-2)
            }

            // Proposal extends canonical chain
            return Ok(-1)
        }

        // Found fork chain
        let chain = &self.forks[chain_index as usize];
        // Proposal extends fork at last proposal
        if state_checkpoint_index == (chain.sequence.len() - 1) {
            return Ok(chain_index)
        }

        info!(target: "consensus::state", "find_extended_chain_index(): Proposal to fork a forkchain was received.");
        let mut chain = self.forks[chain_index as usize].clone();
        // We keep all proposals until the one it extends
        chain.sequence.drain((state_checkpoint_index + 1)..);
        self.forks.push(chain);
        Ok(self.forks.len() as i64 - 1)
    }

    /// Search the chains we're holding for the given proposal.
    pub fn proposal_exists(&self, input_proposal: &blake3::Hash) -> bool {
        for chain in self.forks.iter() {
            for state_checkpoint in chain.sequence.iter().rev() {
                if input_proposal == &state_checkpoint.proposal.hash {
                    return true
                }
            }
        }

        false
    }

    /// Utility function to extract leader selection lottery randomness(eta),
    /// defined as the hash of the last block, converted to pallas base.
    pub fn get_eta(&self) -> pallas::Base {
        let (_, hash) = self.blockchain.last().unwrap();
        let mut bytes: [u8; 32] = *hash.as_bytes();
        // Read first 254 bits
        bytes[30] = 0;
        bytes[31] = 0;
        pallas::Base::from_repr(bytes).unwrap()
    }

    /// Auxillary function to retrieve slot checkpoint of provided slot UID.
    pub fn get_slot_checkpoint(&self, slot: u64) -> Result<SlotCheckpoint> {
        // Check hot/live slot checkpoints
        for slot_checkpoint in self.slot_checkpoints.iter().rev() {
            if slot_checkpoint.slot == slot {
                return Ok(slot_checkpoint.clone())
            }
        }
        // Check if slot is finalized
        if let Ok(slot_checkpoints) = self.blockchain.get_slot_checkpoints_by_slot(&[slot]) {
            if !slot_checkpoints.is_empty() {
                if let Some(slot_checkpoint) = &slot_checkpoints[0] {
                    return Ok(slot_checkpoint.clone())
                }
            }
        }
        Err(Error::SlotCheckpointNotFound(slot))
    }

    /// Auxillary function to check if node has seen current or previous slot checkpoints.
    /// This check ensures that either the slots exist in memory or node has seen the finalization of these slots.
    pub fn slot_checkpoints_is_empty(&self) -> bool {
        let current_slot = self.current_slot();
        if self.get_slot_checkpoint(current_slot).is_ok() {
            return false
        }
        let previous_slot = current_slot - 1;
        !self.get_slot_checkpoint(previous_slot).is_ok()
    }

    /// Auxillary function to update all fork state checkpoints to nodes coins current canonical states.
    /// Note: This function should only be invoked once on nodes' coins creation.
    pub fn update_forks_checkpoints(&mut self) {
        for fork in &mut self.forks {
            for state_checkpoint in &mut fork.sequence {
                state_checkpoint.coins = self.coins.clone();
                state_checkpoint.coins_tree = self.coins_tree.clone();
            }
        }
    }

    /// Auxiliary structure to reset consensus state for a resync
    pub fn reset(&mut self) {
        self.participating = None;
        self.proposing = false;
        self.forks = vec![];
        self.slot_checkpoints = vec![];
        self.previous_leaders = 0;
        self.f_history = vec![constants::FLOAT10_ZERO.clone()];
        self.err_history = vec![constants::FLOAT10_ZERO.clone(), constants::FLOAT10_ZERO.clone()];
        self.nullifiers = vec![];
    }
}

/// Auxiliary structure used for consensus syncing.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ConsensusRequest {}

impl net::Message for ConsensusRequest {
    fn name() -> &'static str {
        "consensusrequest"
    }
}

/// Auxiliary structure used for consensus syncing.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ConsensusResponse {
    /// Slot the network was bootstrapped
    pub bootstrap_slot: u64,
    /// Current slot
    pub current_slot: u64,
    /// Hot/live data used by the consensus algorithm
    pub forks: Vec<ForkInfo>,
    /// Pending transactions
    pub unconfirmed_txs: Vec<Transaction>,
    /// Hot/live slot checkpoints
    pub slot_checkpoints: Vec<SlotCheckpoint>,
    // TODO: When Float10 supports encoding/decoding this should be
    // replaced by directly using Vec<Float10>
    /// Controller output history
    pub f_history: Vec<String>,
    /// Controller proportional error history
    pub err_history: Vec<String>,
    /// Seen nullifiers from proposals
    pub nullifiers: Vec<pallas::Base>,
}

impl net::Message for ConsensusResponse {
    fn name() -> &'static str {
        "consensusresponse"
    }
}

/// Auxiliary structure used for consensus syncing.
#[derive(Debug, SerialEncodable, SerialDecodable)]
pub struct ConsensusSlotCheckpointsRequest {}

impl net::Message for ConsensusSlotCheckpointsRequest {
    fn name() -> &'static str {
        "consensusslotcheckpointsrequest"
    }
}

/// Auxiliary structure used for consensus syncing.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ConsensusSlotCheckpointsResponse {
    /// Node known bootstrap slot
    pub bootstrap_slot: u64,
    /// Node is able to propose proposals
    pub proposing: bool,
    /// Node has hot/live slot checkpoints
    pub is_empty: bool,
}

impl net::Message for ConsensusSlotCheckpointsResponse {
    fn name() -> &'static str {
        "consensusslotcheckpointsresponse"
    }
}

/// Auxiliary structure used to keep track of slot validation parameters.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SlotCheckpoint {
    /// Slot UID
    pub slot: u64,
    /// Slot eta
    pub eta: pallas::Base,
    /// Slot sigma1
    pub sigma1: pallas::Base,
    /// Slot sigma2
    pub sigma2: pallas::Base,
}

impl SlotCheckpoint {
    pub fn new(slot: u64, eta: pallas::Base, sigma1: pallas::Base, sigma2: pallas::Base) -> Self {
        Self { slot, eta, sigma1, sigma2 }
    }

    /// Generate the genesis slot checkpoint.
    pub fn genesis_slot_checkpoint() -> Self {
        let eta = pallas::Base::zero();
        let sigma1 = pallas::Base::zero();
        let sigma2 = pallas::Base::zero();

        Self::new(0, eta, sigma1, sigma2)
    }
}

impl net::Message for SlotCheckpoint {
    fn name() -> &'static str {
        "slotcheckpoint"
    }
}

/// Auxiliary structure used for slot checkpoints syncing
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SlotCheckpointRequest {
    /// Slot UID
    pub slot: u64,
}

impl net::Message for SlotCheckpointRequest {
    fn name() -> &'static str {
        "slotcheckpointrequest"
    }
}

/// Auxiliary structure used for slot checkpoints syncing
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SlotCheckpointResponse {
    /// Response blocks.
    pub slot_checkpoints: Vec<SlotCheckpoint>,
}

impl net::Message for SlotCheckpointResponse {
    fn name() -> &'static str {
        "slotcheckpointresponse"
    }
}

/// Auxiliary structure used to keep track of consensus state checkpoints.
#[derive(Debug, Clone)]
pub struct StateCheckpoint {
    /// Block proposal
    pub proposal: BlockProposal,
    /// Node competing coins current state
    pub coins: Vec<LeadCoin>,
    /// Coin commitments tree current state
    pub coins_tree: BridgeTree<MerkleNode, MERKLE_DEPTH>,
    /// Seen nullifiers from proposals current state
    pub nullifiers: Vec<pallas::Base>,
}

impl StateCheckpoint {
    pub fn new(
        proposal: BlockProposal,
        coins: Vec<LeadCoin>,
        coins_tree: BridgeTree<MerkleNode, MERKLE_DEPTH>,
        nullifiers: Vec<pallas::Base>,
    ) -> Self {
        Self { proposal, coins, coins_tree, nullifiers }
    }
}

/// Auxiliary structure used for forked consensus state checkpoints syncing
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct StateCheckpointInfo {
    /// Block proposal
    pub proposal: BlockProposal,
    /// Seen nullifiers from proposals current state
    pub nullifiers: Vec<pallas::Base>,
}

impl From<StateCheckpoint> for StateCheckpointInfo {
    fn from(state_checkpoint: StateCheckpoint) -> Self {
        Self { proposal: state_checkpoint.proposal, nullifiers: state_checkpoint.nullifiers }
    }
}

impl From<StateCheckpointInfo> for StateCheckpoint {
    fn from(state_checkpoint_info: StateCheckpointInfo) -> Self {
        Self {
            proposal: state_checkpoint_info.proposal,
            coins: vec![],
            coins_tree: BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(constants::EPOCH_LENGTH * 100),
            nullifiers: state_checkpoint_info.nullifiers,
        }
    }
}

/// This struct represents a sequence of consensus state checkpoints.
#[derive(Debug, Clone)]
pub struct Fork {
    pub genesis_block: blake3::Hash,
    pub sequence: Vec<StateCheckpoint>,
}

impl Fork {
    pub fn new(genesis_block: blake3::Hash, initial_state_checkpoint: StateCheckpoint) -> Self {
        Self { genesis_block, sequence: vec![initial_state_checkpoint] }
    }

    /// Insertion of a valid state checkpoint.
    pub fn add(&mut self, state_checkpoint: &StateCheckpoint) {
        if self.check_state_checkpoint(state_checkpoint, self.sequence.last().unwrap()) {
            self.sequence.push(state_checkpoint.clone());
        }
    }

    /// A fork chain is considered valid when every state checkpoint is valid,
    /// based on the `check_state_checkpoint` function
    pub fn check_chain(&self) -> bool {
        for (index, state_checkpoint) in self.sequence[1..].iter().enumerate() {
            if !self.check_state_checkpoint(state_checkpoint, &self.sequence[index]) {
                return false
            }
        }

        true
    }

    /// A state checkpoint is considered valid when its proposal parent hash is equal to the
    /// hash of the previous checkpoint's proposal and their slots are incremental,
    /// excluding the genesis block proposal.
    pub fn check_state_checkpoint(
        &self,
        state_checkpoint: &StateCheckpoint,
        previous: &StateCheckpoint,
    ) -> bool {
        if state_checkpoint.proposal.block.header.previous == self.genesis_block {
            info!(target: "consensus::state", "check_checkpoint(): Genesis block proposal provided.");
            return false
        }

        if state_checkpoint.proposal.block.header.previous != previous.proposal.hash ||
            state_checkpoint.proposal.block.header.slot <= previous.proposal.block.header.slot
        {
            info!(target: "consensus::state", "check_checkpoint(): Provided state checkpoint proposal is invalid.");
            return false
        }

        // TODO: validate rest checkpoint info(like nullifiers)

        true
    }
}

/// Auxiliary structure used for forks syncing
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ForkInfo {
    pub genesis_block: blake3::Hash,
    pub sequence: Vec<StateCheckpointInfo>,
}

impl From<Fork> for ForkInfo {
    fn from(fork: Fork) -> Self {
        let mut sequence = vec![];
        for state_checkpoint in fork.sequence {
            sequence.push(state_checkpoint.into());
        }
        Self { genesis_block: fork.genesis_block, sequence }
    }
}

impl From<ForkInfo> for Fork {
    fn from(fork_info: ForkInfo) -> Self {
        let mut sequence = vec![];
        for checkpoint in fork_info.sequence {
            sequence.push(checkpoint.into());
        }
        Self { genesis_block: fork_info.genesis_block, sequence }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        consensus::{
            state::{Blockchain, ConsensusState},
            utils::fbig2base,
            Float10, TESTNET_BOOTSTRAP_TIMESTAMP, TESTNET_GENESIS_HASH_BYTES,
            TESTNET_GENESIS_TIMESTAMP, TESTNET_INITIAL_DISTRIBUTION,
        },
        wallet::WalletDb,
        Result,
    };

    #[async_std::test]
    async fn calc_sigmas_test() -> Result<()> {
        // Generate dummy state
        let wallet = WalletDb::new("sqlite::memory:", "foo").await?;
        let sled_db = sled::Config::new().temporary(true).open()?;
        let blockchain =
            Blockchain::new(&sled_db, *TESTNET_GENESIS_TIMESTAMP, *TESTNET_GENESIS_HASH_BYTES)?;
        let state = ConsensusState::new(
            wallet,
            blockchain,
            *TESTNET_BOOTSTRAP_TIMESTAMP,
            *TESTNET_GENESIS_TIMESTAMP,
            *TESTNET_GENESIS_HASH_BYTES,
            *TESTNET_INITIAL_DISTRIBUTION,
            true,
        )?;

        let precision_diff = Float10::try_from(
            "10000000000000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap();
        let precision_diff_base = fbig2base(precision_diff);
        let f = Float10::try_from("0.01").unwrap();
        let total_stake = Float10::try_from("100").unwrap();
        let (sigma1, sigma2) = state.calc_sigmas(f, total_stake);
        let sigma1_rhs = Float10::try_from(
            "2909373465034095801035568917399197865646520818579502832252119592405565440",
        )
        .unwrap();
        let sigma1_rhs_base = fbig2base(sigma1_rhs);
        let sigma2_rhs = Float10::try_from(
            "9137556389643100714432609642916129738741963230846798778430644027392",
        )
        .unwrap();
        let sigma2_rhs_base = fbig2base(sigma2_rhs);
        let sigma1_delta = if sigma1_rhs_base > sigma1 {
            sigma1_rhs_base - sigma1
        } else {
            sigma1 - sigma1_rhs_base
        };
        let sigma2_delta = if sigma2_rhs_base > sigma2 {
            sigma2_rhs_base - sigma2
        } else {
            sigma2 - sigma2_rhs_base
        };
        //note! test cases were generated by low precision python scripts.
        //https://github.com/ertosns/lotterysim/blob/master/pallas_unittests.csv
        assert!(sigma1_delta < precision_diff_base);
        assert!(sigma2_delta < precision_diff_base);

        Ok(())
    }
}
