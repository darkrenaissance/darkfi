/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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
use darkfi_sdk::blockchain::Slot;
use darkfi_serial::{deserialize, serialize};

use crate::{Error, Result};

use super::{parse_record, parse_u64_key_record, SledDbOverlayPtr};

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

const SLED_BLOCK_SLOTS_TREE: &[u8] = b"_blocks_slots";

/// The `BlocksSlotsStore` is a `sled` tree storing all the blocks' corresponding slot
/// uids, meaning the slot numbers leading up to each block, where the key is the
/// blocks' hash, and value is the serialized slot uids vector.
#[derive(Clone)]
pub struct BlocksSlotsStore(pub sled::Tree);

impl BlocksSlotsStore {
    /// Opens a new or existing `BlocksSlotsStore` on the given sled database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_BLOCK_SLOTS_TREE)?;
        Ok(Self(tree))
    }

    /// Insert a slice of block hashes and their `u64` vectors into the store.
    pub fn insert(&self, hashes: &[blake3::Hash], slots: &[&Vec<u64>]) -> Result<()> {
        let batch = self.insert_batch(hashes, slots)?;
        self.0.apply_batch(batch)?;
        Ok(())
    }

    /// Generate the sled batch corresponding to an insert, so caller
    /// can handle the write operation. The block hash is used as the key,
    /// and the block slots serialized vector is used as value.
    pub fn insert_batch(
        &self,
        hashes: &[blake3::Hash],
        slots: &[&Vec<u64>],
    ) -> Result<sled::Batch> {
        if hashes.len() != slots.len() {
            return Err(Error::InvalidInputLengths)
        }

        let mut batch = sled::Batch::default();

        for (i, hash) in hashes.iter().enumerate() {
            let serialized = serialize(slots[i]);
            batch.insert(hash.as_bytes(), serialized);
        }

        Ok(batch)
    }

    /// Check if the blocks slots store contains a given block hash.
    pub fn contains(&self, blockhash: &blake3::Hash) -> Result<bool> {
        Ok(self.0.contains_key(blockhash.as_bytes())?)
    }

    /// Fetch given blocks slots from the blocks slots store.
    /// The resulting vector contains `Option`, which is `Some` if the block slots
    /// were found in the blocks slots store, and otherwise it is `None`, if they have not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one block was not found.
    pub fn get(
        &self,
        block_hashes: &[blake3::Hash],
        strict: bool,
    ) -> Result<Vec<Option<Vec<u64>>>> {
        let mut ret = Vec::with_capacity(block_hashes.len());

        for hash in block_hashes {
            if let Some(found) = self.0.get(hash.as_bytes())? {
                let slots = deserialize(&found)?;
                ret.push(Some(slots));
            } else {
                if strict {
                    let s = hash.to_hex().as_str().to_string();
                    return Err(Error::BlockSlotsNotFound(s))
                }
                ret.push(None);
            }
        }

        Ok(ret)
    }

    /// Retrieve all blocks slots from the block store in the form of a tuple
    /// (`hash`, `Vec<u64>`).
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<(blake3::Hash, Vec<u64>)>> {
        let mut blocks_slots = vec![];

        for block_slots in self.0.iter() {
            blocks_slots.push(parse_record(block_slots.unwrap())?);
        }

        Ok(blocks_slots)
    }
}

/// Overlay structure over a [`BlocksSlotsStore`] instance.
pub struct BlocksSlotsStoreOverlay(SledDbOverlayPtr);

impl BlocksSlotsStoreOverlay {
    pub fn new(overlay: &SledDbOverlayPtr) -> Result<Self> {
        overlay.lock().unwrap().open_tree(SLED_BLOCK_SLOTS_TREE)?;
        Ok(Self(overlay.clone()))
    }

    /// Insert a slice of block hashes and their `u64` vectors into the overlay.
    /// The block hash is used as the key, and the block slots serialized vector
    /// is used as value.
    pub fn insert(&self, hashes: &[blake3::Hash], slots: &[&Vec<u64>]) -> Result<()> {
        if hashes.len() != slots.len() {
            return Err(Error::InvalidInputLengths)
        }

        let mut lock = self.0.lock().unwrap();

        for (i, hash) in hashes.iter().enumerate() {
            let serialized = serialize(slots[i]);
            lock.insert(SLED_BLOCK_SLOTS_TREE, hash.as_bytes(), &serialized)?;
        }

        Ok(())
    }

    /// Fetch given blocks slots from the overlay.
    /// The resulting vector contains `Option`, which is `Some` if the block slots
    /// were found in the overlay, and otherwise it is `None`, if they have not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one block was not found.
    pub fn get(
        &self,
        block_hashes: &[blake3::Hash],
        strict: bool,
    ) -> Result<Vec<Option<Vec<u64>>>> {
        let mut ret = Vec::with_capacity(block_hashes.len());
        let lock = self.0.lock().unwrap();

        for hash in block_hashes {
            if let Some(found) = lock.get(SLED_BLOCK_SLOTS_TREE, hash.as_bytes())? {
                let slots = deserialize(&found)?;
                ret.push(Some(slots));
            } else {
                if strict {
                    let s = hash.to_hex().as_str().to_string();
                    return Err(Error::BlockSlotsNotFound(s))
                }
                ret.push(None);
            }
        }

        Ok(ret)
    }
}
