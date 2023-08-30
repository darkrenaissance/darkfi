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

// [`Slot`] is defined in the sdk so contracts can use it
use darkfi_sdk::{blockchain::Slot, pasta::pallas};
use darkfi_serial::{deserialize, serialize};

use crate::{Error, Result};

use super::{parse_u64_key_record, SledDbOverlayPtr};

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

    /* TODO: FIXME: blockchain should not depend on validator
    // Check PID output for this slot (6)
    if (slot.pid.f, slot.pid.error, slot.pid.sigma1, slot.pid.sigma2) !=
        slot_pid_output(previous, slot.previous.producers)
    {
        return error
    }
    */

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

const SLED_SLOT_TREE: &[u8] = b"_slots";

/// The `SlotStore` is a `sled` tree storing the blockhains' slots,
/// where the key is the slot uid, and the value is is the serialized slot.
#[derive(Clone)]
pub struct SlotStore(pub sled::Tree);

impl SlotStore {
    /// Opens a new or existing `SlotStore` on the given sled database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_SLOT_TREE)?;
        Ok(Self(tree))
    }

    /// Insert a slice of [`Slot`] into the slot store.
    pub fn insert(&self, slots: &[Slot]) -> Result<()> {
        let batch = self.insert_batch(slots)?;
        self.0.apply_batch(batch)?;
        Ok(())
    }

    /// Generate the sled batch corresponding to an insert, so caller
    /// can handle the write operation.
    /// The slot id is used as the key, while value is the serialized [`Slot`] itself.
    pub fn insert_batch(&self, slots: &[Slot]) -> Result<sled::Batch> {
        let mut batch = sled::Batch::default();

        for slot in slots {
            let serialized = serialize(slot);
            batch.insert(&slot.id.to_be_bytes(), serialized);
        }

        Ok(batch)
    }

    /// Check if the slot store contains a given id.
    pub fn contains(&self, id: u64) -> Result<bool> {
        Ok(self.0.contains_key(id.to_be_bytes())?)
    }

    /// Fetch given slots from the slot store.
    /// The resulting vector contains `Option`, which is `Some` if the slot
    /// was found in the slot store, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one slot was not found.
    pub fn get(&self, ids: &[u64], strict: bool) -> Result<Vec<Option<Slot>>> {
        let mut ret = Vec::with_capacity(ids.len());

        for id in ids {
            if let Some(found) = self.0.get(id.to_be_bytes())? {
                let slot = deserialize(&found)?;
                ret.push(Some(slot));
            } else {
                if strict {
                    return Err(Error::SlotNotFound(*id))
                }
                ret.push(None);
            }
        }

        Ok(ret)
    }

    /// Retrieve all slot from the slot store.
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<Slot>> {
        let mut slots = vec![];

        for slot in self.0.iter() {
            let (_, slot): (u64, Slot) = parse_u64_key_record(slot.unwrap())?;
            slots.push(slot);
        }

        Ok(slots)
    }

    /// Fetch n slots after given slot. In the iteration, if a slot is not
    /// found, the iteration stops and the function returns what it has found
    /// so far in the `SlotStore`.
    pub fn get_after(&self, id: u64, n: u64) -> Result<Vec<Slot>> {
        let mut ret = vec![];

        let mut key = id;
        let mut counter = 0;
        while counter <= n {
            if let Some(found) = self.0.get_gt(key.to_be_bytes())? {
                let (id, slot) = parse_u64_key_record(found)?;
                key = id;
                ret.push(slot);
                counter += 1;
                continue
            }
            break
        }

        Ok(ret)
    }

    /// Fetch the last slot in the tree, based on the `Ord`
    /// implementation for `Vec<u8>`. This should not be able to
    /// fail because we initialize the store with the genesis slot.
    pub fn get_last(&self) -> Result<Slot> {
        let found = self.0.last()?.unwrap();
        let slot = deserialize(&found.1)?;
        Ok(slot)
    }

    /// Retrieve records count
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Overlay structure over a [`SlotStore`] instance.
pub struct SlotStoreOverlay(SledDbOverlayPtr);

impl SlotStoreOverlay {
    pub fn new(overlay: &SledDbOverlayPtr) -> Result<Self> {
        overlay.lock().unwrap().open_tree(SLED_SLOT_TREE)?;
        Ok(Self(overlay.clone()))
    }

    /// Insert a slice of [`Slot`] into the overlay.
    /// The slot id is used as the key, while value is the serialized [`Slot`] itself.
    pub fn insert(&self, slots: &[Slot]) -> Result<()> {
        let mut lock = self.0.lock().unwrap();

        for slot in slots {
            let serialized = serialize(slot);
            lock.insert(SLED_SLOT_TREE, &slot.id.to_be_bytes(), &serialized)?;
        }

        Ok(())
    }

    /// Fetch slot from the overlay by id.
    pub fn get_by_id(&self, id: u64) -> Result<Vec<u8>> {
        match self.0.lock().unwrap().get(SLED_SLOT_TREE, &id.to_be_bytes())? {
            Some(found) => Ok(found.to_vec()),
            None => Err(Error::SlotNotFound(id)),
        }
    }

    /// Fetch given slots from the overlay.
    /// The resulting vector contains `Option`, which is `Some` if the slot
    /// was found in the overlay, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one slot was not found.
    pub fn get(&self, ids: &[u64], strict: bool) -> Result<Vec<Option<Slot>>> {
        let mut ret = Vec::with_capacity(ids.len());
        let lock = self.0.lock().unwrap();

        for id in ids {
            if let Some(found) = lock.get(SLED_SLOT_TREE, &id.to_be_bytes())? {
                let slot = deserialize(&found)?;
                ret.push(Some(slot));
            } else {
                if strict {
                    return Err(Error::SlotNotFound(*id))
                }
                ret.push(None);
            }
        }

        Ok(ret)
    }

    /// Fetch the last slot from the overlay, based on the `Ord`
    /// implementation for `Vec<u8>`.
    pub fn get_last(&self) -> Result<Slot> {
        let found = self.0.lock().unwrap().last(SLED_SLOT_TREE)?.unwrap();
        let slot = deserialize(&found.1)?;
        Ok(slot)
    }
}
