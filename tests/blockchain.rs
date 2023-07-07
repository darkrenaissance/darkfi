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
    Error, Result,
};
use darkfi_sdk::{
    blockchain::Slot,
    pasta::{group::ff::Field, pallas},
};

struct Harness {
    pub alice: Blockchain,
    pub bob: Blockchain,
}

impl Harness {
    fn new() -> Result<Self> {
        let alice = Blockchain::new(&sled::Config::new().temporary(true).open()?)?;
        let bob = Blockchain::new(&sled::Config::new().temporary(true).open()?)?;
        Ok(Self { alice, bob })
    }

    fn is_empty(&self) {
        assert!(self.alice.is_empty());
        assert!(self.bob.is_empty());
    }

    fn validate_chains(&self) -> Result<()> {
        self.alice.validate()?;
        self.bob.validate()?;

        assert_eq!(self.alice.len(), self.bob.len());

        Ok(())
    }

    fn generate_next_block(&self, previous: &BlockInfo) -> BlockInfo {
        let previous_hash = previous.blockhash();
        // We increment timestamp so we don't have to use sleep
        let mut timestamp = previous.header.timestamp;
        timestamp.add(1);
        let header = Header::new(
            previous_hash,
            previous.header.epoch,
            previous.header.slot + 1,
            timestamp,
            previous.header.root.clone(),
        );
        let slot = Slot::new(
            previous.header.slot + 1,
            pallas::Base::ZERO,
            vec![previous_hash],
            vec![previous.header.previous.clone()],
            0.0,
            0.0,
            0.0,
            0,
            0,
            pallas::Base::ZERO,
            pallas::Base::ZERO,
        );
        BlockInfo::new(header, vec![], previous.producer.clone(), vec![slot])
    }

    fn add_blocks(&self, blocks: &[BlockInfo]) -> Result<()> {
        self.add_blocks_to_chain(&self.alice, blocks)?;
        self.add_blocks_to_chain(&self.bob, blocks)?;

        Ok(())
    }

    // This is what the validator will execute when it receives a block.
    fn add_blocks_to_chain(&self, blockchain: &Blockchain, blocks: &[BlockInfo]) -> Result<()> {
        // Create overlay
        let blockchain_overlay = BlockchainOverlay::new(&blockchain)?;
        let lock = blockchain_overlay.lock().unwrap();

        // When we insert genesis, chain is empty
        let mut previous = if !lock.is_empty()? { Some(lock.last_block()?) } else { None };

        // Validate and insert each block
        for block in blocks {
            // Check if block already exists
            if lock.has_block(block)? {
                return Err(Error::BlockAlreadyExists(block.blockhash().to_string()))
            }

            // This will be true for every insert, apart from genesis
            if let Some(p) = previous {
                block.validate(&p)?;
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

#[async_std::test]
async fn blockchain_add_blocks() -> Result<()> {
    // Initialize harness
    let th = Harness::new()?;

    // Check that nothing exists
    th.is_empty();

    // We generate some blocks
    let mut blocks = vec![];

    let genesis_block = BlockInfo::default();
    blocks.push(genesis_block.clone());

    let block = th.generate_next_block(&genesis_block);
    blocks.push(block.clone());

    let block = th.generate_next_block(&block);
    blocks.push(block.clone());

    let block = th.generate_next_block(&block);
    blocks.push(block.clone());

    th.add_blocks(&blocks)?;

    // Validate chains
    th.validate_chains()?;

    // Thanks for reading
    Ok(())
}
