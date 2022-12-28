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

use darkfi_sdk::crypto::MerkleNode;
use darkfi_serial::{deserialize, serialize};

use crate::Result;

const SLED_ROOTS_TREE: &[u8] = b"_merkleroots";

/// The `RootStore` is a `sled` tree storing all the Merkle roots seen
/// in existing blocks. The key is the Merkle root itself, while the value
/// is an empty vector that's not used.
#[derive(Clone)]
pub struct RootStore(sled::Tree);

impl RootStore {
    /// Opens a new or existing `RootStore` on the given sled database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_ROOTS_TREE)?;
        Ok(Self(tree))
    }

    /// Insert a slice of [`MerkleNode`] into the store. With sled, the
    /// operation is done as a batch. The Merkle root is used as a key,
    /// while the value is an empty vector.
    pub fn insert(&self, roots: &[MerkleNode]) -> Result<()> {
        let mut batch = sled::Batch::default();

        for root in roots {
            batch.insert(serialize(root), vec![] as Vec<u8>);
        }

        self.0.apply_batch(batch)?;
        Ok(())
    }

    /// Check if the rootstore contains a given Merkle root.
    pub fn contains(&self, root: &MerkleNode) -> Result<bool> {
        Ok(self.0.contains_key(serialize(root))?)
    }

    /// Retrieve all Merkle roots from the store.
    /// Be careful as this will try to load everything in memory.
    pub fn get_all(&self) -> Result<Vec<MerkleNode>> {
        let mut roots = vec![];

        for root in self.0.iter() {
            let (key, _) = root.unwrap();
            let root = deserialize(&key)?;
            roots.push(root);
        }

        Ok(roots)
    }
}
