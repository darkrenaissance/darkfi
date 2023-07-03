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
    validator::{Validator, ValidatorConfig, ValidatorPtr},
    Result,
};
use darkfi_contract_test_harness::{init_logger, vks};

struct Harness {
    pub _alice: ValidatorPtr,
    pub _bob: ValidatorPtr,
}

impl Harness {
    async fn new() -> Result<Self> {
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
        let _alice = Validator::new(&sled_db, config.clone()).await?;
        let sled_db = sled::Config::new().temporary(true).open()?;
        vks::inject(&sled_db)?;
        let _bob = Validator::new(&sled_db, config).await?;

        Ok(Self { _alice, _bob })
    }
}

#[async_std::test]
async fn add_blocks() -> Result<()> {
    init_logger();

    // Initialize harness
    let _th = Harness::new().await?;

    // Thanks for reading
    Ok(())
}
