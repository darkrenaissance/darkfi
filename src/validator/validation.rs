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
    blockchain::{block_version, expected_reward, Slot},
    pasta::{group::ff::Field, pallas},
};
use num_bigint::BigUint;

use crate::{
    blockchain::{BlockInfo, Blockchain},
    validator::{pid::slot_pid_output, pow::PoWModule},
    Error, Result,
};

/// Validate provided block, using its previous, based on its version.
pub fn validate_block(
    block: &BlockInfo,
    previous: &BlockInfo,
    expected_reward: u64,
    module: &PoWModule,
) -> Result<()> {
    // TODO: verify block validations work as expected on versions change(cutoff)
    match block_version(block.header.height) {
        1 => validate_pow_block(block, previous, expected_reward, module)?,
        2 => validate_pos_block(block, previous, expected_reward)?,
        _ => return Err(Error::BlockVersionIsInvalid(block.header.version)),
    }

    Ok(())
}

/// A PoW block is considered valid when the following rules apply:
///     1. Block version is equal to 1
///     2. Parent hash is equal to the hash of the previous block
///     3. Block height increments previous block height by 1
///     4. Timestamp is valid based on PoWModule validation
///     5. Block hash is valid based on PoWModule validation
///     6. Slots vector contains a single valid slot
///     7. Block height is the same as the slots vector last slot id
/// Additional validity rules can be applied.
pub fn validate_pow_block(
    block: &BlockInfo,
    previous: &BlockInfo,
    expected_reward: u64,
    module: &PoWModule,
) -> Result<()> {
    let error = Err(Error::BlockIsInvalid(block.hash()?.to_string()));

    // Check block version (1)
    if block.header.version != 1 {
        return error
    }

    // Check previous hash (2)
    let previous_hash = previous.hash()?;
    if block.header.previous != previous_hash {
        return error
    }

    // Check heights are incremental (3)
    if block.header.height != previous.header.height + 1 {
        return error
    }

    // Check timestamp validity (4)
    if !module.verify_timestamp_by_median(block.header.timestamp.0) {
        return error
    }

    // Check block hash corresponds to next one (5)
    module.verify_block_hash(block)?;

    // Verify slots vector contains single slot (6)
    if block.slots.len() != 1 {
        return error
    }

    // Retrieve previous block last slot
    let previous_slot = previous.slots.last().unwrap();

    // Validate last slot
    let last_slot = block.slots.last().unwrap();
    validate_pow_slot(
        last_slot,
        previous_slot,
        &previous_hash,
        &previous.header.previous,
        &pallas::Base::from(previous.header.nonce),
        expected_reward,
    )?;

    // Check block height is the last slot id (7)
    if last_slot.id != block.header.height {
        return error
    }

    Ok(())
}

/// A PoW slot is considered valid when the following rules apply:
///     1. Id increments previous slot id by 1
///     2. Forks extend previous block hash
///     3. Forks follow previous block sequence
///     4. Slot total tokens represent the total network tokens
///        up until this slot
///     5. Slot's 'previous error' value matches the PID error of the previous slot
///     6. Slot previous has only 1 producer (the miner)
///     7. PID output for this slot is correct (zero)
///     8. Slot last nonce is the expected one
///     9. Slot reward value is the expected one
/// Additional validity rules can be applied.
pub fn validate_pow_slot(
    slot: &Slot,
    previous: &Slot,
    previous_block_hash: &blake3::Hash,
    previous_block_sequence: &blake3::Hash,
    last_nonce: &pallas::Base,
    expected_reward: u64,
) -> Result<()> {
    let error = Err(Error::SlotIsInvalid(slot.id));

    // Check slots are incremental (1)
    if slot.id != previous.id + 1 {
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

    // Check previous slot producers (6)
    if slot.previous.producers != 1 {
        return error
    }

    // Check PID output for this slot (7)
    if (slot.pid.f, slot.pid.error, slot.pid.sigma1, slot.pid.sigma2) !=
        (0.0, 0.0, pallas::Base::ZERO, pallas::Base::ZERO)
    {
        return error
    }

    // Check nonce is the expected one
    if &slot.last_nonce != last_nonce {
        return error
    }

    // Check reward is the expected one (9)
    if slot.reward != expected_reward {
        return error
    }

    Ok(())
}

/// A PoS block is considered valid when the following rules apply:
///     1. Block version is equal to 2
///     2. Parent hash is equal to the hash of the previous block
///     3. Timestamp increments previous block timestamp
///     4. Slot increments previous block slot
///     5. Slots vector is not empty and all its slots are valid
///     6. Slot is the same as the slots vector last slot id
/// Additional validity rules can be applied.
pub fn validate_pos_block(
    block: &BlockInfo,
    previous: &BlockInfo,
    expected_reward: u64,
) -> Result<()> {
    let error = Err(Error::BlockIsInvalid(block.hash()?.to_string()));

    // Check block version (1)
    if block.header.version != 2 {
        return error
    }

    // Check previous hash (2)
    let previous_hash = previous.hash()?;
    if block.header.previous != previous_hash {
        return error
    }

    // Check timestamps are incremental (3)
    if block.header.timestamp <= previous.header.timestamp {
        return error
    }

    // Check heights are incremental (4)
    if block.header.height <= previous.header.height {
        return error
    }

    // Verify slots (5)
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
            validate_pos_slot(
                slot,
                previous_slot,
                &previous_hash,
                &previous.header.previous,
                &previous.header.nonce,
                0,
            )?;
            previous_slot = slot;
        }
    }

    // Validate last slot
    let last_slot = block.slots.last().unwrap();
    validate_pos_slot(
        last_slot,
        previous_slot,
        &previous_hash,
        &previous.header.previous,
        &previous.header.nonce,
        expected_reward,
    )?;

    // Check block height is the last slot id (6)
    if last_slot.id != block.header.height {
        return error
    }

    Ok(())
}

/// A PoS slot is considered valid when the following rules apply:
///     1. Id increments previous slot id
///     2. Forks extend previous block hash
///     3. Forks follow previous block sequence
///     4. Slot total tokens represent the total network tokens
///        up until this slot
///     5. Slot's 'previous error' value matches the PID error of the previous slot
///     6. PID output for this slot is correct
///     7. Slot last nonce(eta) is the expected one
///     8. Slot reward value is the expected one
/// Additional validity rules can be applied.
pub fn validate_pos_slot(
    slot: &Slot,
    previous: &Slot,
    previous_block_hash: &blake3::Hash,
    previous_block_sequence: &blake3::Hash,
    last_nonce: &pallas::Base,
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

    // Check nonce (eta) is the expected one (7)
    if &slot.last_nonce != last_nonce {
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
pub fn validate_blockchain(
    blockchain: &Blockchain,
    pow_threads: usize,
    pow_target: usize,
    pow_fixed_difficulty: Option<BigUint>,
) -> Result<()> {
    // Generate a PoW module
    let mut module =
        PoWModule::new(blockchain.clone(), pow_threads, pow_target, pow_fixed_difficulty)?;
    // We use block order store here so we have all blocks in order
    let blocks = blockchain.order.get_all()?;
    for (index, block) in blocks[1..].iter().enumerate() {
        let full_blocks = blockchain.get_blocks_by_hash(&[blocks[index].1, block.1])?;
        let expected_reward = expected_reward(full_blocks[1].header.height);
        let full_block = &full_blocks[1];
        validate_block(full_block, &full_blocks[0], expected_reward, &module)?;
        // Update PoW module
        if full_block.header.version == 1 {
            module.append(full_block.header.timestamp.0, &module.next_difficulty()?);
        }
    }

    Ok(())
}
