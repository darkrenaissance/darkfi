/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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
    str::FromStr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    thread,
    time::Instant,
};

use tracing::{error, info};

use darkfi::{
    blockchain::{Header, HeaderHash},
    util::time::Timestamp,
    validator::pow::{generate_mining_vms, get_mining_flags},
    Result,
};

/// Performs provided number of nonces simulating mining for provided
/// threads count to determine system hashrate.
pub fn benchmark(
    fast_mode: bool,
    large_pages: bool,
    secure: bool,
    threads: usize,
    nonces: u64,
) -> Result<()> {
    // Check provided params are valid
    if threads == 0 {
        error!(target: "minerd::benchmark", "No threads were configured!");
        return Ok(())
    }

    if nonces == 0 {
        error!(target: "minerd::benchmark", "No number of nonces was configured!");
        return Ok(())
    }
    info!(target: "minerd::benchmark", "Starting DarkFi hashrate benchmark for {threads} threads and {nonces} nonces");

    // Setup VMs using a dummy key for reproducible results
    let key =
        HeaderHash::from_str("c09967802bab1a95a4c434f18beb5a79e68ec7c75b252eb47e56516f32db8ce1")?;
    info!(target: "minerd::benchmark", "Initializing {threads} VMs for key: {key}");
    let (_, recvr) = smol::channel::bounded(1);
    let flags = get_mining_flags(fast_mode, large_pages, secure);
    let vms = generate_mining_vms(flags, &key, threads, &recvr)?;

    // Use a dummy header to mine for reproducible results
    let header = Header::new(key, 1, Timestamp::from_u64(1765378623), 0);

    // Start mining
    info!(target: "minerd::benchmark", "Starting mining threads...");
    let mut handles = Vec::with_capacity(vms.len());
    let atomic_nonce = Arc::new(AtomicU64::new(0));
    let threads = vms.len() as u64;
    let mining_start = Instant::now();
    for t in 0..threads {
        let vm = vms[t as usize].clone();
        let mut thread_header = header.clone();
        let atomic_nonce = atomic_nonce.clone();

        handles.push(thread::spawn(move || {
            thread_header.nonce = atomic_nonce.fetch_add(1, Ordering::SeqCst);
            vm.calculate_hash_first(thread_header.hash().inner()).unwrap();
            while thread_header.nonce < nonces {
                thread_header.nonce = atomic_nonce.fetch_add(1, Ordering::SeqCst);
                let _ = vm.calculate_hash_next(thread_header.hash().inner()).unwrap();
            }
        }));
    }

    // Wait for threads to finish mining
    for handle in handles {
        let _ = handle.join();
    }

    // Print total results
    let elapsed = mining_start.elapsed();
    let hashrate = if elapsed.as_secs_f64() == 0.0 {
        nonces as f64 * 1000.0 / elapsed.as_millis() as f64
    } else {
        nonces as f64 / elapsed.as_secs_f64()
    };
    info!(target: "minerd::benchmark", "Threads completed {nonces} nonces in {elapsed:?} with hashrate: {hashrate:.2} h/s");
    Ok(())
}
