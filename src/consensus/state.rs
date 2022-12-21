/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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
use darkfi_serial::{SerialDecodable, SerialEncodable};
use log::info;
use rand::{thread_rng, Rng};

use super::{
    constants,
    leadcoin::{LeadCoin, LeadCoinSecrets},
    utils::fbig2base,
    Block, BlockProposal, Float10,
};

use crate::{blockchain::Blockchain, net, tx::Transaction, util::time::Timestamp, Error, Result};
use dashu::base::Abs;

/// This struct represents the information required by the consensus algorithm
pub struct ConsensusState {
    /// Canonical (finalized) blockchain
    pub blockchain: Blockchain,
    /// Network bootstrap timestamp
    pub bootstrap_ts: Timestamp,
    /// Genesis block creation timestamp
    pub genesis_ts: Timestamp,
    /// Genesis block hash
    pub genesis_block: blake3::Hash,
    /// Slot the network was bootstrapped
    pub bootstrap_slot: u64,
    /// Participating start slot
    pub participating: Option<u64>,
    /// Node is able to propose proposals
    pub proposing: bool,
    /// Last slot node check for finalization
    pub checked_finalization: u64,
    /// Slots offset since genesis,
    pub offset: Option<u64>,
    /// Fork chains containing block proposals
    pub forks: Vec<Fork>,
    /// Current epoch
    pub epoch: u64,
    /// Current epoch eta
    pub epoch_eta: pallas::Base,
    /// Hot/live slot checkpoints
    pub slot_checkpoints: Vec<SlotCheckpoint>,
    /// Leaders count history
    pub leaders_history: Vec<u64>,
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
        blockchain: Blockchain,
        bootstrap_ts: Timestamp,
        genesis_ts: Timestamp,
        genesis_data: blake3::Hash,
    ) -> Result<Self> {
        let genesis_block = Block::genesis_block(genesis_ts, genesis_data).blockhash();
        Ok(Self {
            blockchain,
            bootstrap_ts,
            genesis_ts,
            genesis_block,
            bootstrap_slot: 0,
            participating: None,
            proposing: false,
            checked_finalization: 0,
            offset: None,
            forks: vec![],
            epoch: 0,
            epoch_eta: pallas::Base::zero(),
            slot_checkpoints: vec![],
            leaders_history: vec![0],
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
        let checkpoint = SlotCheckpoint { slot, eta: self.epoch_eta, sigma1, sigma2 };
        self.slot_checkpoints.push(checkpoint);
    }

    // Initialize node lead coins and set current epoch and eta.
    pub async fn init_coins(&mut self) -> Result<()> {
        self.epoch = self.current_epoch();
        if self.slot_checkpoints.is_empty() {
            // Create slot checkpoint if not on genesis slot (already in db)
            if self.current_slot() != 0 {
                self.epoch_eta = self.get_eta();
                let (sigma1, sigma2) = self.sigmas();
                self.generate_slot_checkpoint(sigma1, sigma2);
            }
        } else {
            let last_slot_checkpoint = self.slot_checkpoints.last().unwrap();
            self.epoch_eta = last_slot_checkpoint.eta;
        };
        self.coins = self.create_coins(self.epoch_eta).await?;
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
        self.epoch_eta = self.get_eta();

        Ok(true)
    }

    /// return 2-term target approximation sigma coefficients.
    pub fn sigmas(&mut self) -> (pallas::Base, pallas::Base) {
        let f = self.win_inv_prob_with_full_stake();

        // Generate sigmas
        let mut total_stake = self.total_stake(); // Only used for fine-tuning
                                                  // at genesis epoch first slot, of absolute index 0,
                                                  // the total stake would be 0, to avoid division by zero,
                                                  // we asume total stake at first division is GENESIS_TOTAL_STAKE.
        if total_stake == 0 {
            total_stake = constants::GENESIS_TOTAL_STAKE;
        }
        info!("sigmas(): f: {}", f);
        info!("sigmas(): stake: {}", total_stake);
        let one = constants::FLOAT10_ONE.clone();
        let two = constants::FLOAT10_TWO.clone();
        let field_p = Float10::from_str_native(constants::P)
            .unwrap()
            .with_precision(constants::RADIX_BITS)
            .value();
        let total_sigma =
            Float10::try_from(total_stake).unwrap().with_precision(constants::RADIX_BITS).value();

        let x = one - f;
        let c = x.ln();

        let sigma1_fbig = c.clone() / total_sigma.clone() * field_p.clone();
        let sigma1 = fbig2base(sigma1_fbig);

        let sigma2_fbig = (c / total_sigma).powf(two.clone()) * (field_p / two);
        let sigma2 = fbig2base(sigma2_fbig);
        (sigma1, sigma2)
    }

    /// Generate coins for provided sigmas.
    /// NOTE: The strategy here is having a single competing coin per slot.
    // TODO: DRK coin need to be burned, and consensus coin to be minted.
    async fn create_coins(&mut self, eta: pallas::Base) -> Result<Vec<LeadCoin>> {
        let slot = self.current_slot();

        // TODO: cleanup LeadCoinSecrets, no need to keep a vector
        let mut rng = thread_rng();
        let mut seeds: Vec<u64> = Vec::with_capacity(constants::EPOCH_LENGTH);
        for _ in 0..constants::EPOCH_LENGTH {
            seeds.push(rng.gen());
        }
        let epoch_secrets = LeadCoinSecrets::generate();

        // LeadCoin matrix containing node competing coins.
        let mut coins: Vec<LeadCoin> = Vec::with_capacity(constants::EPOCH_LENGTH);

        // TODO: TESTNET: Here we would look into the wallet to find coins we're able to use.
        //                The wallet has specific tables for consensus coins.
        // TODO: TESTNET: Token ID still has to be enforced properly in the consensus.

        // Temporarily, we compete with zero stake
        let coin = LeadCoin::new(
            eta,
            rand::thread_rng().gen_range(0..1000),
            slot,
            epoch_secrets.secret_keys[0].inner(),
            epoch_secrets.merkle_roots[0],
            0,
            epoch_secrets.merkle_paths[0],
            seeds[0],
            &mut self.coins_tree,
        );
        coins.push(coin);

        Ok(coins)
    }

    /// leadership reward, assuming constant reward
    /// TODO (res) implement reward mechanism with accord to DRK,DARK token-economics
    fn reward() -> u64 {
        constants::REWARD
    }

    /// Auxillary function to receive current slot offset.
    /// If offset is None, its setted up as last block slot offset.
    pub fn get_current_offset(&mut self, current_slot: u64) -> u64 {
        // This is the case were we restarted our node, didn't receive offset from other nodes,
        // so we need to find offset from last block, exluding network dead period.
        if self.offset.is_none() {
            let (last_slot, last_offset) = self.blockchain.get_last_offset().unwrap();
            let offset = last_offset + (current_slot - last_slot);
            info!("get_current_offset(): Setting slot offset: {}", offset);
            self.offset = Some(offset);
        }

        self.offset.unwrap()
    }

    /// Auxillary function to calculate overall empty slots.
    /// We keep an offset from genesis indicating when the first slot actually started.
    /// This offset is shared between nodes.
    fn overall_empty_slots(&mut self, current_slot: u64) -> u64 {
        // Retrieve existing blocks excluding genesis
        let blocks = (self.blockchain.len() as u64) - 1;
        // Setup offset if only have genesis and havent received offset from other nodes
        if blocks == 0 && self.offset.is_none() {
            info!(
                "overall_empty_slots(): Blockchain contains only genesis, setting slot offset: {}",
                current_slot
            );
            self.offset = Some(current_slot);
        }
        // Retrieve longest fork length, to also those proposals in the calculation
        let max_fork_length = self.longest_chain_length() as u64;
        current_slot - blocks - self.get_current_offset(current_slot) - max_fork_length
    }

    /// total stake
    /// assuming constant Reward.
    fn total_stake(&mut self) -> i64 {
        let current_slot = self.current_slot();
        ((current_slot - self.overall_empty_slots(current_slot)) * Self::reward()) as i64
    }

    /// Calculate how many leaders existed in previous slot and appends
    /// it to history, to report it if win. On finalization sync period,
    /// node replaces its leaders history with the sequence extracted by
    /// the longest fork.
    fn extend_leaders_history(&mut self) -> Float10 {
        let slot = self.current_slot();
        let previous_slot = slot - 1;
        let mut count = 0;
        for chain in &self.forks {
            // Previous slot proposals exist at end of each fork
            if chain.sequence.last().unwrap().proposal.block.header.slot == previous_slot {
                count += 1;
            }
        }
        self.leaders_history.push(count);
        info!("extend_leaders_history(): Current leaders history: {:?}", self.leaders_history);
        Float10::try_from(count as i64).unwrap().with_precision(constants::RADIX_BITS).value()
    }

    fn pid_error(feedback: Float10) -> Float10 {
        let target = constants::FLOAT10_ONE.clone();
        target - feedback
    }

    fn f_dif(&mut self) -> Float10 {
        Self::pid_error(self.extend_leaders_history())
    }

    fn max_windowed_forks(&self) -> Float10 {
        let mut max: u64 = 5;
        let window_size = 10;
        let len = self.leaders_history.len();
        let window_begining = if len <= (window_size + 1) { 0 } else { len - (window_size + 1) };
        for item in &self.leaders_history[window_begining..] {
            if *item > max {
                max = *item;
            }
        }

        Float10::try_from(max as i64).unwrap().with_precision(constants::RADIX_BITS).value()
    }

    fn tuned_kp(&self) -> Float10 {
        (constants::KP.clone() * constants::FLOAT10_FIVE.clone()) / self.max_windowed_forks()
    }

    fn weighted_f_dif(&mut self) -> Float10 {
        self.tuned_kp() * self.f_dif()
    }

    fn f_der(&self) -> Float10 {
        let len = self.leaders_history.len();
        let last = Float10::try_from(self.leaders_history[len - 1] as i64)
            .unwrap()
            .with_precision(constants::RADIX_BITS)
            .value();
        let second_to_last = Float10::try_from(self.leaders_history[len - 2] as i64)
            .unwrap()
            .with_precision(constants::RADIX_BITS)
            .value();
        let mut der =
            (Self::pid_error(second_to_last) - Self::pid_error(last)) / constants::DT.clone();
        der = if der > constants::MAX_DER.clone() { constants::MAX_DER.clone() } else { der };
        der = if der < constants::MIN_DER.clone() { constants::MIN_DER.clone() } else { der };
        der
    }

    fn weighted_f_der(&self) -> Float10 {
        constants::KD.clone() * self.f_der()
    }

    fn f_int(&self) -> Float10 {
        let mut sum = constants::FLOAT10_ZERO.clone();
        let lead_history_len = self.leaders_history.len();
        let history_begin_index = if lead_history_len > 10 { lead_history_len - 10 } else { 0 };

        for lf in &self.leaders_history[history_begin_index..] {
            sum += Float10::try_from(*lf).unwrap().abs();
        }
        sum
    }

    fn tuned_ki(&self) -> Float10 {
        (constants::KI.clone() * constants::FLOAT10_FIVE.clone()) / self.max_windowed_forks()
    }

    fn weighted_f_int(&self) -> Float10 {
        constants::KI.clone() * self.f_int()
    }

    fn zero_leads_len(&self) -> Float10 {
        let mut count = constants::FLOAT10_ZERO.clone();
        let hist_len = self.leaders_history.len();
        for i in 1..hist_len {
            if self.leaders_history[hist_len - i] == 0 {
                count += constants::FLOAT10_ONE.clone();
            } else {
                break
            }
        }
        count
    }

    /// the probability inverse of winnig lottery having all the stake
    /// returns f
    fn win_inv_prob_with_full_stake(&mut self) -> Float10 {
        let p = self.weighted_f_dif();
        let i = self.weighted_f_int();
        let d = self.weighted_f_der();
        info!("win_inv_prob_with_full_stake(): PID P: {:?}", p);
        info!("win_inv_prob_with_full_stake(): PID I: {:?}", i);
        info!("win_inv_prob_with_full_stake(): PID D: {:?}", d);
        let f = p + i.clone() + d;
        info!("win_inv_prob_with_full_stake(): PID f: {}", f);
        if f == constants::FLOAT10_ZERO.clone() {
            return constants::MIN_F.clone()
        } else if f >= constants::FLOAT10_ONE.clone() {
            return constants::MAX_F.clone()
        }
        let hist_len = self.leaders_history.len();
        if hist_len > 3 &&
            self.leaders_history[hist_len - 1] == 0 &&
            self.leaders_history[hist_len - 2] == 0 &&
            self.leaders_history[hist_len - 3] == 0 &&
            i == constants::FLOAT10_ZERO.clone()
        {
            return f * constants::DEG_RATE.clone().powf(self.zero_leads_len())
        }
        f
    }

    /// Check that the participant/stakeholder coins win the slot lottery.
    /// If the stakeholder has multiple competing winning coins, only the highest value
    /// coin is selected, since the stakeholder can't give more than one proof per block/slot.
    /// * 'sigma1', 'sigma2': slot sigmas
    /// Returns: (check: bool, idx: usize) where idx is the winning coin's index
    pub fn is_slot_leader(&mut self, sigma1: pallas::Base, sigma2: pallas::Base) -> (bool, usize) {
        // Check if node can produce proposals
        if !self.proposing {
            return (false, 0)
        }
        let competing_coins = &self.coins.clone();

        let mut won = false;
        let mut highest_stake = 0;
        let mut highest_stake_idx = 0;
        let total_stake = self.total_stake();
        for (winning_idx, coin) in competing_coins.iter().enumerate() {
            info!("is_slot_leader: coin stake: {:?}", coin.value);
            info!("is_slot_leader: total stake: {}", total_stake);
            info!("is_slot_leader: relative stake: {}", (coin.value as f64) / total_stake as f64);
            let first_winning = coin.is_leader(sigma1, sigma2);
            if first_winning && !won {
                highest_stake_idx = winning_idx;
            }

            won |= first_winning;
            if won && coin.value > highest_stake {
                highest_stake = coin.value;
                highest_stake_idx = winning_idx;
            }
        }

        (won, highest_stake_idx)
    }

    /// Finds the longest blockchain the node holds and
    /// returns the last block hash and the chain index.
    pub fn longest_chain_last_hash(&self) -> Result<(blake3::Hash, i64)> {
        let mut longest: Option<Fork> = None;
        let mut length = 0;
        let mut index = -1;

        if !self.forks.is_empty() {
            for (i, chain) in self.forks.iter().enumerate() {
                if chain.sequence.len() > length {
                    longest = Some(chain.clone());
                    length = chain.sequence.len();
                    index = i as i64;
                }
            }
        }

        let hash = match longest {
            Some(chain) => chain.sequence.last().unwrap().proposal.hash,
            None => self.blockchain.last()?.1,
        };

        Ok((hash, index))
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
                info!("find_extended_chain_index(): Proposal doesn't extend any known chain");
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

        info!("find_extended_chain_index(): Proposal to fork a forkchain was received.");
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

    /// Auxillary function to set nodes leaders count history to the largest fork sequence
    /// of leaders, by using provided index.
    pub fn set_leader_history(&mut self, index: i64, current_slot: u64) {
        // Check if we found longest fork to extract sequence from
        match index {
            -1 => {
                info!("set_leader_history(): No fork exists.");
            }
            _ => {
                info!("set_leader_history(): Checking last proposal of fork: {}", index);
                let last_proposal = &self.forks[index as usize].sequence.last().unwrap().proposal;
                if last_proposal.block.header.slot == current_slot {
                    // Replacing our last history element with the leaders one
                    self.leaders_history.pop();
                    self.leaders_history.push(last_proposal.block.lead_info.leaders);
                    info!("set_leader_history(): New leaders history: {:?}", self.leaders_history);
                    return
                }
            }
        }
        self.leaders_history.push(0);
    }

    /// Utility function to extract leader selection lottery randomness(eta),
    /// defined as the hash of the previous lead proof converted to pallas base.
    fn get_eta(&self) -> pallas::Base {
        let proof_tx_hash = self.blockchain.get_last_proof_hash().unwrap();
        let mut bytes: [u8; 32] = *proof_tx_hash.as_bytes();
        // read first 254 bits
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
        self.offset = None;
        self.forks = vec![];
        self.slot_checkpoints = vec![];
        self.leaders_history = vec![0];
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
    /// Slots offset since genesis,
    pub offset: Option<u64>,
    /// Hot/live data used by the consensus algorithm
    pub forks: Vec<ForkInfo>,
    /// Pending transactions
    pub unconfirmed_txs: Vec<Transaction>,
    /// Hot/live slot checkpoints
    pub slot_checkpoints: Vec<SlotCheckpoint>,
    /// Leaders count history
    pub leaders_history: Vec<u64>,
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
            info!("check_checkpoint(): Genesis block proposal provided.");
            return false
        }

        if state_checkpoint.proposal.block.header.previous != previous.proposal.hash ||
            state_checkpoint.proposal.block.header.slot <= previous.proposal.block.header.slot
        {
            info!("check_checkpoint(): Provided state checkpoint proposal is invalid.");
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
