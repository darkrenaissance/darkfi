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

use darkfi::Result;
use sled_overlay::sled;

pub const SLED_ORDER_TREE: &[u8] = b"_order";
pub const SLED_STATE_INVERSE_DIFF_TREE: &[u8] = b"_state_inverse_diff";
pub const SLED_MERKLE_TREES_TREE: &[u8] = b"_merkle_trees";
pub const SLED_MONEY_SMT_TREE: &[u8] = b"_money_smt";

/// Structure holding all sled trees that define the Blockchain cache.
#[derive(Clone)]
pub struct Cache {
    /// Main pointer to the sled db connection
    pub sled_db: sled::Db,
    /// The `sled` tree storing the order of the blockchain's blocks,
    /// where the key is the height number, and the value is the blocks'
    /// hash.
    pub order: sled::Tree,
    /// The `sled` tree storing each blocks' full database state inverse
    /// changes, where the key is the block height number, and the value
    /// is the serialized database inverse diff.
    pub state_inverse_diff: sled::Tree,
    /// The `sled` tree storing the merkle trees of the blockchain,
    /// where the key is the tree name, and the value is the serialized
    /// merkle tree itself.
    pub merkle_trees: sled::Tree,
    /// The `sled` tree storing the Sparse Merkle Tree of the money
    /// contract.
    pub money_smt: sled::Tree,
}

impl Cache {
    /// Instantiate a new `Cache` with the given `sled` database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let order = db.open_tree(SLED_ORDER_TREE)?;
        let state_inverse_diff = db.open_tree(SLED_STATE_INVERSE_DIFF_TREE)?;
        let merkle_trees = db.open_tree(SLED_MERKLE_TREES_TREE)?;
        let money_smt = db.open_tree(SLED_MONEY_SMT_TREE)?;

        Ok(Self { sled_db: db.clone(), order, state_inverse_diff, merkle_trees, money_smt })
    }
}
