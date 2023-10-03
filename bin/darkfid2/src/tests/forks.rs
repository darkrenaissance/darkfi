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

use darkfi::{blockchain::Blockchain, validator::consensus::Fork, Result};

#[test]
fn forks() -> Result<()> {
    // Dummy records we will insert
    let record0 = blake3::hash(b"Let there be dark!");
    let record1 = blake3::hash(b"Never skip brain day.");

    // Create a temporary blockchain
    let blockchain = Blockchain::new(&sled::Config::new().temporary(true).open()?)?;

    // Create a fork
    let fork = Fork::new(&blockchain, Some(90))?;

    // Add a dummy record to fork
    fork.overlay.lock().unwrap().order.insert(&[0], &[record0])?;

    // Verify blockchain doesn't contain the record
    assert_eq!(blockchain.order.get(&[0], false)?, [None]);
    assert_eq!(fork.overlay.lock().unwrap().order.get(&[0], true)?, [Some(record0)]);

    // Now we are going to clone the fork
    let fork_clone = fork.full_clone()?;

    // Verify it cointains the original record
    assert_eq!(fork_clone.overlay.lock().unwrap().order.get(&[0], true)?, [Some(record0)]);

    // Add another dummy record to cloned fork
    fork_clone.overlay.lock().unwrap().order.insert(&[1], &[record1])?;

    // Verify blockchain and original fork don't contain the second record
    assert_eq!(blockchain.order.get(&[0, 1], false)?, [None, None]);
    assert_eq!(fork.overlay.lock().unwrap().order.get(&[0, 1], false)?, [Some(record0), None]);
    assert_eq!(
        fork_clone.overlay.lock().unwrap().order.get(&[0, 1], true)?,
        [Some(record0), Some(record1)]
    );

    Ok(())
}
