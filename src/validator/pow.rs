/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use darkfi_sdk::num_traits::{One, Zero};
use num_bigint::BigUint;
use randomx::{RandomXCache, RandomXDataset, RandomXFlags, RandomXVM};
use smol::channel::Receiver;
use tracing::{debug, error};

use crate::{
    blockchain::{
        block_store::BlockDifficulty,
        header_store::{
            Header, HeaderHash,
            PowData::{DarkFi, Monero},
        },
        Blockchain, BlockchainOverlayPtr,
    },
    util::{ringbuffer::RingBuffer, time::Timestamp},
    validator::{utils::median, RandomXFactory},
    Error, Result,
};

// Note: We have combined some constants for better performance.
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
/// How many most recent blocks to use to verify new blocks' timestamp
const BLOCKCHAIN_TIMESTAMP_CHECK_WINDOW: usize = 60;
/// Time limit in the future of what blocks can be
const BLOCK_FUTURE_TIME_LIMIT: Timestamp = Timestamp::from_u64(60 * 60 * 2);
/// RandomX VM key changing height
pub const RANDOMX_KEY_CHANGING_HEIGHT: u32 = 2048;
/// RandomX VM key change delay
pub const RANDOMX_KEY_CHANGE_DELAY: u32 = 64;

/// This struct represents the information required by the PoW algorithm
#[derive(Clone)]
pub struct PoWModule {
    /// Genesis block timestamp
    pub genesis: Timestamp,
    /// Target block time, in seconds
    pub target: u32,
    /// Optional fixed difficulty
    pub fixed_difficulty: Option<BigUint>,
    /// Latest block timestamps ringbuffer
    pub timestamps: RingBuffer<Timestamp, BUF_SIZE>,
    /// Latest block cumulative difficulties ringbuffer
    pub difficulties: RingBuffer<BigUint, BUF_SIZE>,
    /// Total blocks cumulative difficulty
    /// Note: we keep this as a struct field for faster
    /// access(optimization), since its always same as
    /// difficulties buffer last.
    pub cumulative_difficulty: BigUint,
    /// Native PoW RandomX VMs current and next keys pair
    pub darkfi_rx_keys: (HeaderHash, HeaderHash),
    /// RandomXFactory for native PoW (Arc from parent)
    pub darkfi_rx_factory: RandomXFactory,
    /// RandomXFactory for Monero PoW (Arc from parent)
    pub monero_rx_factory: RandomXFactory,
}

impl PoWModule {
    // Initialize a new `PowModule` for provided target over provided `Blockchain`.
    // Optionally, a fixed difficulty can be set and/or initialize before some height.
    pub fn new(
        blockchain: Blockchain,
        target: u32,
        fixed_difficulty: Option<BigUint>,
        height: Option<u32>,
    ) -> Result<Self> {
        // Retrieve genesis block timestamp
        let genesis = blockchain.genesis_block()?.header.timestamp;

        // Retrieving last BUF_SIZE difficulties from blockchain to build the buffers
        let mut timestamps = RingBuffer::<Timestamp, BUF_SIZE>::new();
        let mut difficulties = RingBuffer::<BigUint, BUF_SIZE>::new();
        let mut cumulative_difficulty = BigUint::zero();
        let last_n = match height {
            Some(h) => blockchain.blocks.get_difficulties_before(h, BUF_SIZE)?,
            None => blockchain.blocks.get_last_n_difficulties(BUF_SIZE)?,
        };
        for difficulty in last_n {
            timestamps.push(difficulty.timestamp);
            difficulties.push(difficulty.cumulative_difficulty.clone());
            cumulative_difficulty = difficulty.cumulative_difficulty;
        }

        // If a fixed difficulty has been set, assert its greater than zero
        if let Some(diff) = &fixed_difficulty {
            assert!(diff > &BigUint::zero());
        }

        // Retrieve current and next native PoW RandomX VM keys pair,
        // and generate the RandomX factories.
        let darkfi_rx_keys = blockchain.get_randomx_vm_keys(
            &RANDOMX_KEY_CHANGING_HEIGHT,
            &RANDOMX_KEY_CHANGE_DELAY,
            height,
        )?;
        let darkfi_rx_factory = RandomXFactory::default();
        let monero_rx_factory = RandomXFactory::default();

        Ok(Self {
            genesis,
            target,
            fixed_difficulty,
            timestamps,
            difficulties,
            cumulative_difficulty,
            darkfi_rx_keys,
            darkfi_rx_factory,
            monero_rx_factory,
        })
    }

    /// Compute the next mining difficulty, based on current ring buffers.
    /// If ring buffers contain 2 or less items, difficulty 1 is returned.
    /// If a fixed difficulty has been set, this function will always
    /// return that after first 2 difficulties.
    pub fn next_difficulty(&self) -> Result<BigUint> {
        // Retrieve first DIFFICULTY_WINDOW timestamps from the ring buffer
        let mut timestamps: Vec<Timestamp> =
            self.timestamps.iter().take(DIFFICULTY_WINDOW).cloned().collect();

        // Check we have enough timestamps
        let length = timestamps.len();
        if length < 2 {
            return Ok(BigUint::one())
        }

        // If a fixed difficulty has been set, return that
        if let Some(diff) = &self.fixed_difficulty {
            return Ok(diff.clone())
        }

        // Sort the timestamps vector
        timestamps.sort_unstable();

        // Grab cutoff indexes
        let (cut_begin, cut_end) = self.cutoff(length)?;

        // Calculate total time span
        let cut_end = cut_end - 1;

        let mut time_span = timestamps[cut_end].checked_sub(timestamps[cut_begin])?;
        if time_span.inner() == 0 {
            time_span = 1.into();
        }

        // Calculate total work done during this time span
        let total_work = &self.difficulties[cut_end] - &self.difficulties[cut_begin];
        if total_work <= BigUint::zero() {
            return Err(Error::PoWTotalWorkIsZero)
        }

        // Compute next difficulty
        let next_difficulty =
            (total_work * self.target + time_span.inner() - BigUint::one()) / time_span.inner();

        Ok(next_difficulty)
    }

    /// Calculate cutoff indexes.
    /// If buffers have been filled, we return the
    /// already known indexes, for performance.
    fn cutoff(&self, length: usize) -> Result<(usize, usize)> {
        if length >= DIFFICULTY_WINDOW {
            return Ok((CUT_BEGIN, CUT_END))
        }

        let (cut_begin, cut_end) = if length <= RETAINED {
            (0, length)
        } else {
            let cut_begin = (length - RETAINED).div_ceil(2);
            (cut_begin, cut_begin + RETAINED)
        };
        // Sanity check
        if
        /* cut_begin < 0 || */
        cut_begin + 2 > cut_end || cut_end > length {
            return Err(Error::PoWCuttofCalculationError)
        }

        Ok((cut_begin, cut_end))
    }

    /// Compute the next mine target.
    pub fn next_mine_target(&self) -> Result<BigUint> {
        Ok(BigUint::from_bytes_le(&[0xFF; 32]) / &self.next_difficulty()?)
    }

    /// Compute the next mine target and difficulty.
    pub fn next_mine_target_and_difficulty(&self) -> Result<(BigUint, BigUint)> {
        let difficulty = self.next_difficulty()?;
        let mine_target = BigUint::from_bytes_le(&[0xFF; 32]) / &difficulty;
        Ok((mine_target, difficulty))
    }

    /// Verify provided difficulty corresponds to the next one.
    pub fn verify_difficulty(&self, difficulty: &BigUint) -> Result<bool> {
        Ok(difficulty == &self.next_difficulty()?)
    }

    /// Verify provided block timestamp is not far in the future and
    /// check its valid acorrding to current timestamps median.
    pub fn verify_current_timestamp(&self, timestamp: Timestamp) -> Result<bool> {
        if timestamp > Timestamp::current_time().checked_add(BLOCK_FUTURE_TIME_LIMIT)? {
            return Ok(false)
        }

        Ok(self.verify_timestamp_by_median(timestamp))
    }

    /// Verify provided block timestamp is valid and matches certain criteria.
    pub fn verify_timestamp_by_median(&self, timestamp: Timestamp) -> bool {
        // Check timestamp is after genesis one
        if timestamp <= self.genesis {
            return false
        }

        // If not enough blocks, no proper median yet, return true
        if self.timestamps.len() < BLOCKCHAIN_TIMESTAMP_CHECK_WINDOW {
            return true
        }

        // Make sure the timestamp is higher or equal to the median
        let timestamps = self
            .timestamps
            .iter()
            .rev()
            .take(BLOCKCHAIN_TIMESTAMP_CHECK_WINDOW)
            .map(|x| x.inner())
            .collect();

        timestamp >= median(timestamps).into()
    }

    /// Verify provided block timestamp and hash.
    pub fn verify_current_block(&self, header: &Header) -> Result<()> {
        // First we verify the block's timestamp
        if !self.verify_current_timestamp(header.timestamp)? {
            return Err(Error::PoWInvalidTimestamp)
        }

        // Then we verify the block's hash
        self.verify_block_hash(header)
    }

    /// Verify provided block hash is less than provided mine target.
    pub fn verify_block_target(&self, header: &Header, target: &BigUint) -> Result<BigUint> {
        let verifier_setup = Instant::now();

        // Grab verifier output hash based on block PoW data
        let (out_hash, verification_time) = match &header.pow_data {
            DarkFi => {
                // Check which VM key should be used.
                // We only use the next key when the next block is the
                // height changing one.
                let randomx_key = if header.height > RANDOMX_KEY_CHANGING_HEIGHT &&
                    header.height % RANDOMX_KEY_CHANGING_HEIGHT == RANDOMX_KEY_CHANGE_DELAY
                {
                    &self.darkfi_rx_keys.1
                } else {
                    &self.darkfi_rx_keys.0
                };

                let vm = self.darkfi_rx_factory.create(&randomx_key.inner()[..])?;

                debug!(
                    target: "validator::pow::verify_block_target",
                    "[VERIFIER] DarkFi PoW setup time: {:?}",
                    verifier_setup.elapsed(),
                );

                let verification_time = Instant::now();
                let out_hash = vm.calculate_hash(&header.to_block_hashing_blob())?;
                (BigUint::from_bytes_le(&out_hash), verification_time)
            }
            Monero(powdata) => {
                let vm = self.monero_rx_factory.create(powdata.randomx_key())?;

                debug!(
                    target: "validator::pow::verify_block_target",
                    "[VERIFIER] Monero PoW setup time: {:?}",
                    verifier_setup.elapsed(),
                );

                let verification_time = Instant::now();
                let out_hash = vm.calculate_hash(&powdata.to_block_hashing_blob())?;
                (BigUint::from_bytes_le(&out_hash), verification_time)
            }
        };
        debug!(target: "validator::pow::verify_block_target", "[VERIFIER] Verification time: {:?}", verification_time.elapsed());

        // Verify hash is less than the provided mine target
        if out_hash > *target {
            return Err(Error::PoWInvalidOutHash)
        }

        Ok(out_hash)
    }

    /// Verify provided block corresponds to next mine target.
    pub fn verify_block_hash(&self, header: &Header) -> Result<()> {
        // Grab the next mine target
        let target = self.next_mine_target()?;

        // Verify hash is less than the expected mine target
        let _ = self.verify_block_target(header, &target)?;
        Ok(())
    }

    /// Append provided header timestamp and difficulty to the ring
    /// buffers, and check if we need to rotate and/or create the next
    /// key RandomX VM in the native PoW factory.
    pub fn append(&mut self, header: &Header, difficulty: &BigUint) -> Result<()> {
        self.timestamps.push(header.timestamp);
        self.cumulative_difficulty += difficulty;
        self.difficulties.push(self.cumulative_difficulty.clone());

        if header.height < RANDOMX_KEY_CHANGING_HEIGHT {
            return Ok(())
        }

        // Check if need to set the new key
        if header.height.is_multiple_of(RANDOMX_KEY_CHANGING_HEIGHT) {
            let next_key = header.hash();
            let _ = self.darkfi_rx_factory.create(&next_key.inner()[..])?;
            self.darkfi_rx_keys.1 = next_key;
            return Ok(())
        }

        // Check if need to rotate keys
        if header.height % RANDOMX_KEY_CHANGING_HEIGHT == RANDOMX_KEY_CHANGE_DELAY {
            self.darkfi_rx_keys.0 = self.darkfi_rx_keys.1;
        }

        Ok(())
    }

    /// Append provided block difficulty to the ring buffers and insert
    /// it to provided overlay.
    pub fn append_difficulty(
        &mut self,
        overlay: &BlockchainOverlayPtr,
        header: &Header,
        difficulty: BlockDifficulty,
    ) -> Result<()> {
        self.append(header, &difficulty.difficulty)?;
        overlay.lock().unwrap().blocks.insert_difficulty(&[difficulty])
    }

    /// Mine provided block, based on next mine target.
    /// Note: this is used in tests not in actual mining.
    pub fn mine_block(
        &self,
        header: &mut Header,
        threads: usize,
        stop_signal: &Receiver<()>,
    ) -> Result<()> {
        // Grab the RandomX key to use.
        // We only use the next key when the next block is the
        // height changing one.
        let randomx_key = if header.height > RANDOMX_KEY_CHANGING_HEIGHT &&
            header.height % RANDOMX_KEY_CHANGING_HEIGHT == RANDOMX_KEY_CHANGE_DELAY
        {
            &self.darkfi_rx_keys.1
        } else {
            &self.darkfi_rx_keys.0
        };

        // Generate the RandomX VMs for the key
        let flags = get_mining_flags(false, false, false);
        let vms = generate_mining_vms(flags, randomx_key, threads, stop_signal)?;

        // Grab the next mine target
        let target = self.next_mine_target()?;

        // Mine the block
        mine_block(&vms, &target, header, stop_signal)
    }
}

impl std::fmt::Display for PoWModule {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "PoWModule:")?;
        write!(f, "\ttarget: {}", self.target)?;
        write!(f, "\ttimestamps: {:?}", self.timestamps)?;
        write!(f, "\tdifficulties: {:?}", self.difficulties)?;
        write!(f, "\tcumulative_difficulty: {}", self.cumulative_difficulty)
    }
}

/// Auxiliary function to define `RandomXFlags` used in mining.
///
/// Note: RandomX recommended flags will include `SSSE3` and `AVX2`
/// extensions if CPU supports them.
pub fn get_mining_flags(fast_mode: bool, large_pages: bool, secure: bool) -> RandomXFlags {
    let mut flags = RandomXFlags::get_recommended_flags();
    if fast_mode {
        flags |= RandomXFlags::FULLMEM;
    }
    if large_pages {
        flags |= RandomXFlags::LARGEPAGES;
    }
    if secure && flags.contains(RandomXFlags::JIT) {
        flags |= RandomXFlags::SECURE;
    }
    flags
}

/// Auxiliary function to initialize a `RandomXDataset` using all
/// available threads.
fn init_dataset(
    flags: RandomXFlags,
    input: &HeaderHash,
    stop_signal: &Receiver<()>,
) -> Result<RandomXDataset> {
    // Allocate cache and dataset
    let cache = RandomXCache::new(flags, &input.inner()[..])?;
    let dataset_item_count = RandomXDataset::count()?;
    let dataset = RandomXDataset::new(flags, cache, dataset_item_count)?;

    // Multithreaded dataset init using all available threads
    let threads = thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    debug!(target: "validator::pow::init_dataset", "[MINER] Initializing RandomX dataset using {threads} threads...");
    let mut handles = Vec::with_capacity(threads);
    let threads_u32 = threads as u32;
    let per_thread = dataset_item_count / threads_u32;
    let remainder = dataset_item_count % threads_u32;
    for t in 0..threads_u32 {
        // Check if stop signal is received
        if stop_signal.is_full() {
            debug!(target: "validator::pow::init_dataset", "[MINER] Stop signal received, threads creation loop exiting");
            break
        }

        let dataset = dataset.clone();
        let start_item = t * per_thread;
        let count = per_thread + if t == threads_u32 - 1 { remainder } else { 0 };
        handles.push(thread::spawn(move || {
            dataset.init(start_item, count);
        }));
    }

    // Wait for threads to finish setup
    for handle in handles {
        let _ = handle.join();
    }

    // Check if stop signal is received
    if stop_signal.is_full() {
        debug!(target: "validator::pow::init_dataset", "[MINER] Stop signal received, exiting");
        return Err(Error::MinerTaskStopped);
    }
    Ok(dataset)
}

/// Auxiliary function to generate mining VMs for provided RandomX key.
pub fn generate_mining_vms(
    flags: RandomXFlags,
    input: &HeaderHash,
    threads: usize,
    stop_signal: &Receiver<()>,
) -> Result<Vec<Arc<RandomXVM>>> {
    debug!(target: "validator::pow::generate_mining_vms", "[MINER] Initializing RandomX cache and dataset...");
    debug!(target: "validator::pow::generate_mining_vms", "[MINER] PoW input: {input}");
    let setup_start = Instant::now();
    // Check if fast mode is enabled
    let (cache, dataset) = if flags.contains(RandomXFlags::FULLMEM) {
        // Initialize dataset
        let dataset = init_dataset(flags, input, stop_signal)?;
        (None, Some(dataset))
    } else {
        // Initialize cache for light mode
        let cache = RandomXCache::new(flags, &input.inner()[..])?;
        (Some(cache), None)
    };
    debug!(target: "validator::pow::generate_mining_vms", "[MINER] Initialized RandomX cache and dataset: {:?}", setup_start.elapsed());

    // Single thread mining VM
    if threads == 1 {
        debug!(target: "validator::pow::generate_mining_vms", "[MINER] Initializing RandomX VM...");
        let vm_start = Instant::now();
        let vm = Arc::new(RandomXVM::new(flags, cache, dataset)?);
        debug!(target: "validator::pow::generate_mining_vms", "[MINER] Initialized RandomX VM in {:?}", vm_start.elapsed());
        debug!(target: "validator::pow::generate_mining_vms", "[MINER] Setup time: {:?}", setup_start.elapsed());
        return Ok(vec![vm])
    }

    // Multi thread mining VMs
    debug!(target: "validator::pow::generate_mining_vms", "[MINER] Initializing {threads} RandomX VMs...");
    let mut vms = Vec::with_capacity(threads);
    let threads_u32 = threads as u32;
    for t in 0..threads_u32 {
        // Check if stop signal is received
        if stop_signal.is_full() {
            debug!(target: "validator::pow::generate_mining_vms", "[MINER] Stop signal received, exiting");
            return Err(Error::MinerTaskStopped);
        }

        debug!(target: "validator::pow::generate_mining_vms", "[MINER] Initializing RandomX VM #{t}...");
        let vm_start = Instant::now();
        vms.push(Arc::new(RandomXVM::new(flags, cache.clone(), dataset.clone())?));
        debug!(target: "validator::pow::generate_mining_vms", "[MINER] Initialized RandomX VM #{t} in {:?}", vm_start.elapsed());
    }
    debug!(target: "validator::pow::generate_mining_vms", "[MINER] Setup time: {:?}", setup_start.elapsed());
    Ok(vms)
}

/// Mine provided header, based on provided PoW module next mine target,
/// using provided RandomX VMs setup.
pub fn mine_block(
    vms: &[Arc<RandomXVM>],
    target: &BigUint,
    header: &mut Header,
    stop_signal: &Receiver<()>,
) -> Result<()> {
    debug!(target: "validator::pow::mine_block", "[MINER] Mine target: 0x{target:064x}");

    // Check VMs were provided
    if vms.is_empty() {
        error!(target: "validator::pow::mine_block", "[MINER] No VMs were provided!");
        return Err(Error::MinerTaskStopped)
    }

    debug!(target: "validator::pow::randomx_vms_mine", "[MINER] Initializing mining threads...");
    let mut handles = Vec::with_capacity(vms.len());
    let atomic_nonce = Arc::new(AtomicU32::new(0));
    let found_header = Arc::new(AtomicBool::new(false));
    let found_nonce = Arc::new(AtomicU32::new(0));
    let threads = vms.len() as u32;
    let mining_start = Instant::now();
    for t in 0..threads {
        // Check if stop signal is received
        if stop_signal.is_full() {
            debug!(target: "validator::pow::randomx_vms_mine", "[MINER] Stop signal received, threads creation loop exiting");
            break
        }

        if found_header.load(Ordering::SeqCst) {
            debug!(target: "validator::pow::randomx_vms_mine", "[MINER] Block header found, threads creation loop exiting");
            break
        }

        let vm = vms[t as usize].clone();
        let target = target.clone();
        let mut thread_header = header.clone();
        let atomic_nonce = atomic_nonce.clone();
        let found_header = found_header.clone();
        let found_nonce = found_nonce.clone();
        let stop_signal = stop_signal.clone();

        handles.push(thread::spawn(move || {
            let mut last_nonce = atomic_nonce.fetch_add(1, Ordering::SeqCst);
            thread_header.nonce = last_nonce;
            if let Err(e) = vm.calculate_hash_first(thread_header.hash().inner()) {
                error!(target: "validator::pow::randomx_vms_mine", "[MINER] Calculating hash in thread #{t} failed: {e}");
                return
            };
            loop {
                // Check if stop signal was received
                if stop_signal.is_full() {
                    debug!(target: "validator::pow::randomx_vms_mine", "[MINER] Stop signal received, thread #{t} exiting");
                    break
                }

                if found_header.load(Ordering::SeqCst) {
                    debug!(target: "validator::pow::randomx_vms_mine", "[MINER] Block header found, thread #{t} exiting");
                    break;
                }

                thread_header.nonce = atomic_nonce.fetch_add(1, Ordering::SeqCst);
                let out_hash = match vm.calculate_hash_next(thread_header.hash().inner()) {
                    Ok(hash) => hash,
                    Err(e) => {
                        error!(target: "validator::pow::randomx_vms_mine", "[MINER] Calculating hash in thread #{t} failed: {e}");
                        break
                    }
                };
                let out_hash = BigUint::from_bytes_le(&out_hash);
                if out_hash <= target {
                    found_header.store(true, Ordering::SeqCst);
                    thread_header.nonce = last_nonce; // Since out hash refers to previous run nonce
                    found_nonce.store(thread_header.nonce, Ordering::SeqCst);
                    debug!(target: "validator::pow::randomx_vms_mine", "[MINER] Thread #{t} found block header using nonce {}",
                        thread_header.nonce
                    );
                    debug!(target: "validator::pow::randomx_vms_mine", "[MINER] Block header hash {}", thread_header.hash());
                    debug!(target: "validator::pow::randomx_vms_mine", "[MINER] RandomX output: 0x{out_hash:064x}");
                    break;
                }
                last_nonce = thread_header.nonce;
            }
        }));
    }

    // Wait for threads to finish mining
    for handle in handles {
        let _ = handle.join();
    }

    // Check if stop signal is received
    if stop_signal.is_full() {
        debug!(target: "validator::pow::randomx_vms_mine", "[MINER] Stop signal received, exiting");
        return Err(Error::MinerTaskStopped);
    }

    debug!(target: "validator::pow::randomx_vms_mine", "[MINER] Completed mining in {:?}", mining_start.elapsed());
    header.nonce = found_nonce.load(Ordering::SeqCst);
    debug!(target: "validator::pow::randomx_vms_mine", "[MINER] Mined header: {header:?}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        io::{BufRead, Cursor},
        process::Command,
    };

    use darkfi_sdk::num_traits::Num;
    use num_bigint::BigUint;
    use sled_overlay::sled;

    use crate::{
        blockchain::{header_store::Header, BlockInfo, Blockchain},
        Result,
    };

    use super::PoWModule;

    const DEFAULT_TEST_THREADS: usize = 2;
    const DEFAULT_TEST_DIFFICULTY_TARGET: u32 = 120;

    #[test]
    fn test_wide_difficulty() -> Result<()> {
        let sled_db = sled::Config::new().temporary(true).open()?;
        let blockchain = Blockchain::new(&sled_db)?;
        let genesis_block = BlockInfo::default();
        blockchain.add_block(&genesis_block)?;

        let mut module = PoWModule::new(blockchain, DEFAULT_TEST_DIFFICULTY_TARGET, None, None)?;

        let output = Command::new("./script/monero_gen_wide_data.py").output().unwrap();
        let reader = Cursor::new(output.stdout);

        let mut previous = genesis_block.header;
        for (n, line) in reader.lines().enumerate() {
            let line = line.unwrap();
            let parts: Vec<String> = line.split(' ').map(|x| x.to_string()).collect();
            assert!(parts.len() == 2);

            let header = Header::new(
                previous.hash(),
                previous.height + 1,
                0,
                parts[0].parse::<u64>().unwrap().into(),
            );
            let difficulty = BigUint::from_str_radix(&parts[1], 10).unwrap();

            let res = module.next_difficulty()?;

            if res != difficulty {
                eprintln!("Wrong wide difficulty for block {n}");
                eprintln!("Expected: {difficulty}");
                eprintln!("Found: {res}");
                assert!(res == difficulty);
            }

            module.append(&header, &difficulty)?;
            previous = header;
        }

        Ok(())
    }

    #[test]
    fn test_miner_correctness() -> Result<()> {
        // Default setup
        let sled_db = sled::Config::new().temporary(true).open()?;
        let blockchain = Blockchain::new(&sled_db)?;
        let mut genesis_block = BlockInfo::default();
        genesis_block.header.timestamp = 0.into();
        blockchain.add_block(&genesis_block)?;

        let module = PoWModule::new(blockchain, DEFAULT_TEST_DIFFICULTY_TARGET, None, None)?;

        let (_, recvr) = smol::channel::bounded(1);

        // Mine next block
        let mut next_block = BlockInfo::default();
        next_block.header.previous = genesis_block.hash();
        module.mine_block(&mut next_block.header, DEFAULT_TEST_THREADS, &recvr)?;

        // Verify it
        module.verify_current_block(&next_block.header)?;

        Ok(())
    }
}
