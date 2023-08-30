/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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
    crypto::MerkleTree,
    num_traits::{One, Zero},
    pasta::{group::ff::FromUniformBytes, pallas},
};
use darkfi_serial::{async_trait, Encodable, SerialDecodable, SerialEncodable};
use num_bigint::BigUint;
use rand::{rngs::OsRng, Rng};
use randomx::{RandomXCache, RandomXDataset, RandomXFlags, RandomXVM};

#[cfg(test)]
mod tests;

// Number of threads to use for hashing
const NUM_THREADS: usize = 4;
/// Constant genesis block string used as the previous block hash
const GENESIS: &[u8] = b"genesis";
/// The output length of the BLAKE2b hash in bytes
const HASH_LEN: usize = 32;
/// Blocks, must be >=2
const DIFFICULTY_WINDOW: usize = 720;
/// Timestamps to cut after sorting, (2*DIFFICULTY_CUT<=DIFFICULTY_WINDOW-2) must be true
const DIFFICULTY_CUT: usize = 60;
/// !!!
const DIFFICULTY_LAG: usize = 15;
/// Target block time in seconds
const DIFFICULTY_TARGET: usize = 60;
/// The most recent blocks used to verify new blocks' timestamp
const BLOCKCHAIN_TIMESTAMP_CHECK_WINDOW: usize = 60;
/// Time limit in the future of what blocks can be
const BLOCK_FUTURE_TIME_LIMIT: u64 = 60 * 60 * 2;

#[derive(Clone, SerialEncodable, SerialDecodable)]
struct Transaction(Vec<u8>);

impl Transaction {
    fn hash(&self) -> Result<blake2b_simd::Hash> {
        let mut hasher = blake2b_simd::Params::new().hash_length(HASH_LEN).to_state();
        self.encode(&mut hasher)?;
        Ok(hasher.finalize())
    }
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
struct BlockHeader {
    nonce: u32,
    previous_hash: blake2b_simd::Hash,
    timestamp: Timestamp,
    txtree: MerkleTree,
}

#[derive(Clone, SerialEncodable, SerialDecodable)]
struct Block {
    header: BlockHeader,
    transactions: Vec<Transaction>,
}

impl Block {
    fn hash(&self) -> Result<blake2b_simd::Hash> {
        let mut len = 0;
        let mut hasher = blake2b_simd::Params::new().hash_length(HASH_LEN).to_state();

        len += self.header.encode(&mut hasher)?;
        len += self.header.txtree.root(0).unwrap().encode(&mut hasher)?;
        len += self.transactions.len().encode(&mut hasher)?;

        len.encode(&mut hasher)?;

        Ok(hasher.finalize())
    }

    fn insert_tx(&mut self, tx: &Transaction) -> Result<()> {
        let mut buf = [0u8; 64];
        buf[..HASH_LEN].copy_from_slice(tx.hash()?.as_bytes());
        let leaf = pallas::Base::from_uniform_bytes(&buf);
        self.header.txtree.append(leaf.into());
        self.transactions.push(tx.clone());
        Ok(())
    }
}

fn next_difficulty(
    mut timestamps: Vec<u64>,
    mut cummulative_difficulties: Vec<BigUint>,
    target_seconds: usize,
) -> BigUint {
    if timestamps.len() > DIFFICULTY_WINDOW {
        timestamps.resize(DIFFICULTY_WINDOW, 1);
        cummulative_difficulties.resize(DIFFICULTY_WINDOW, BigUint::one());
    }

    let length = timestamps.len();
    assert!(length == cummulative_difficulties.len());

    if length <= 1 {
        return BigUint::one()
    }

    assert!(length <= DIFFICULTY_WINDOW);
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
    assert!(total_work >= BigUint::zero());
    (total_work * target_seconds + time_span - BigUint::one()) / time_span
}

fn get_mid(a: u64, b: u64) -> u64 {
    (a / 2) + (b / 2) + ((a - 2 * (a / 2)) + (b - 2 * (b / 2))) / 2
}

fn median(v: &mut Vec<u64>) -> u64 {
    assert!(!v.is_empty());

    if v.len() == 1 {
        return v[0]
    }

    let n = v.len() / 2;
    v.sort();

    if v.len() % 2 == 0 {
        v[n]
    } else {
        get_mid(v[n - 1], v[n])
    }
}

fn check_block_timestamp_median(timestamps: &mut Vec<u64>, block: &Block) -> bool {
    let median_ts = median(timestamps);

    if block.header.timestamp.0 < median_ts {
        return false
    }

    true
}

fn check_block_timestamp(block: &Block, blockchain: &[Block]) -> bool {
    if block.header.timestamp.0 > Timestamp::current_time().0 + BLOCK_FUTURE_TIME_LIMIT {
        return false
    }

    // If not enough blocks, no proper median yet, return true
    if blockchain.len() < BLOCKCHAIN_TIMESTAMP_CHECK_WINDOW {
        return true
    }

    let mut timestamps: Vec<u64> = blockchain
        .iter()
        .rev()
        .take(BLOCKCHAIN_TIMESTAMP_CHECK_WINDOW)
        .map(|x| x.header.timestamp.0)
        .collect();

    check_block_timestamp_median(&mut timestamps, block)
}

fn main() -> Result<()> {
    // Construct the genesis block
    let genesis_hash =
        blake2b_simd::Params::new().hash_length(HASH_LEN).to_state().update(GENESIS).finalize();

    let mut genesis_block = Block {
        header: BlockHeader {
            nonce: 0,
            previous_hash: genesis_hash,
            timestamp: Timestamp::current_time(),
            txtree: MerkleTree::new(100),
        },
        transactions: vec![],
    };

    let genesis_tx = Transaction(vec![1, 3, 3, 7]);
    genesis_block.insert_tx(&genesis_tx)?;

    let mut blockchain: Vec<Block> = vec![];
    let mut cummulative_difficulties: Vec<BigUint> = vec![];

    let mut cummulative_difficulty = BigUint::zero();

    // Melt the CPU
    let mut n = 0;
    let mut cur_block = genesis_block;
    loop {
        // Calculate the next difficulty
        let begin: usize;
        let end: usize;
        if n < DIFFICULTY_WINDOW + DIFFICULTY_LAG {
            begin = 0;
            end = min(n, DIFFICULTY_WINDOW);
        } else {
            end = n - DIFFICULTY_LAG;
            begin = end - DIFFICULTY_WINDOW;
        }

        let timestamps: Vec<u64> =
            blockchain[begin..end].iter().map(|x| x.header.timestamp.0).collect();
        let difficulties: Vec<BigUint> = cummulative_difficulties[begin..end].to_vec();

        let difficulty = next_difficulty(timestamps, difficulties, DIFFICULTY_TARGET);

        let target = BigUint::from_bytes_be(&[
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff,
        ]) / &difficulty;
        println!("[{}] [MINER] Difficulty: {}", n, difficulty);
        println!("[{}] [MINER] Target: {}", n, target);

        // Get the PoW input. The key changes with every mined block.
        let pow_input = cur_block.hash()?;
        println!("[{}] [MINER] PoW Input: {}", n, pow_input.to_hex());

        let miner_setup = Instant::now();
        let flags = RandomXFlags::default() | RandomXFlags::FULLMEM;
        println!("[{}] [MINER] Initializing RandomX dataset...", n);
        let dataset =
            Arc::new(RandomXDataset::new(flags, pow_input.as_bytes(), NUM_THREADS).unwrap());

        // The miner creates a block
        let mut miner_block = Block {
            header: BlockHeader {
                nonce: 0,
                previous_hash: cur_block.hash()?,
                timestamp: Timestamp::current_time(),
                txtree: MerkleTree::new(100),
            },
            transactions: vec![],
        };
        let tx0 = Transaction(OsRng.gen::<[u8; 32]>().to_vec());
        let tx1 = Transaction(OsRng.gen::<[u8; 32]>().to_vec());
        miner_block.insert_tx(&tx0)?;
        miner_block.insert_tx(&tx1)?;
        println!("[{}] [MINER] Setup time: {:?}", n, miner_setup.elapsed());

        // Multithreaded mining setup
        let mining_time = Instant::now();
        let mut handles = vec![];
        let found_block = Arc::new(AtomicBool::new(false));
        let found_nonce = Arc::new(AtomicU32::new(0));
        for t in 0..NUM_THREADS {
            let target = target.clone();
            let mut block = miner_block.clone();
            let found_block = Arc::clone(&found_block);
            let found_nonce = Arc::clone(&found_nonce);
            let dataset = dataset.clone();
            handles.push(thread::spawn(move || {
                println!("[{}] [MINER] Initializing RandomX VM #{}...", n, t);
                block.header.nonce = t as u32;
                let vm = RandomXVM::new_fast(flags, &dataset).unwrap();
                loop {
                    if found_block.load(Ordering::SeqCst) {
                        println!("[{}] [MINER] Block was found, thread #{} exiting", n, t);
                        break
                    }

                    let out_hash = vm.hash(block.hash().unwrap().as_bytes());
                    let out_hash = BigUint::from_bytes_be(&out_hash);
                    if out_hash <= target {
                        found_block.store(true, Ordering::SeqCst);
                        found_nonce.store(block.header.nonce, Ordering::SeqCst);
                        println!(
                            "[{}] [MINER] Thread #{} found block using nonce {}",
                            n, t, block.header.nonce
                        );
                        println!("[{}] [MINER] Block hash {}", n, block.hash().unwrap().to_hex(),);
                        println!("[{}] [MINER] RandomX hash bytes: {:?}", n, out_hash);
                        break
                    }

                    // This means thread 0 will use nonces, 0, 4, 8, ...
                    // and thread 1 will use nonces, 1, 5, 9, ...
                    block.header.nonce += NUM_THREADS as u32;
                }
            }))
        }

        for handle in handles {
            let _ = handle.join();
        }
        println!("[{}] [MINER] Mining time: {:?}", n, mining_time.elapsed());

        // Set the valid mined nonce in the block that's broadcasted
        miner_block.header.nonce = found_nonce.load(Ordering::SeqCst);

        // Verify
        assert!(check_block_timestamp(&miner_block, &blockchain));

        let verifier_setup = Instant::now();
        let flags = RandomXFlags::default();
        let cache = RandomXCache::new(flags, pow_input.as_bytes()).unwrap();
        let vm = RandomXVM::new(flags, &cache).unwrap();
        println!("[{}] [VERIFIER] Setup time: {:?}", n, verifier_setup.elapsed());

        let verification_time = Instant::now();
        let out_hash = vm.hash(miner_block.hash()?.as_bytes());
        let out_hash = BigUint::from_bytes_be(&out_hash);
        assert!(out_hash <= target);
        println!("[{}] [VERIFIER] Verification time: {:?}", n, verification_time.elapsed());

        // The new block appends to the blockchain
        cur_block = miner_block.clone();
        blockchain.push(miner_block);
        cummulative_difficulty += difficulty;
        cummulative_difficulties.push(cummulative_difficulty.clone());

        n += 1;
    }
}
