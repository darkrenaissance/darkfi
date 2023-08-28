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

use std::{sync::Arc, time::Instant};

use darkfi::{util::time::Timestamp, Result};
use darkfi_sdk::{
    crypto::MerkleTree,
    pasta::{group::ff::FromUniformBytes, pallas},
};
use darkfi_serial::{async_trait, Encodable, SerialDecodable, SerialEncodable};
use randomx::{RandomXCache, RandomXDataset, RandomXFlags, RandomXVM};

const GENESIS: &[u8] = b"genesis";
const DIFFICULTY: usize = 1;
const HASH_LEN: usize = 32;

#[derive(SerialEncodable, SerialDecodable)]
struct Transaction(Vec<u8>);

impl Transaction {
    fn hash(&self) -> Result<blake2b_simd::Hash> {
        let mut hasher = blake2b_simd::Params::new().hash_length(HASH_LEN).to_state();
        self.encode(&mut hasher)?;
        Ok(hasher.finalize())
    }
}

#[derive(SerialEncodable, SerialDecodable)]
struct BlockHeader {
    nonce: u32,
    previous_hash: blake2b_simd::Hash,
    timestamp: Timestamp,
    txtree: MerkleTree,
}

#[derive(SerialEncodable, SerialDecodable)]
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
            timestamp: Timestamp::current_time(),
            txtree: MerkleTree::new(100),
        },
        transactions: vec![],
    };

    let genesis_tx = Transaction(vec![1, 3, 3, 7]);
    genesis_block.insert_tx(&genesis_tx)?;

    // Get initial PoW input
    let pow_input = genesis_block.hash()?;

    // This is single-threaded mining, but check darkrenaissance/RandomX/examples/
    // for multi-threaded ops.
    let miner_setup = Instant::now();
    let flags = RandomXFlags::default() | RandomXFlags::FULLMEM;
    let dataset = Arc::new(RandomXDataset::new(flags, pow_input.as_bytes(), 1).unwrap());
    let vm = RandomXVM::new_fast(flags, &dataset).unwrap();

    // The miner creates a block
    let mut miner_block = Block {
        header: BlockHeader {
            nonce: 0,
            previous_hash: genesis_block.hash()?,
            timestamp: Timestamp::current_time(),
            txtree: MerkleTree::new(100),
        },
        transactions: vec![],
    };
    let tx0 = Transaction(vec![0, 3, 1, 2]);
    let tx1 = Transaction(vec![1, 2, 1, 0]);
    miner_block.insert_tx(&tx0)?;
    miner_block.insert_tx(&tx1)?;
    println!("Miner setup time: {:?}", miner_setup.elapsed());

    // Melt the CPU
    let mining_time = Instant::now();
    loop {
        let out_hash = vm.hash(miner_block.hash()?.as_bytes());
        let mut success = true;
        for i in 0..DIFFICULTY {
            if out_hash[i] != 0x00 {
                success = false;
            }
        }

        if success {
            break
        }

        miner_block.header.nonce += 1;
    }
    println!("Mining time: {:?}", mining_time.elapsed());

    // Verify
    let verifier_setup = Instant::now();
    let flags = RandomXFlags::default();
    let cache = RandomXCache::new(flags, pow_input.as_bytes()).unwrap();
    let vm = RandomXVM::new(flags, &cache).unwrap();
    println!("Verifier setup time: {:?}", verifier_setup.elapsed());

    let verification_time = Instant::now();
    let out_hash = vm.hash(miner_block.hash()?.as_bytes());
    for i in 0..DIFFICULTY {
        assert!(out_hash[i] == 0x00);
    }
    println!("Verification time: {:?}", verification_time.elapsed());

    Ok(())
}
