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
    blockchain::{BlockInfo, Header},
    util::time::TimeKeeper,
    validator::{Validator, ValidatorConfig},
    Result,
};
use darkfi_contract_test_harness::vks;
use darkfi_sdk::{
    blockchain::Slot,
    pasta::{group::ff::Field, pallas},
};

use crate::Darkfid;

pub struct Harness {
    pub alice: Darkfid,
    pub bob: Darkfid,
}

impl Harness {
    pub async fn new(testing_node: bool) -> Result<Self> {
        // Generate default genesis block
        let genesis_block = BlockInfo::default();

        // Generate validators configuration
        // NOTE: we are not using consensus constants here so we
        // don't get circular dependencies.
        let time_keeper = TimeKeeper::new(genesis_block.header.timestamp, 10, 90, 0);
        let config = ValidatorConfig::new(time_keeper, genesis_block, vec![], testing_node);

        // Generate validators using pregenerated vks
        let sled_db = sled::Config::new().temporary(true).open()?;
        vks::inject(&sled_db)?;
        let validator = Validator::new(&sled_db, config.clone()).await?;
        let alice = Darkfid::new(validator).await;
        let sled_db = sled::Config::new().temporary(true).open()?;
        vks::inject(&sled_db)?;
        let validator = Validator::new(&sled_db, config.clone()).await?;
        let bob = Darkfid::new(validator).await;

        Ok(Self { alice, bob })
    }

    pub async fn validate_chains(&self) -> Result<()> {
        let alice = &self.alice._validator.read().await;
        let bob = &self.bob._validator.read().await;

        alice.validate_blockchain().await?;
        bob.validate_blockchain().await?;

        assert_eq!(alice.blockchain.len(), bob.blockchain.len());

        Ok(())
    }

    pub async fn add_blocks(&self, blocks: &[BlockInfo]) -> Result<()> {
        let alice = &self.alice._validator.read().await;
        let bob = &self.bob._validator.read().await;

        alice.add_blocks(blocks).await?;
        bob.add_blocks(blocks).await?;

        Ok(())
    }

    pub async fn generate_next_block(&self) -> Result<BlockInfo> {
        // Retrieve last block
        let previous = self.alice._validator.read().await.blockchain.last_block()?;
        let previous_hash = previous.blockhash();

        // We increment timestamp so we don't have to use sleep
        let mut timestamp = previous.header.timestamp;
        timestamp.add(1);

        // Generate header
        let header = Header::new(
            previous_hash,
            previous.header.epoch,
            previous.header.slot + 1,
            timestamp,
            previous.header.root.clone(),
        );

        // Generate slot
        let slot = Slot::new(
            previous.header.slot + 1,
            pallas::Base::ZERO,
            vec![previous_hash],
            vec![previous.header.previous.clone()],
            pallas::Base::ZERO,
            pallas::Base::ZERO,
        );

        // Generate block
        let block = BlockInfo::new(header, vec![], previous.producer.clone(), vec![slot]);

        Ok(block)
    }
}
