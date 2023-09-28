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
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
    thread,
    time::Instant,
};

use darkfi_sdk::{
    num_traits::{One, Zero},
    pasta::pallas,
};
use log::info;
use num_bigint::BigUint;
use randomx::{RandomXCache, RandomXDataset, RandomXFlags, RandomXVM};
use smol::channel::Receiver;

use crate::{
    blockchain::{BlockInfo, Blockchain},
    util::{ringbuffer::RingBuffer, time::Timestamp},
    Error, Result,
};

// TODO: replace asserts with error returns
// TODO: Set correct log targets
// TODO: verify why we use Instant here instead of our own Timestamp

// Note: We have combined some constants for better performance.
/// Default number of threads to use for hashing
const N_THREADS: usize = 4;
/// Amount of max items(blocks) to use for next difficulty calculation.
/// Must be >= 2 and == BUF_SIZE - DIFFICULTY_LAG.
const DIFFICULTY_WINDOW: usize = 720;
/// Amount of latest blocks to exlude from the calculation.
/// Our ring buffer has length: DIFFICULTY_WINDOW + DIFFICULTY_LAG,
/// but we only use DIFFICULTY_WINDOW items in calculations.
/// Must be == BUF_SIZE - DIFFICULTY_WINDOW.
const _DIFFICULTY_LAG: usize = 15;
/// Ring buffer length.
/// Must be == DIFFICULTY_WINDOW + DIFFICULTY_LAG
const BUF_SIZE: usize = 735;
/// Used to calculate how many items to retain for next difficulty
/// calculation. We are keeping the middle items, meaning cutting
/// both from frond and back of the ring buffer, ending up with max
/// DIFFICULTY_WINDOW - 2*DIFFICULTY_CUT items.
/// (2*DIFFICULTY_CUT <= DIFFICULTY_WINDOW-2) must be true.
const _DIFFICULTY_CUT: usize = 60;
/// Max items to use for next difficulty calculation.
/// Must be DIFFICULTY_WINDOW - 2 * DIFFICULTY_CUT
const RETAINED: usize = 600;
/// Already known cutoff start index for this config
const CUT_BEGIN: usize = 60;
/// Already known cutoff end index for this config
const CUT_END: usize = 660;
/// Default target block time, in seconds
const DIFFICULTY_TARGET: usize = 20;
/// How many most recent blocks to use to verify new blocks' timestamp
const BLOCKCHAIN_TIMESTAMP_CHECK_WINDOW: usize = 60;
/// Time limit in the future of what blocks can be
const BLOCK_FUTURE_TIME_LIMIT: u64 = 60 * 60 * 2;

/// This struct represents the information required by the PoW algorithm
#[derive(Clone)]
pub struct PoWModule {
    /// Canonical (finalized) blockchain
    pub blockchain: Blockchain,
    /// Number of threads to use for hashing,
    /// if None provided will use N_THREADS
    pub threads: usize,
    /// Target block time, in seconds,
    /// if None provided will use DIFFICULTY_TARGET
    pub target: usize,
    /// Latest block timestamps ringbuffer
    pub timestamps: RingBuffer<u64, BUF_SIZE>,
    /// Latest block cummulative difficulties ringbuffer
    pub difficulties: RingBuffer<BigUint, BUF_SIZE>,
    /// Total blocks cummulative difficulty
    pub cummulative_difficulty: BigUint,
}

impl PoWModule {
    pub fn new(blockchain: Blockchain, threads: Option<usize>, target: Option<usize>) -> Self {
        let threads = if let Some(t) = threads { t } else { N_THREADS };
        let target = if let Some(t) = target { t } else { DIFFICULTY_TARGET };
        // TODO: store/retrieve info in/from sled
        let timestamps = RingBuffer::<u64, BUF_SIZE>::new();
        let difficulties = RingBuffer::<BigUint, BUF_SIZE>::new();
        let cummulative_difficulty = BigUint::zero();
        Self { blockchain, threads, target, timestamps, difficulties, cummulative_difficulty }
    }

    /// Compute the next mining difficulty, based on current ring buffers.
    /// If ring buffers contain 2 or less items, difficulty 1 is returned.
    // TODO: difficulty 1 for first 2 blocks makes cummulative difficulty
    //       to increment slowly, making diversion to target very slow.
    //       We should increase this value after testing, so blocks diverge
    //       to target block time faster.
    pub fn next_difficulty(&self) -> BigUint {
        // Retrieve first DIFFICULTY_WINDOW timestamps from the ring buffer
        let mut timestamps: Vec<u64> =
            self.timestamps.iter().take(DIFFICULTY_WINDOW).copied().collect();

        // Check we have enough timestamps
        let length = timestamps.len();
        if length < 2 {
            return BigUint::one()
        }

        // Sort the timestamps vector
        timestamps.sort_unstable();

        // Grab cutoff indexes
        let (cut_begin, cut_end) = self.cutoff(length);

        // Calculate total time span
        let cut_end = cut_end - 1;
        let mut time_span = timestamps[cut_end] - timestamps[cut_begin];
        if time_span == 0 {
            time_span = 1;
        }

        // Calculate total work done during this time span
        let total_work = &self.difficulties[cut_end] - &self.difficulties[cut_begin];
        assert!(total_work > BigUint::zero());

        (total_work * self.target + time_span - BigUint::one()) / time_span
    }

    /// Calculate cutoff indexes.
    /// If buffers have been filled, we return the
    /// already known indexes, for performance.
    fn cutoff(&self, length: usize) -> (usize, usize) {
        if length >= DIFFICULTY_WINDOW {
            return (CUT_BEGIN, CUT_END)
        }

        let (cut_begin, cut_end) = if length <= RETAINED {
            (0, length)
        } else {
            let cut_begin = (length - RETAINED + 1) / 2;
            (cut_begin, cut_begin + RETAINED)
        };
        // Sanity check
        assert!(/* cut_begin >= 0 && */ cut_begin + 2 <= cut_end && cut_end <= length);

        (cut_begin, cut_end)
    }

    /// Compute the next mine target
    pub fn next_mine_target(&self) -> BigUint {
        BigUint::from_bytes_be(&[0xFF; 32]) / &self.next_difficulty()
    }

    /// Verify provided difficulty corresponds to the next one
    pub fn verify_difficulty(&self, difficulty: &BigUint) -> bool {
        difficulty == &self.next_difficulty()
    }

    /// Verify provided block timestamp is valid and matches certain criteria
    pub fn verify_timestamp(&self, timestamp: u64) -> bool {
        if timestamp > Timestamp::current_time().0 + BLOCK_FUTURE_TIME_LIMIT {
            return false
        }

        // If not enough blocks, no proper median yet, return true
        if self.timestamps.len() < BLOCKCHAIN_TIMESTAMP_CHECK_WINDOW {
            return true
        }

        // Make sure the timestamp is higher or equal to the median
        let timestamps =
            self.timestamps.iter().rev().take(BLOCKCHAIN_TIMESTAMP_CHECK_WINDOW).copied().collect();
        timestamp >= median(timestamps)
    }

    /// Verify provided block timestamp and difficulty pair
    pub fn verify_pair(&self, timestamp: u64, difficulty: &BigUint) {
        assert!(self.verify_timestamp(timestamp));
        assert!(self.verify_difficulty(difficulty));
    }

    /// Verify provided block corresponds to next mine target
    pub fn verify_block(&self, block: &BlockInfo) -> Result<()> {
        // First we verify the block's timestamp
        assert!(self.verify_timestamp(block.header.timestamp.0));

        // Then we verify the proof of work:
        let verifier_setup = Instant::now();

        // Grab the next mine target
        let target = self.next_mine_target();

        // Setup verifier
        let flags = RandomXFlags::default();
        let cache = RandomXCache::new(flags, block.header.previous.as_bytes()).unwrap();
        let vm = RandomXVM::new(flags, &cache).unwrap();
        info!(target: "validator::pow::verify_block", "[VERIFIER] Setup time: {:?}", verifier_setup.elapsed());

        // Compute the output hash
        let verification_time = Instant::now();
        let out_hash = vm.hash(block.hash()?.as_bytes());
        let out_hash = BigUint::from_bytes_be(&out_hash);

        // Verify hash is less than the expected mine target
        assert!(out_hash <= target);
        info!(target: "validator::pow::verify_block", "[VERIFIER] Verification time: {:?}", verification_time.elapsed());

        Ok(())
    }

    /// Append provided timestamp and difficulty to the ring buffers
    pub fn append(&mut self, timestamp: u64, difficulty: &BigUint) {
        self.timestamps.push(timestamp);
        self.cummulative_difficulty += difficulty;
        self.difficulties.push(self.cummulative_difficulty.clone());
    }

    /// Mine provided block, based on provided PoW module next mine target and difficulty
    pub fn mine_block(
        &self,
        miner_block: &mut BlockInfo,
        stop_signal: &Receiver<()>,
    ) -> Result<()> {
        let miner_setup = Instant::now();

        // Grab the next mine target
        let target = self.next_mine_target();
        info!(target: "validator::pow::mine_block", "[MINER] Mine target: 0x{:064x}", target);

        // Get the PoW input. The key changes with every mined block.
        let input = miner_block.header.previous;
        info!(target: "validator::pow::mine_block", "[MINER] PoW input: {}", input.to_hex());
        let flags = RandomXFlags::default() | RandomXFlags::FULLMEM;
        info!(target: "validator::pow::mine_block", "[MINER] Initializing RandomX dataset...");
        let dataset = Arc::new(RandomXDataset::new(flags, input.as_bytes(), self.threads).unwrap());
        info!(target: "validator::pow::mine_block", "[MINER] Setup time: {:?}", miner_setup.elapsed());

        // Multithreaded mining setup
        let mining_time = Instant::now();
        let mut handles = vec![];
        let found_block = Arc::new(AtomicBool::new(false));
        let found_nonce = Arc::new(AtomicU32::new(0));
        let threads = self.threads as u32;
        for t in 0..threads {
            let target = target.clone();
            let mut block = miner_block.clone();
            let found_block = Arc::clone(&found_block);
            let found_nonce = Arc::clone(&found_nonce);
            let dataset = Arc::clone(&dataset);
            let stop_signal = stop_signal.clone();

            handles.push(thread::spawn(move || {
                info!(target: "validator::pow::mine_block", "[MINER] Initializing RandomX VM #{}...", t);
                let mut miner_nonce = t;
                let vm = RandomXVM::new_fast(flags, &dataset).unwrap();
                loop {
                    // Check if stop signal was received
                    if stop_signal.is_full() {
                        info!(target: "validator::pow::mine_block", "[MINER] Stop signal received, thread #{} exiting", t);
                        break
                    }

                    block.header.nonce = pallas::Base::from(miner_nonce as u64);
                    if found_block.load(Ordering::SeqCst) {
                        info!(target: "validator::pow::mine_block", "[MINER] Block found, thread #{} exiting", t);
                        break
                    }

                    let out_hash = vm.hash(block.hash().unwrap().as_bytes());
                    let out_hash = BigUint::from_bytes_be(&out_hash);
                    if out_hash <= target {
                        found_block.store(true, Ordering::SeqCst);
                        found_nonce.store(miner_nonce, Ordering::SeqCst);
                        info!(target: "validator::pow::mine_block", "[MINER] Thread #{} found block using nonce {}",
                            t, miner_nonce
                        );
                        info!(target: "validator::pow::mine_block", "[MINER] Block hash {}", block.hash().unwrap().to_hex());
                        info!(target: "validator::pow::mine_block", "[MINER] RandomX output: 0x{:064x}", out_hash);
                        break
                    }

                    // This means thread 0 will use nonces, 0, 4, 8, ...
                    // and thread 1 will use nonces, 1, 5, 9, ...
                    miner_nonce += threads;
                }
            }));
        }

        for handle in handles {
            let _ = handle.join();
        }
        // Check if stop signal was received
        if stop_signal.is_full() {
            return Err(Error::MinerTaskStopped)
        }

        info!(target: "validator::pow::mine_block", "[MINER] Mining time: {:?}", mining_time.elapsed());

        // Set the valid mined nonce in the block
        miner_block.header.nonce = pallas::Base::from(found_nonce.load(Ordering::SeqCst) as u64);

        Ok(())
    }
}

impl std::fmt::Display for PoWModule {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "PoWModule:")?;
        write!(f, "\tthreads: {}", self.threads)?;
        write!(f, "\ttarget: {}", self.target)?;
        write!(f, "\ttimestamps: {:?}", self.timestamps)?;
        write!(f, "\tdifficulties: {:?}", self.difficulties)?;
        write!(f, "\tcummulative_difficulty: {}", self.cummulative_difficulty)
    }
}

// TODO: move these to utils or something
fn get_mid(a: u64, b: u64) -> u64 {
    (a / 2) + (b / 2) + ((a - 2 * (a / 2)) + (b - 2 * (b / 2))) / 2
}

/// Aux function to calculate the median of a given `Vec<u64>`.
/// The function sorts the vector internally.
fn median(mut v: Vec<u64>) -> u64 {
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

#[cfg(test)]
mod tests {
    use std::{
        io::{BufRead, Cursor},
        process::Command,
    };

    use darkfi_sdk::num_traits::Num;
    use num_bigint::BigUint;

    use crate::{
        blockchain::{BlockInfo, Blockchain},
        Result,
    };

    use super::PoWModule;

    const DEFAULT_TEST_DIFFICULTY_TARGET: usize = 120;

    #[test]
    fn test_wide_difficulty() -> Result<()> {
        let sled_db = sled::Config::new().temporary(true).open()?;
        let blockchain = Blockchain::new(&sled_db)?;
        let mut module = PoWModule::new(blockchain, None, Some(DEFAULT_TEST_DIFFICULTY_TARGET));

        let output = Command::new("./script/research/pow/gen_wide_data.py").output().unwrap();
        let reader = Cursor::new(output.stdout);

        for (n, line) in reader.lines().enumerate() {
            let line = line.unwrap();
            let parts: Vec<String> = line.split(' ').map(|x| x.to_string()).collect();
            assert!(parts.len() == 2);

            let timestamp = parts[0].parse::<u64>().unwrap();
            let difficulty = BigUint::from_str_radix(&parts[1], 10).unwrap();

            let res = module.next_difficulty();

            if res != difficulty {
                eprintln!("Wrong wide difficulty for block {}", n);
                eprintln!("Expected: {}", difficulty);
                eprintln!("Found: {}", res);
                assert!(res == difficulty);
            }

            module.append(timestamp, &difficulty);
        }

        Ok(())
    }

    #[test]
    fn test_miner_correctness() -> Result<()> {
        // Default setup
        let sled_db = sled::Config::new().temporary(true).open()?;
        let blockchain = Blockchain::new(&sled_db)?;
        let module = PoWModule::new(blockchain, None, Some(DEFAULT_TEST_DIFFICULTY_TARGET));
        let (_, recvr) = smol::channel::bounded(1);
        let genesis_block = BlockInfo::default();

        // Mine next block
        let mut next_block = BlockInfo::default();
        next_block.header.previous = genesis_block.hash()?;
        module.mine_block(&mut next_block, &recvr)?;

        // Verify it
        module.verify_block(&next_block)?;

        Ok(())
    }
}
