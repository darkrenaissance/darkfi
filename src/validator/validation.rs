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

use darkfi_sdk::{
    blockchain::{expected_reward, Slot},
    pasta::pallas,
};

use crate::{
    blockchain::{BlockInfo, Blockchain},
    validator::pid::slot_pid_output,
    Error, Result,
};

/// A block is considered valid when the following rules apply:
///     1. Parent hash is equal to the hash of the previous block
///     2. Timestamp increments previous block timestamp
///     3. Slot increments previous block slot
///     4. Slots vector is not empty and all its slots are valid
///     5. Slot is the same as the slots vector last slot id
/// Additional validity rules can be applied.
pub fn validate_block(block: &BlockInfo, previous: &BlockInfo, expected_reward: u64) -> Result<()> {
    let error = Err(Error::BlockIsInvalid(block.blockhash().to_string()));
    let previous_hash = previous.blockhash();

    // Check previous hash (1)
    if block.header.previous != previous_hash {
        return error
    }

    // Check timestamps are incremental (2)
    if block.header.timestamp <= previous.header.timestamp {
        return error
    }

    // Check slots are incremental (3)
    if block.header.slot <= previous.header.slot {
        return error
    }

    // Verify slots (4)
    if block.slots.is_empty() {
        return error
    }

    // Retrieve previous block last slot
    let mut previous_slot = previous.slots.last().unwrap();

    // Check if empty slots existed
    if block.slots.len() > 1 {
        // All slots exluding the last one must have reward value set to 0.
        // Slots must already be in correct order (sorted by id).
        for slot in &block.slots[..block.slots.len() - 1] {
            validate_slot(
                slot,
                previous_slot,
                &previous_hash,
                &previous.header.previous,
                &previous.eta,
                0,
            )?;
            previous_slot = slot;
        }
    }

    validate_slot(
        block.slots.last().unwrap(),
        previous_slot,
        &previous_hash,
        &previous.header.previous,
        &previous.eta,
        expected_reward,
    )?;

    // Check block slot is the last slot id (5)
    if block.slots.last().unwrap().id != block.header.slot {
        return error
    }

    Ok(())
}

/// A slot is considered valid when the following rules apply:
///     1. Id increments previous slot id
///     2. Forks extend previous block hash
///     3. Forks follow previous block sequence
///     4. Slot total tokens represent the total network tokens
///        up until this slot
///     5. Slot previous error value correspond to previous slot one
///     6. PID output for this slot is correct
///     7. Slot last eta is the expected one
///     8. Slot reward value is the expected one
/// Additional validity rules can be applied.
pub fn validate_slot(
    slot: &Slot,
    previous: &Slot,
    previous_block_hash: &blake3::Hash,
    previous_block_sequence: &blake3::Hash,
    last_eta: &pallas::Base,
    expected_reward: u64,
) -> Result<()> {
    let error = Err(Error::SlotIsInvalid(slot.id));

    // Check slots are incremental (1)
    if slot.id <= previous.id {
        return error
    }

    // Check previous block hash (2)
    if !slot.previous.last_hashes.contains(previous_block_hash) {
        return error
    }

    // Check previous block sequence (3)
    if !slot.previous.second_to_last_hashes.contains(previous_block_sequence) {
        return error
    }

    // Check total tokens (4)
    if slot.total_tokens != previous.total_tokens + previous.reward {
        return error
    }

    // Check previous slot error (5)
    if slot.previous.error != previous.pid.error {
        return error
    }

    // Check PID output for this slot (6)
    if (slot.pid.f, slot.pid.error, slot.pid.sigma1, slot.pid.sigma2) !=
        slot_pid_output(previous, slot.previous.producers)
    {
        return error
    }

    // Check reward is the expected one (7)
    if &slot.last_eta != last_eta {
        return error
    }

    // Check reward is the expected one (8)
    if slot.reward != expected_reward {
        return error
    }

    Ok(())
}

/// A blockchain is considered valid, when every block is valid,
/// based on validate_block checks.
/// Be careful as this will try to load everything in memory.
pub fn validate_blockchain(blockchain: &Blockchain) -> Result<()> {
    // We use block order store here so we have all blocks in order
    let blocks = blockchain.order.get_all()?;
    for (index, block) in blocks[1..].iter().enumerate() {
        let full_blocks = blockchain.get_blocks_by_hash(&[blocks[index].1, block.1])?;
        let expected_reward = expected_reward(full_blocks[1].header.slot);
        validate_block(&full_blocks[1], &full_blocks[0], expected_reward)?;
    }

    Ok(())
}
