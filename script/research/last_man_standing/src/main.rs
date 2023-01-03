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

use darkfi::{
    blockchain::Blockchain,
    consensus::{
        constants,
        leadcoin::{LeadCoin, LeadCoinSecrets},
        utils::fbig2base,
        Float10,
    },
    util::time::Timestamp,
    Result,
};
use darkfi_sdk::{
    crypto::{constants::MERKLE_DEPTH, MerkleNode},
    incrementalmerkletree::bridgetree::BridgeTree,
    pasta::{group::ff::PrimeField, pallas},
};
use dashu::base::Abs;
use rand::{thread_rng, Rng};

// Simulation configuration
const NODES: u64 = 10;
const SLOTS: u64 = 10;

// PID controller configuration/constants
#[derive(Clone)]
struct PID {
    pub dt: Float10,
    pub _ti: Float10,
    pub _td: Float10,
    pub kp: Float10,
    pub ki: Float10,
    pub kd: Float10,
    pub _pid_out_step: Float10,
    pub max_der: Float10,
    pub min_der: Float10,
    pub max_f: Float10,
    pub min_f: Float10,
    pub deg_rate: Float10,
}

impl PID {
    fn new() -> Self {
        Self {
            dt: Float10::from_str_native("0.1")
                .unwrap()
                .with_precision(constants::RADIX_BITS)
                .value(),
            _ti: constants::FLOAT10_ONE.clone(),
            _td: constants::FLOAT10_ONE.clone(),
            kp: Float10::from_str_native("0.1")
                .unwrap()
                .with_precision(constants::RADIX_BITS)
                .value(),
            ki: Float10::from_str_native("0.03")
                .unwrap()
                .with_precision(constants::RADIX_BITS)
                .value(),
            kd: constants::FLOAT10_ONE.clone(),
            _pid_out_step: Float10::from_str_native("0.1")
                .unwrap()
                .with_precision(constants::RADIX_BITS)
                .value(),
            max_der: Float10::from_str_native("0.1")
                .unwrap()
                .with_precision(constants::RADIX_BITS)
                .value(),
            min_der: Float10::from_str_native("-0.1")
                .unwrap()
                .with_precision(constants::RADIX_BITS)
                .value(),
            max_f: Float10::from_str_native("0.99")
                .unwrap()
                .with_precision(constants::RADIX_BITS)
                .value(),
            min_f: Float10::from_str_native("0.05")
                .unwrap()
                .with_precision(constants::RADIX_BITS)
                .value(),
            deg_rate: Float10::from_str_native("0.9")
                .unwrap()
                .with_precision(constants::RADIX_BITS)
                .value(),
        }
    }
}

/// Node consensus state
struct ConsensusState {
    /// Current slot
    pub current_slot: u64,
    /// Total sum of initial staking coins
    pub initial_distribution: u64,
    /// Competing coins
    pub coins: Vec<LeadCoin>,
    /// Coin commitments tree
    pub coins_tree: BridgeTree<MerkleNode, MERKLE_DEPTH>,
    /// Previous rounds leaders
    pub leaders_history: Vec<i64>,
    /// PID configuration
    pub pid: PID,
}

impl ConsensusState {
    fn pid_error(&self, feedback: Float10) -> Float10 {
        let target = constants::FLOAT10_ONE.clone();
        target - feedback
    }

    fn f_dif(&self) -> Float10 {
        let last_round_leaders = *self.leaders_history.last().unwrap();
        let previous_leaders = Float10::try_from(last_round_leaders)
            .unwrap()
            .with_precision(constants::RADIX_BITS)
            .value();
        self.pid_error(previous_leaders)
    }

    fn max_windowed_forks(&self) -> Float10 {
        let mut max: i64 = 5;
        let window_size = 10;
        let len = self.leaders_history.len();
        let window_begining = if len <= (window_size + 1) { 0 } else { len - (window_size + 1) };
        for item in &self.leaders_history[window_begining..] {
            if *item > max {
                max = *item;
            }
        }

        Float10::try_from(max).unwrap().with_precision(constants::RADIX_BITS).value()
    }

    fn tuned_kp(&self) -> Float10 {
        (self.pid.kp.clone() * constants::FLOAT10_FIVE.clone()) / self.max_windowed_forks()
    }

    fn weighted_f_dif(&self) -> Float10 {
        self.tuned_kp() * self.f_dif()
    }

    fn f_int(&self) -> Float10 {
        let mut sum = constants::FLOAT10_ZERO.clone();
        let lead_history_len = self.leaders_history.len();
        let history_begin_index = if lead_history_len > 10 { lead_history_len - 10 } else { 0 };

        for lf in &self.leaders_history[history_begin_index..] {
            sum += self.pid_error(Float10::try_from(lf.clone()).unwrap()).abs();
        }
        sum
    }

    fn tuned_ki(&self) -> Float10 {
        (self.pid.ki.clone() * constants::FLOAT10_FIVE.clone()) / self.max_windowed_forks()
    }

    fn weighted_f_int(&self) -> Float10 {
        self.tuned_ki() * self.f_int()
    }

    fn f_der(&self) -> Float10 {
        let len = self.leaders_history.len();
        let last = Float10::try_from(self.leaders_history[len - 1] as i64)
            .unwrap()
            .with_precision(constants::RADIX_BITS)
            .value();
        let mut der = if len > 1 {
            let second_to_last = Float10::try_from(self.leaders_history[len - 2] as i64)
                .unwrap()
                .with_precision(constants::RADIX_BITS)
                .value();
            (self.pid_error(second_to_last) - self.pid_error(last)) / self.pid.dt.clone()
        } else {
            self.pid_error(last) / self.pid.dt.clone()
        };

        der = if der > self.pid.max_der.clone() { self.pid.max_der.clone() } else { der };
        der = if der < self.pid.min_der.clone() { self.pid.min_der.clone() } else { der };
        der
    }

    fn weighted_f_der(&self) -> Float10 {
        self.pid.kd.clone() * self.f_der()
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

    /// Inverse probability of winning lottery having all the stake.    
    fn win_inv_prob_with_full_stake(&self) -> Float10 {
        let p = self.weighted_f_dif();
        let i = self.weighted_f_int();
        let d = self.weighted_f_der();
        //println!("win_inv_prob_with_full_stake(): PID P: {:?}", p);
        //println!("win_inv_prob_with_full_stake(): PID I: {:?}", i);
        //println!("win_inv_prob_with_full_stake(): PID D: {:?}", d);
        let f = p + i.clone() + d;
        //println!("win_inv_prob_with_full_stake(): PID f: {}", f);
        if f == constants::FLOAT10_ZERO.clone() {
            return self.pid.min_f.clone()
        } else if f >= constants::FLOAT10_ONE.clone() {
            return self.pid.max_f.clone()
        }
        let hist_len = self.leaders_history.len();
        if hist_len > 3 &&
            self.leaders_history[hist_len - 1] == 0 &&
            self.leaders_history[hist_len - 2] == 0 &&
            self.leaders_history[hist_len - 3] == 0 &&
            i == constants::FLOAT10_ZERO.clone()
        {
            return f * self.pid.deg_rate.clone().powf(self.zero_leads_len())
        }
        f
    }

    /// Leadership reward, assuming constant reward
    /// TODO (res) implement reward mechanism with accord to DRK,DARK token-economics
    fn reward(&self) -> u64 {
        constants::REWARD
    }

    /// Network total stake, assuming constant reward.
    /// Only used for fine-tuning. At genesis epoch first slot, of absolute index 0,
    /// if no stake was distributed, the total stake would be 0.
    /// To avoid division by zero, we asume total stake at first division is GENESIS_TOTAL_STAKE(1).
    fn total_stake(&self) -> u64 {
        let rewards = (self.current_slot - 1) * self.reward();
        let total_stake = rewards + self.initial_distribution;
        if total_stake == 0 {
            return constants::GENESIS_TOTAL_STAKE
        }
        total_stake
    }

    /// Return 2-term target approximation sigma coefficients.
    pub fn sigmas(&self) -> (pallas::Base, pallas::Base) {
        let f = self.win_inv_prob_with_full_stake();
        let total_stake = self.total_stake();
        //println!("sigmas(): f: {}", f);
        //println!("sigmas(): stake: {}", total_stake);
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

    /// Check that the participant/stakeholder coins win the slot lottery.
    /// If the stakeholder has multiple competing winning coins, only the highest value
    /// coin is selected, since the stakeholder can't give more than one proof per block/slot.
    /// * 'sigma1', 'sigma2': slot sigmas
    /// Returns: (check: bool, idx: usize) where idx is the winning coin's index
    pub fn is_slot_leader(&mut self, sigma1: pallas::Base, sigma2: pallas::Base) -> (bool, usize) {
        let mut won = false;
        let mut highest_stake = 0;
        let mut highest_stake_idx = 0;
        let _total_stake = self.total_stake();
        for (winning_idx, coin) in self.coins.iter().enumerate() {
            //println!("is_slot_leader: coin stake: {:?}", coin.value);
            //println!("is_slot_leader: total stake: {}", total_stake);
            //println!("is_slot_leader: relative stake: {}", (coin.value as f64) / total_stake as f64);
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
}

/// Utility function to extract leader selection lottery randomness(eta),
/// defined as the hash of the previous lead proof converted to pallas base.
fn get_eta(blockchain: &Blockchain) -> pallas::Base {
    let proof_tx_hash = blockchain.get_last_proof_hash().unwrap();
    let mut bytes: [u8; 32] = *proof_tx_hash.as_bytes();
    // read first 254 bits
    bytes[30] = 0;
    bytes[31] = 0;
    pallas::Base::from_repr(bytes).unwrap()
}

// Generate N nodes states
fn generate_nodes() -> Result<Vec<ConsensusState>> {
    println!("Generating {NODES} nodes...");

    // Generate a dummy DB to get initial coins eta from genesis block hash
    let db = sled::Config::new().temporary(true).open()?;
    let timestamp = Timestamp::current_time();
    let blockchain = Blockchain::new(&db, timestamp, *constants::TESTNET_GENESIS_HASH_BYTES)?;

    // Generate coins configuration
    let mut stakes = vec![];
    let mut initial_distribution = 0;
    for _ in 0..NODES {
        let stake = rand::thread_rng().gen_range(0..1000);
        //let stake = 100;
        initial_distribution += stake;
        stakes.push(stake);
    }
    let slot = 0;
    let eta = get_eta(&blockchain);
    let pid = PID::new();
    let mut nodes = vec![];
    for i in 0..NODES {
        println!("Generating node {i}");
        // Generate coin here to control stake
        let mut coins_tree =
            BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(constants::EPOCH_LENGTH * 100);
        let mut rng = thread_rng();
        let mut seeds: Vec<u64> = Vec::with_capacity(constants::EPOCH_LENGTH);
        for _ in 0..constants::EPOCH_LENGTH {
            seeds.push(rng.gen());
        }
        let epoch_secrets = LeadCoinSecrets::generate();
        let coin = LeadCoin::new(
            eta,
            stakes[i as usize],
            slot,
            epoch_secrets.secret_keys[0].inner(),
            epoch_secrets.merkle_roots[0],
            0,
            epoch_secrets.merkle_paths[0],
            pallas::Base::from(seeds[0]),
            &mut coins_tree,
        );

        let node_state = ConsensusState {
            current_slot: slot,
            initial_distribution,
            coins: vec![coin],
            coins_tree,
            leaders_history: vec![0],
            pid: pid.clone(),
        };

        nodes.push(node_state);
    }

    Ok(nodes)
}

#[async_std::main]
async fn main() -> Result<()> {
    // This script simulates the last man standing logic of replaying the crypsinous
    // leader election lottery until a single leader occurs, for instant finality.
    // Porpuse of the simulation is to validate if that logic is feasible as the network grows.

    // Generate nodes
    let mut nodes = generate_nodes()?;

    // In real conditions, everyone waits until a leader arises, and then the "draft" period begins,
    // where other leaders can join/challenge the fight for leadership. If a leader submits a proof after
    // that window passes, it gets ignorred.
    // Note: This time window is the min slot time.

    // Playing lottery for N slots
    for slot in 1..SLOTS {
        println!("Playing lottery for slot: {slot}");
        // Updating nodes
        for node in &mut nodes {
            node.current_slot = slot;
            // Clean leaders history
            //node.leaders_history = vec![0];
        }

        // Start slot loop
        let mut slot_leader: Option<usize> = None;
        loop {
            // Check if slot leader was found
            if let Some(leader) = slot_leader {
                println!("Slot {slot} leader: {leader}");
                // Rewarding leader
                let mut coins_tree = nodes[leader].coins_tree.clone();
                nodes[leader].coins[0] = nodes[leader].coins[0].derive_coin(&mut coins_tree);
                nodes[leader].coins_tree = coins_tree;
                break
            }

            // Draft round where everyone plays the lottery
            let mut sigmas: Vec<(pallas::Base, pallas::Base)> = vec![];
            let mut leaders = vec![];
            for (i, node) in nodes.iter_mut().enumerate() {
                // We verify all nodes will calculate the same sigmas
                let (sigma1, sigma2) = node.sigmas();
                for pair in &sigmas {
                    if sigma1 != pair.0 && sigma2 != pair.1 {
                        println!("ABORT, sigmas are wrong.");
                        return Ok(())
                    }
                }
                sigmas.push((sigma1, sigma2));
                let (won, _) = node.is_slot_leader(sigma1, sigma2);
                if won {
                    leaders.push(i);
                }
            }

            // Check if single leader was found
            if leaders.len() == 1 {
                slot_leader = Some(leaders[0]);
                continue
            }

            println!("Slot leaders: {:?}", leaders);

            // Updated nodes leaders history
            for node in &mut nodes {
                node.leaders_history.push(leaders.len() as i64);
            }

            // If more than one leaders occur, we enter the last man standing mode,
            // where they replay the lottery in specific time windows (rounds), until only one left.
            // Rounds should be the same time window as the draft period.
            // Also to "progress" to next round the node must have submitted proof for all the previous rounds.
            if leaders.len() > 1 {
                println!("Entering last man standing mode...");
                let mut round = 0;
                let mut survivors = vec![];
                loop {
                    println!("Round {round}, FIGHT!");
                    // Sanity check: we verify all nodes will calculate the same sigmas for round validations
                    let mut sigmas: Vec<(pallas::Base, pallas::Base)> = vec![];
                    for node in &nodes {
                        let (sigma1, sigma2) = node.sigmas();
                        for pair in &sigmas {
                            if sigma1 != pair.0 && sigma2 != pair.1 {
                                println!("ABORT, sigmas are wrong.");
                                return Ok(())
                            }
                        }
                        sigmas.push((sigma1, sigma2));
                    }

                    // Now leaders/survivors can replay the lottery
                    let participants =
                        if !survivors.is_empty() { survivors.clone() } else { leaders.clone() };
                    survivors = vec![];
                    for participant in &participants {
                        let (sigma1, sigma2) = nodes[*participant].sigmas();
                        // Verify no shenanigans happen when recalculating sigmas
                        if sigma1 != sigmas[*participant].0 && sigma2 != sigmas[*participant].1 {
                            println!("ABORT, participant sigmas are wrong.");
                            return Ok(())
                        }

                        let (won, _) = nodes[*participant].is_slot_leader(sigma1, sigma2);
                        if won {
                            survivors.push(*participant);
                        }
                    }

                    // Updated nodes leaders history
                    for node in &mut nodes {
                        node.leaders_history.push(survivors.len() as i64);
                    }

                    println!("Round {round} survivors: {:?}", survivors);
                    if survivors.is_empty() {
                        println!("Survivors didn't win round, terminating last man standing mode");
                        break
                    } else if survivors.len() == 1 {
                        println!("Node {} is the last man standing!", survivors[0]);
                        slot_leader = Some(survivors[0]);
                        break
                    }

                    round += 1;
                }
            }
        }
    }

    Ok(())
}
