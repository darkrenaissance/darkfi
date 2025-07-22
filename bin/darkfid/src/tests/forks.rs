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

use darkfi::{
    blockchain::{BlockInfo, Blockchain, HeaderHash},
    validator::{consensus::Fork, pow::PoWModule},
    Result,
};
use sled_overlay::sled;

#[test]
fn forks() -> Result<()> {
    smol::block_on(async {
        // Dummy records we will insert
        let record1 = HeaderHash::new(blake3::hash(b"Let there be dark!").into());
        let record2 = HeaderHash::new(blake3::hash(b"Never skip brain day.").into());

        // Create a temporary blockchain
        let blockchain = Blockchain::new(&sled::Config::new().temporary(true).open()?)?;

        // Generate and insert default genesis block
        let genesis_block = BlockInfo::default();
        blockchain.add_block(&genesis_block)?;
        let genesis_block_hash = genesis_block.hash();

        // Generate the PoW module
        let module = PoWModule::new(blockchain.clone(), 90, None, None)?;

        // Create a fork
        let fork = Fork::new(blockchain.clone(), module).await?;

        // Add a dummy record to fork
        fork.overlay.lock().unwrap().blocks.insert_order(&[1], &[record1])?;

        // Verify blockchain doesn't contain the record
        assert_eq!(blockchain.blocks.get_order(&[0, 1], false)?, [Some(genesis_block_hash), None]);
        assert_eq!(
            fork.overlay.lock().unwrap().blocks.get_order(&[0, 1], true)?,
            [Some(genesis_block_hash), Some(record1)]
        );

        // Now we are going to clone the fork
        let fork_clone = fork.full_clone()?;

        // Verify it contains the original records
        assert_eq!(
            fork_clone.overlay.lock().unwrap().blocks.get_order(&[0, 1], true)?,
            [Some(genesis_block_hash), Some(record1)]
        );

        // Add another dummy record to cloned fork
        fork_clone.overlay.lock().unwrap().blocks.insert_order(&[2], &[record2])?;

        // Verify blockchain and original fork don't contain the second record
        assert_eq!(
            blockchain.blocks.get_order(&[0, 1, 2], false)?,
            [Some(genesis_block_hash), None, None]
        );
        assert_eq!(
            fork.overlay.lock().unwrap().blocks.get_order(&[0, 1, 2], false)?,
            [Some(genesis_block_hash), Some(record1), None]
        );
        assert_eq!(
            fork_clone.overlay.lock().unwrap().blocks.get_order(&[0, 1, 2], true)?,
            [Some(genesis_block_hash), Some(record1), Some(record2)]
        );

        Ok(())
    })
}
