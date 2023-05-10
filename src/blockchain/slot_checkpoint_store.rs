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

use darkfi_serial::{deserialize, serialize};

use crate::{blockchain::SledDbOverlayPtr, consensus::SlotCheckpoint, Error, Result};

const SLED_SLOT_CHECKPOINT_TREE: &[u8] = b"_slot_checkpoints";

/// The `SlotCheckpointStore` is a `sled` tree storing the checkpoints of the
/// blockchain's slots, where the key is the slot uid, and the value is
/// is the serialized checkpoint.
#[derive(Clone)]
pub struct SlotCheckpointStore(sled::Tree);

impl SlotCheckpointStore {
    /// Opens a new or existing `SlotCheckpointStore` on the given sled database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_SLOT_CHECKPOINT_TREE)?;
        let store = Self(tree);

        // In case the store is empty, initialize it with the genesis checkpoint.
        if store.0.is_empty() {
            let genesis_checkpoint = SlotCheckpoint::genesis_slot_checkpoint();
            store.insert(&[genesis_checkpoint])?;
        }

        Ok(store)
    }

    /// Insert a slice of [`SlotCheckpoint`] into the slotcheckpointstore.
    /// With sled, the operation is done as a batch.
    /// The block slot is used as the key, while value is the serialized [`SlotCheckpoint`] itself.
    pub fn insert(&self, checkpoints: &[SlotCheckpoint]) -> Result<()> {
        let mut batch = sled::Batch::default();

        for checkpoint in checkpoints {
            let serialized = serialize(checkpoint);
            batch.insert(&checkpoint.slot.to_be_bytes(), serialized);
        }

        self.0.apply_batch(batch)?;
        Ok(())
    }

    /// Check if the slotcheckpointstore contains a given slot.
    pub fn contains(&self, slot: u64) -> Result<bool> {
        Ok(self.0.contains_key(slot.to_be_bytes())?)
    }

    /// Fetch given slots from the slotcheckpointstore.
    /// The resulting vector contains `Option`, which is `Some` if the slot
    /// was found in the slotcheckpointstore, and otherwise it is `None`, if it has not.
    /// The second parameter is a boolean which tells the function to fail in
    /// case at least one slot was not found.
    pub fn get(&self, slots: &[u64], strict: bool) -> Result<Vec<Option<SlotCheckpoint>>> {
        let mut ret = Vec::with_capacity(slots.len());

        for slot in slots {
            if let Some(found) = self.0.get(slot.to_be_bytes())? {
                let checkpoint = deserialize(&found)?;
                ret.push(Some(checkpoint));
            } else {
                if strict {
                    return Err(Error::SlotNotFound(*slot))
                }
                ret.push(None);
            }
        }

        Ok(ret)
    }

    /// Retrieve all slot checkpointss from the slotcheckpointstore.
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<SlotCheckpoint>> {
        let mut slots = vec![];

        for slot in self.0.iter() {
            let (_, value) = slot.unwrap();
            let checkpoint = deserialize(&value)?;
            slots.push(checkpoint);
        }

        Ok(slots)
    }

    /// Fetch n slot checkpoints after given slot. In the iteration, if a slot is not
    /// found, the iteration stops and the function returns what it has found
    /// so far in the `SlotCheckpointStore`.
    pub fn get_after(&self, slot: u64, n: u64) -> Result<Vec<SlotCheckpoint>> {
        let mut ret = vec![];

        let mut key = slot;
        let mut counter = 0;
        while counter <= n {
            if let Some(found) = self.0.get_gt(key.to_be_bytes())? {
                let key_bytes: [u8; 8] = found.0.as_ref().try_into().unwrap();
                key = u64::from_be_bytes(key_bytes);
                let checkpoint = deserialize(&found.1)?;
                ret.push(checkpoint);
                counter += 1;
                continue
            }
            break
        }

        Ok(ret)
    }

    /// Fetch the last slot checkpoint in the tree, based on the `Ord`
    /// implementation for `Vec<u8>`. This should not be able to
    /// fail because we initialize the store with the genesis slot checkpoint.
    pub fn get_last(&self) -> Result<SlotCheckpoint> {
        let found = self.0.last()?.unwrap();
        let checkpoint = deserialize(&found.1)?;
        Ok(checkpoint)
    }

    /// Retrieve records count
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.len() == 0
    }
}

/// Overlay structure over a [`SlotCheckpointStore`] instance.
pub struct SlotCheckpointStoreOverlay(SledDbOverlayPtr);

impl SlotCheckpointStoreOverlay {
    pub fn new(overlay: SledDbOverlayPtr) -> Result<Self> {
        overlay.lock().unwrap().open_tree(SLED_SLOT_CHECKPOINT_TREE)?;
        Ok(Self(overlay))
    }

    /// Fetch given slot from the slotcheckpointstore.
    pub fn get(&self, slot: u64) -> Result<Vec<u8>> {
        match self.0.lock().unwrap().get(SLED_SLOT_CHECKPOINT_TREE, &slot.to_be_bytes())? {
            Some(found) => Ok(found.to_vec()),
            None => Err(Error::SlotNotFound(slot)),
        }
    }
}
