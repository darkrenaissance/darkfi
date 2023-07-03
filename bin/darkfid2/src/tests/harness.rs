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
    blockchain::BlockInfo,
    util::time::TimeKeeper,
    validator::{Validator, ValidatorConfig},
    Result,
};
use darkfi_contract_test_harness::vks;

use crate::Darkfid;

pub struct Harness {
    pub _alice: Darkfid,
    pub _bob: Darkfid,
}

impl Harness {
    pub async fn new() -> Result<Self> {
        // Generate default genesis block
        let genesis_block = BlockInfo::default();

        // Generate validators configuration
        // NOTE: we are not using consensus constants here so we
        // don't get circular dependencies.
        let time_keeper = TimeKeeper::new(genesis_block.header.timestamp, 10, 90, 0);
        let config = ValidatorConfig::new(time_keeper, genesis_block, vec![]);

        // Generate validators using pregenerated vks
        let sled_db = sled::Config::new().temporary(true).open()?;
        vks::inject(&sled_db)?;
        let validator = Validator::new(&sled_db, config.clone()).await?;
        let _alice = Darkfid::new(validator).await;
        let sled_db = sled::Config::new().temporary(true).open()?;
        vks::inject(&sled_db)?;
        let validator = Validator::new(&sled_db, config.clone()).await?;
        let _bob = Darkfid::new(validator).await;

        Ok(Self { _alice, _bob })
    }
}
