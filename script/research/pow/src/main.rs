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

use darkfi::{util::time::Timestamp, Result};
use darkfi_sdk::{
    crypto::MerkleTree,
    pasta::{group::ff::FromUniformBytes, pallas},
};
use darkfi_serial::{async_trait, Encodable, SerialDecodable, SerialEncodable};
use rand::{rngs::OsRng, Rng};
use randomx::{RandomXCache, RandomXDataset, RandomXFlags, RandomXVM};

/// Constant genesis block string used as the previous block hash
const GENESIS: &[u8] = b"genesis";
/// The target mining difficulty
const DIFFICULTY: usize = 1;
/// The output length of the BLAKE2b hash in bytes
const HASH_LEN: usize = 32;
/// The amount of blocks the main loop will mine until the program exits
const N_BLOCKS: usize = 5;

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

fn main() -> Result<()> {
    // Construct the genesis block
    let genesis_hash =
        blake2b_simd::Params::new().hash_length(HASH_LEN).to_state().update(GENESIS).finalize();

    let mut genesis_block = Block {
        header: BlockHeader {
            nonce: 0,
            previous_hash: genesis_hash,
            timestamp: Timestamp(1693213806),
            txtree: MerkleTree::new(100),
        },
        transactions: vec![],
    };

    let genesis_tx = Transaction(vec![1, 3, 3, 7]);
    genesis_block.insert_tx(&genesis_tx)?;

    let mut cur_block = genesis_block;
    for i in 0..N_BLOCKS {
        // Get the PoW input. The key changes with every mined block.
        let pow_input = cur_block.hash()?;
        println!("[{}] [MINER] PoW Input: {}", i, pow_input.to_hex());

        let miner_setup = Instant::now();
        let flags = RandomXFlags::default() | RandomXFlags::FULLMEM;
        println!("[{}] [MINER] Initializing RandomX dataset...", i);
        let dataset = Arc::new(RandomXDataset::new(flags, pow_input.as_bytes(), 1).unwrap());

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
        println!("[{}] [MINER] Setup time: {:?}", i, miner_setup.elapsed());

        // Multithreaded mining setup
        let mining_time = Instant::now();
        // Let's use 4 threads
        const NUM_THREADS: u32 = 4;
        let mut handles = vec![];
        let found_block = Arc::new(AtomicBool::new(false));
        let found_nonce = Arc::new(AtomicU32::new(0));
        for t in 0..NUM_THREADS {
            let mut block = miner_block.clone();
            let found_block = Arc::clone(&found_block);
            let found_nonce = Arc::clone(&found_nonce);
            let dataset = dataset.clone();
            handles.push(thread::spawn(move || {
                println!("[{}] [MINER] Initializing RandomX VM #{}...", i, t);
                block.header.nonce = t;
                let vm = RandomXVM::new_fast(flags, &dataset).unwrap();
                loop {
                    if found_block.load(Ordering::SeqCst) {
                        println!("[{}] [MINER] Block was found, thread #{} exiting", i, t);
                        break
                    }

                    let out_hash = vm.hash(block.hash().unwrap().as_bytes());
                    let mut success = true;
                    for idx in 0..DIFFICULTY {
                        if out_hash[idx] != 0x00 {
                            success = false;
                        }
                    }

                    if success {
                        found_block.store(true, Ordering::SeqCst);
                        found_nonce.store(block.header.nonce, Ordering::SeqCst);
                        println!(
                            "[{}] [MINER] Thread #{} found block using nonce {}",
                            i, t, block.header.nonce
                        );
                        println!("[{}] [MINER] Block hash {}", i, block.hash().unwrap().to_hex(),);
                        println!("[{}] [MINER] RandomX hash bytes: {:?}", i, out_hash);
                        break
                    }

                    // This means thread 0 will use nonces, 0, 4, 8, ...
                    // and thread 1 will use nonces, 1, 5, 9, ...
                    block.header.nonce += NUM_THREADS;
                }
            }))
        }

        // Melt the CPU
        for handle in handles {
            let _ = handle.join();
        }
        println!("[{}] [MINER] Mining time: {:?}", i, mining_time.elapsed());

        // Set the valid mined nonce in the block that's broadcasted
        miner_block.header.nonce = found_nonce.load(Ordering::SeqCst);

        // Verify
        let verifier_setup = Instant::now();
        let flags = RandomXFlags::default();
        let cache = RandomXCache::new(flags, pow_input.as_bytes()).unwrap();
        let vm = RandomXVM::new(flags, &cache).unwrap();
        println!("[{}] [VERIFIER] Setup time: {:?}", i, verifier_setup.elapsed());

        let verification_time = Instant::now();
        let out_hash = vm.hash(miner_block.hash()?.as_bytes());
        for idx in 0..DIFFICULTY {
            assert!(out_hash[idx] == 0x00);
        }
        println!("[{}] [VERIFIER] Verification time: {:?}", i, verification_time.elapsed());

        // The new block appends to the blockchain
        cur_block = miner_block;
    }

    Ok(())
}
