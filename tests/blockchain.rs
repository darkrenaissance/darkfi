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
    blockchain::{BlockInfo, Blockchain, BlockchainOverlay, Header},
    validator::{
        pid::slot_pid_output,
        pow::PoWModule,
        validation::{validate_block, validate_blockchain},
    },
    Error, Result,
};
use darkfi_sdk::{
    blockchain::{expected_reward, PidOutput, PreviousSlot, Slot, POS_START},
    pasta::{group::ff::Field, pallas},
};

const POW_THREADS: usize = 1;
const POW_TARGET: usize = 10;

struct Node {
    blockchain: Blockchain,
    module: PoWModule,
}

impl Node {
    fn new() -> Result<Self> {
        let blockchain = Blockchain::new(&sled::Config::new().temporary(true).open()?)?;
        let module = PoWModule::new(blockchain.clone(), POW_THREADS, POW_TARGET)?;
        Ok(Self { blockchain, module })
    }
}

struct Harness {
    pub alice: Node,
    pub bob: Node,
}

impl Harness {
    fn new() -> Result<Self> {
        let alice = Node::new()?;
        let bob = Node::new()?;
        Ok(Self { alice, bob })
    }

    fn is_empty(&self) {
        assert!(self.alice.blockchain.is_empty());
        assert!(self.bob.blockchain.is_empty());
    }

    fn validate_chains(&self) -> Result<()> {
        validate_blockchain(&self.alice.blockchain, POW_THREADS, POW_TARGET)?;
        validate_blockchain(&self.bob.blockchain, POW_THREADS, POW_TARGET)?;

        assert_eq!(self.alice.blockchain.len(), self.bob.blockchain.len());

        Ok(())
    }

    fn generate_next_pos_block(&self, previous: &BlockInfo) -> Result<BlockInfo> {
        let previous_hash = previous.hash()?;

        // Generate slot
        let previous_slot = previous.slots.last().unwrap();
        let id = if previous_slot.id < POS_START { POS_START } else { previous_slot.id + 1 };
        let producers = 1;
        let previous_slot_info = PreviousSlot::new(
            producers,
            vec![previous_hash],
            vec![previous.header.previous],
            previous_slot.pid.error,
        );
        let (f, error, sigma1, sigma2) = slot_pid_output(previous_slot, producers);
        let pid = PidOutput::new(f, error, sigma1, sigma2);
        let total_tokens = previous_slot.total_tokens + previous_slot.reward;
        let reward = expected_reward(id);
        let slot = Slot::new(id, previous_slot_info, pid, pallas::Base::ZERO, total_tokens, reward);

        // We increment timestamp so we don't have to use sleep
        let mut timestamp = previous.header.timestamp;
        timestamp.add(1);

        // Generate header
        let header =
            Header::new(previous_hash, previous.header.epoch, id, timestamp, previous.header.nonce);

        // Generate the block
        let mut block = BlockInfo::new_empty(header, vec![slot]);

        // Add transactions to the block
        block.append_txs(previous.txs.clone())?;

        // Attach signature
        block.signature = previous.signature;

        Ok(block)
    }

    fn add_pos_blocks(&mut self, blocks: &[BlockInfo]) -> Result<()> {
        Self::add_pos_blocks_to_chain(&mut self.alice, blocks)?;
        Self::add_pos_blocks_to_chain(&mut self.bob, blocks)?;

        Ok(())
    }

    // This is what the validator will execute when it receives a block.
    fn add_pos_blocks_to_chain(node: &mut Node, blocks: &[BlockInfo]) -> Result<()> {
        // Create overlay
        let blockchain_overlay = BlockchainOverlay::new(&node.blockchain)?;
        let lock = blockchain_overlay.lock().unwrap();

        // When we insert genesis, chain is empty
        let mut previous = if !lock.is_empty()? { Some(lock.last_block()?) } else { None };

        // Validate and insert each block
        for block in blocks {
            // Check if block already exists
            if lock.has_block(block)? {
                return Err(Error::BlockAlreadyExists(block.hash()?.to_string()))
            }

            // This will be true for every insert, apart from genesis
            if let Some(p) = previous {
                // Retrieve expected reward
                let expected_reward = expected_reward(block.header.height);

                // Validate block
                validate_block(block, &p, expected_reward, &node.module)?;

                // Update PoW module
                if block.header.version == 1 {
                    node.module.append(block.header.timestamp.0, &node.module.next_difficulty()?);
                }
            }

            // Insert block
            lock.add_block(block)?;

            // Use last inserted block as next iteration previous
            previous = Some(block.clone());
        }

        // Write overlay
        lock.overlay.lock().unwrap().apply()?;

        Ok(())
    }
}

#[test]
fn blockchain_add_pos_blocks() -> Result<()> {
    smol::block_on(async {
        // Initialize harness
        let mut th = Harness::new()?;

        // Check that nothing exists
        th.is_empty();

        // We generate some pos blocks
        let mut blocks = vec![];

        let genesis_block = BlockInfo::default();
        blocks.push(genesis_block.clone());

        let block = th.generate_next_pos_block(&genesis_block)?;
        blocks.push(block.clone());

        let block = th.generate_next_pos_block(&block)?;
        blocks.push(block.clone());

        let block = th.generate_next_pos_block(&block)?;
        blocks.push(block.clone());

        th.add_pos_blocks(&blocks)?;

        // Validate chains
        th.validate_chains()?;

        // Thanks for reading
        Ok(())
    })
}
