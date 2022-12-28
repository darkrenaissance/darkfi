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

use darkfi_sdk::crypto::Nullifier;
use darkfi_serial::{deserialize, serialize};

use crate::Result;

const SLED_NULLIFIER_TREE: &[u8] = b"_nullifiers";

/// The `NullifierStore` is a `sled` tree storing all the nullifiers seen
/// in existing blocks. The key is the nullifier itself, while the value
/// is an empty vector that's not used. As a sidenote, perhaps we could
/// hold the transaction hash where the nullifier was seen in the value.
#[derive(Clone)]
pub struct NullifierStore(sled::Tree);

impl NullifierStore {
    /// Opens a new or existing `NullifierStore` on the given sled database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_NULLIFIER_TREE)?;
        Ok(Self(tree))
    }

    /// Insert a slice of [`Nullifier`] into the store. With sled, the
    /// operation is done as a batch. The nullifier is used as a key,
    /// while the value is an empty vector.
    pub fn insert(&self, nfs: &[Nullifier]) -> Result<()> {
        let mut batch = sled::Batch::default();

        for nf in nfs {
            batch.insert(serialize(nf), vec![] as Vec<u8>);
        }

        self.0.apply_batch(batch)?;
        Ok(())
    }

    /// Check if the nullifierstore contains a given nullifier.
    pub fn contains(&self, nullifier: &Nullifier) -> Result<bool> {
        Ok(self.0.contains_key(serialize(nullifier))?)
    }

    /// Retrieve all nullifiers from the store.
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<Nullifier>> {
        let mut nullifiers = vec![];

        for nullifier in self.0.iter() {
            let (key, _) = nullifier.unwrap();
            let nullifier = deserialize(&key)?;
            nullifiers.push(nullifier);
        }

        Ok(nullifiers)
    }
}
