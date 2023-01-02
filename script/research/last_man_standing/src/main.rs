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
        constants::{EPOCH_LENGTH, TESTNET_GENESIS_HASH_BYTES},
        leadcoin::{LeadCoin, LeadCoinSecrets},
        state::ConsensusState,
    },
    util::{async_util::sleep, time::Timestamp},
    Result,
};
use darkfi_sdk::pasta::pallas;
use rand::{thread_rng, Rng};

// Simulation configuration
const N: u64 = 10;
const INIT_DISTRIBUTION: u64 = 1000;

// Generate N nodes states
fn generate_nodes() -> Result<Vec<ConsensusState>> {
    println!("Generating {N} nodes...");
    let stake = INIT_DISTRIBUTION / N;
    let mut nodes = vec![];
    for i in 0..N {
        println!("Generating node {i}");
        let db = sled::Config::new().temporary(true).open()?;
        let timestamp = Timestamp::current_time();
        let blockchain = Blockchain::new(&db, timestamp, *TESTNET_GENESIS_HASH_BYTES)?;
        let mut node_state = ConsensusState::new(
            blockchain,
            timestamp,
            timestamp,
            *TESTNET_GENESIS_HASH_BYTES,
            INIT_DISTRIBUTION,
        )?;

        // Generate coin here to control stake
        let slot = node_state.current_slot();
        let eta = node_state.get_eta();
        let mut rng = thread_rng();
        let mut seeds: Vec<u64> = Vec::with_capacity(EPOCH_LENGTH);
        for _ in 0..EPOCH_LENGTH {
            seeds.push(rng.gen());
        }
        let epoch_secrets = LeadCoinSecrets::generate();
        let coin = LeadCoin::new(
            eta,
            stake,
            slot,
            epoch_secrets.secret_keys[0].inner(),
            epoch_secrets.merkle_roots[0],
            0,
            epoch_secrets.merkle_paths[0],
            pallas::Base::from(seeds[0]),
            &mut node_state.coins_tree,
        );
        node_state.coins.push(coin);
        node_state.proposing = true;
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

    // Skip genesis slot.
    // Note: increase slot duration if nodes generation takes longer than it
    let seconds_next_slot = nodes[0].next_n_slot_start(2).as_secs();
    println!("Waiting for next slot ({seconds_next_slot} sec)");
    sleep(seconds_next_slot).await;

    // Playing lottery
    let mut leaders = vec![];
    let slot = nodes[0].current_slot();
    let (sigma1, sigma2) = nodes[0].sigmas();
    println!("Playing lottery for slot: {slot}");
    for (i, node) in nodes.iter_mut().enumerate() {
        let (won, _, _) = node.is_slot_leader(sigma1, sigma2);
        if won {
            leaders.push(i);
        }
    }
    // In real conditions, everyone waits until a leader arises, and then the "draft" period begins,
    // where other leaders can join/challenge the fight for leadership. If a leader submits a proof after
    // that window passes, it gets ignorred.
    // Note: This time window is the min slot time.
    println!("Slot leaders: {:?}", leaders);
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
            let participants = if !survivors.is_empty() {
                survivors.clone()
            } else {
                leaders.clone()
            };
            survivors = vec![];
            for participant in &participants {
                // We derive the new coin. In real conditions, slot sigmas should adapt on how many
                // leaders/survivors we have seen on each round.
                let mut coins_tree = nodes[*participant].coins_tree.clone();
                nodes[*participant].coins[0] = nodes[*participant].coins[0].derive_coin(&mut coins_tree);
                nodes[*participant].coins_tree = coins_tree;
                let (won, _, _) = nodes[*participant].is_slot_leader(sigma1, sigma2);
                if won {
                    survivors.push(*participant);
                }
            }
            println!("Round {round} survivors: {:?}", survivors);
            if survivors.is_empty() {
                println!("Survivors didn't win round, terminating last man standing mode");
                break
            } else if survivors.len() == 1 {
                println!("Node {} is the last man standing!", survivors[0]);
                break
            }
            round += 1;
        }
    }

    Ok(())
}
