/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
 * Copyright (C) 2014-2023 The Monero Project (Under MIT license)
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
    cmp::min,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
    thread,
    time::Instant,
};

use darkfi::{util::time::Timestamp, Result};
use darkfi_sdk::{
    crypto::{pasta_prelude::Field, MerkleTree},
    num_traits::{One, Zero},
    pasta::{group::ff::FromUniformBytes, pallas},
};
use darkfi_serial::{async_trait, Encodable, SerialEncodable};
use lazy_static::lazy_static;
use num_bigint::BigUint;
use rand::{rngs::OsRng, Rng};
use randomx::{RandomXCache, RandomXDataset, RandomXFlags, RandomXVM};

#[cfg(test)]
mod tests;

/// Number of threads to use for hashing
const N_THREADS: usize = 4;
/// The output length of the BLAKE2b hash in bytes
const HASH_LEN: usize = 32;
/// Amount of blocks to take for next difficulty calculation.
/// Must be >= 2
const DIFFICULTY_WINDOW: usize = 720;
/// Timestamps to cut after sorting for next difficulty calculation.
/// (2*DIFFICULTY_CUT <= DIFFICULTY_WINDOW-2) must be true.
const DIFFICULTY_CUT: usize = 60;
/// !!!
const DIFFICULTY_LAG: usize = 15;
/// Target block time in seconds
const DIFFICULTY_TARGET: usize = 20;
/// How many most recent blocks to use to verify new blocks' timestamp
const BLOCKCHAIN_TIMESTAMP_CHECK_WINDOW: usize = 60;
/// Time limit in the future of what blocks can be
const BLOCK_FUTURE_TIME_LIMIT: u64 = 60 * 60 * 2;

lazy_static! {
    /// The genesis block hash
    static ref GENESIS_HASH: blake2b_simd::Hash =
        blake2b_simd::Params::new().hash_length(HASH_LEN).to_state().update(b"genesis").finalize();
}

#[derive(Clone, SerialEncodable)]
/// Dummy transaction definition
struct Transaction(Vec<u8>);

impl Transaction {
    /// Hash the transaction
    fn hash(&self) -> Result<blake2b_simd::Hash> {
        let mut hasher = blake2b_simd::Params::new().hash_length(HASH_LEN).to_state();
        self.encode(&mut hasher)?;
        Ok(hasher.finalize())
    }
}

#[derive(Clone, SerialEncodable)]
/// A block's header
struct BlockHeader {
    /// The block's nonce, represented as a pallas::Base.
    /// This value changes arbitrarily with mining.
    nonce: pallas::Base,
    /// The hash of the previous block in the blockchain
    previous_hash: [u8; HASH_LEN],
    /// The block timestamp
    timestamp: u64,
    /// Merkle tree of the transactions contained in this block
    txtree: MerkleTree,
}

#[derive(Clone, SerialEncodable)]
/// Block definition
struct Block {
    /// The block header
    header: BlockHeader,
    /// Transactions contained in the block
    txs: Vec<Transaction>,
}

impl Block {
    /// Compute the block's hash
    fn hash(&self) -> Result<blake2b_simd::Hash> {
        let mut hasher = blake2b_simd::Params::new().hash_length(HASH_LEN).to_state();

        self.header.nonce.encode(&mut hasher)?;
        self.header.previous_hash.encode(&mut hasher)?;
        self.header.timestamp.encode(&mut hasher)?;
        self.header.txtree.root(0).unwrap().encode(&mut hasher)?;

        Ok(hasher.finalize())
    }

    /// Append a transaction to the block. Also adds it to the Merkle tree.
    fn append_tx(&mut self, tx: Transaction) -> Result<()> {
        let mut buf = [0u8; 64];
        buf[..HASH_LEN].copy_from_slice(tx.hash()?.as_bytes());
        let leaf = pallas::Base::from_uniform_bytes(&buf);

        self.header.txtree.append(leaf.into());
        self.txs.push(tx);

        Ok(())
    }
}

fn get_mid(a: u64, b: u64) -> u64 {
    (a / 2) + (b / 2) + ((a - 2 * (a / 2)) + (b - 2 * (b / 2))) / 2
}

/// Aux function to calculate the median of a given `Vec<u64>`.
/// The function sorts the vector internally.
fn median(v: &mut Vec<u64>) -> u64 {
    assert!(v.is_empty());

    if v.len() == 1 {
        return v[0]
    }

    let n = v.len() / 2;
    v.sort_unstable();

    if v.len() % 2 == 0 {
        v[n]
    } else {
        get_mid(v[n - 1], v[n])
    }
}

/// Verify a block's timestamp is valid and matches certain criteria.
fn check_block_timestamp(block: &Block, timestamps: &mut Vec<u64>) -> bool {
    if block.header.timestamp > Timestamp::current_time().inner() + BLOCK_FUTURE_TIME_LIMIT {
        return false
    }

    // If not enough blocks, no proper median yet, return true
    if timestamps.len() < BLOCKCHAIN_TIMESTAMP_CHECK_WINDOW {
        return true
    }

    // Make sure the timestamp is higher than the median
    if block.header.timestamp < median(timestamps) {
        return false
    }

    true
}

/// Calculate the next mining difficulty.
///
/// Takes a `RingBuffer` of timestamps, a `RingBuffer` of cummulative
/// difficulties, and a target block time in seconds.
/// **NOTE**: `timestamps` get sorted in this function.
///
/// Panics if:
/// * `timestamps.len() != cummulative_difficulties.len()`
/// * `timestamps.len() > DIFFICULTY_WINDOW`
fn next_difficulty(
    timestamps: &mut Vec<u64>,
    cummulative_difficulties: &[BigUint],
    target_seconds: usize,
) -> BigUint {
    let length = timestamps.len();
    assert!(length == cummulative_difficulties.len() && length <= DIFFICULTY_WINDOW);

    if length <= 1 {
        return BigUint::one()
    }

    // Sort the timestamps vector
    timestamps.sort_unstable();

    let cut_begin: usize;
    let cut_end: usize;

    if length <= DIFFICULTY_WINDOW - 2 * DIFFICULTY_CUT {
        cut_begin = 0;
        cut_end = length;
    } else {
        cut_begin = (length - (DIFFICULTY_WINDOW - 2 * DIFFICULTY_CUT) + 1) / 2;
        cut_end = cut_begin + (DIFFICULTY_WINDOW - 2 * DIFFICULTY_CUT);
    }

    assert!(/* cut_begin >= 0 && */ cut_begin + 2 <= cut_end && cut_end <= length);

    let mut time_span = timestamps[cut_end - 1] - timestamps[cut_begin];
    if time_span == 0 {
        time_span = 1;
    }

    let total_work = &cummulative_difficulties[cut_end - 1] - &cummulative_difficulties[cut_begin];
    assert!(total_work > BigUint::zero());

    (total_work * target_seconds + time_span - BigUint::one()) / time_span
}

fn main() -> Result<()> {
    // Construct the genesis block
    let mut previous_hash = [0u8; HASH_LEN];
    previous_hash.copy_from_slice(GENESIS_HASH.as_bytes());

    let mut genesis_block = Block {
        header: BlockHeader {
            nonce: pallas::Base::ZERO,
            previous_hash,
            timestamp: Timestamp::current_time().inner(),
            txtree: MerkleTree::new(1),
        },
        txs: vec![],
    };

    let genesis_tx = Transaction(vec![1, 3, 3, 7]);
    genesis_block.append_tx(genesis_tx)?;

    // This represents the blocks in our blockchain
    let mut blockchain: Vec<Block> = vec![genesis_block.clone()];
    // The cummulative difficulties track difficulty through time.
    // The genesis block (block 0) is ignored. Blocks 1 and 2 must have difficulty 1.
    let mut difficulties = vec![];
    let mut cummulative_difficulty = BigUint::zero();
    // We also track block timestamps this way.
    let mut timestamps = vec![];

    // Melt the CPU
    loop {
        // Reference to our chain tip
        let n = blockchain.len(); // Block height
        let cur_block = &blockchain.last().unwrap();
        assert!(difficulties.len() == timestamps.len() && timestamps.len() == n - 1);

        // Calculate the next difficulty target: T = 2^256 / difficulty
        let begin: usize;
        let end: usize;
        if n - 1 < DIFFICULTY_WINDOW + DIFFICULTY_LAG {
            begin = 0;
            end = min(n - 1, DIFFICULTY_WINDOW);
        } else {
            end = n - 1 - DIFFICULTY_LAG;
            begin = end - DIFFICULTY_WINDOW;
        }

        let mut ts: Vec<u64> = timestamps[begin..end].to_vec();
        let difficulty = next_difficulty(&mut ts, &difficulties[begin..end], DIFFICULTY_TARGET);
        let target = BigUint::from_bytes_be(&[0xFF; 32]) / &difficulty;
        println!("[#{}] [MINER] Difficulty:  0x{:064x}", n, difficulty);
        println!("[#{}] [MINER] Mine target: 0x{:064x}", n, target);

        // Get the PoW input. The key changes with every mined block.
        let powinput = cur_block.hash()?;
        println!("[#{}] [MINER] PoW input: {}", n, powinput.to_hex());

        let miner_setup = Instant::now();
        let flags = RandomXFlags::default() | RandomXFlags::FULLMEM;
        println!("[#{}] [MINER] Initializing RandomX dataset...", n);
        let dataset = Arc::new(RandomXDataset::new(flags, powinput.as_bytes(), N_THREADS).unwrap());

        // The miner creates a block
        let mut previous_hash = [0u8; HASH_LEN];
        previous_hash.copy_from_slice(cur_block.hash()?.as_bytes());
        let mut miner_block = Block {
            header: BlockHeader {
                nonce: pallas::Base::ZERO,
                previous_hash,
                timestamp: Timestamp::current_time().inner(),
                txtree: MerkleTree::new(1),
            },
            txs: vec![],
        };

        // Insert some transactions from the mempool
        let tx0 = Transaction(OsRng.gen::<[u8; 32]>().to_vec());
        let tx1 = Transaction(OsRng.gen::<[u8; 32]>().to_vec());
        miner_block.append_tx(tx0)?;
        miner_block.append_tx(tx1)?;
        println!("[#{}] [MINER] Setup time: {:?}", n, miner_setup.elapsed());

        // Multithreaded mining setup
        let mining_time = Instant::now();
        let mut handles = vec![];
        let found_block = Arc::new(AtomicBool::new(false));
        let found_nonce = Arc::new(AtomicU32::new(0));
        for t in 0..N_THREADS {
            let target = target.clone();
            let mut block = miner_block.clone();
            let found_block = Arc::clone(&found_block);
            let found_nonce = Arc::clone(&found_nonce);
            let dataset = Arc::clone(&dataset);

            handles.push(thread::spawn(move || {
                println!("[#{}] [MINER] Initializing RandomX VM #{}...", n, t);
                let mut miner_nonce = t as u32;
                let vm = RandomXVM::new_fast(flags, &dataset).unwrap();
                loop {
                    block.header.nonce = pallas::Base::from(miner_nonce as u64);
                    if found_block.load(Ordering::SeqCst) {
                        println!("[#{}] [MINER] Block found, thread #{} exiting", n, t);
                        break
                    }

                    let out_hash = vm.hash(block.hash().unwrap().as_bytes());
                    let out_hash = BigUint::from_bytes_be(&out_hash);
                    if out_hash <= target {
                        found_block.store(true, Ordering::SeqCst);
                        found_nonce.store(miner_nonce, Ordering::SeqCst);
                        println!(
                            "[#{}] [MINER] Thread #{} found block using nonce {}",
                            n, t, miner_nonce
                        );
                        println!("[#{}] [MINER] Block hash {}", n, block.hash().unwrap().to_hex());
                        println!("[#{}] [MINER] RandomX output: 0x{:064x}", n, out_hash);
                        break
                    }

                    // This means thread 0 will use nonces, 0, 4, 8, ...
                    // and thread 1 will use nonces, 1, 5, 9, ...
                    miner_nonce += N_THREADS as u32;
                }
            }));
        }

        for handle in handles {
            let _ = handle.join();
        }
        println!("[#{}] [MINER] Mining time: {:?}", n, mining_time.elapsed());

        // Set the valid mined nonce in the block that's being broadcasted
        miner_block.header.nonce = pallas::Base::from(found_nonce.load(Ordering::SeqCst) as u64);

        // Now the block is broadcasted to the network, and a node can verify it.
        // First we verify the block's timestamp. We take the last
        // `BLOCKCHAIN_TIMESTAMP_CHECK_WINDOW` timestamps and perform the check:
        let mut v_ts =
            timestamps.iter().rev().take(BLOCKCHAIN_TIMESTAMP_CHECK_WINDOW).copied().collect();
        assert!(check_block_timestamp(&miner_block, &mut v_ts));

        // Then we verify the proof of work:
        let verifier_setup = Instant::now();
        let flags = RandomXFlags::default();
        let cache = RandomXCache::new(flags, powinput.as_bytes()).unwrap();
        let vm = RandomXVM::new(flags, &cache).unwrap();
        println!("[#{}] [VERIFIER] Setup time: {:?}", n, verifier_setup.elapsed());

        let verification_time = Instant::now();
        let out_hash = vm.hash(miner_block.hash()?.as_bytes());
        let out_hash = BigUint::from_bytes_be(&out_hash);
        assert!(out_hash <= target);
        println!("[#{}] [VERIFIER] Verification time: {:?}", n, verification_time.elapsed());

        // The new block appends to the blockchain
        timestamps.push(miner_block.header.timestamp);
        blockchain.push(miner_block);
        cummulative_difficulty += difficulty;
        difficulties.push(cummulative_difficulty.clone());
    }
}
