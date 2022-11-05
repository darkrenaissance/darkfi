/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use darkfi_sdk::crypto::ContractId;
use darkfi_serial::{deserialize, serialize};

use crate::{
    Error::{ContractAlreadyInitialized, ContractNotFound, ContractStateNotFound},
    Result,
};

#[derive(Clone)]
pub struct ContractStore(sled::Tree);

const SLED_CONTRACTS_TREE: &[u8] = b"_contracts";

// =================
// TODO: Drop tree
// =================

impl ContractStore {
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_CONTRACTS_TREE)?;
        Ok(Self(tree))
    }

    /// Database layout:
    /// ```plaintext
    /// Tree: _contracts
    /// key:   ContractId
    /// value: blake3(ContractId || tree_name)
    /// ```
    ///
    /// `value` when init-ed represents a Contract's state tree:
    /// ```plaintext
    /// Tree: blake3(ContractId || tree_name)
    /// key: &[u8]
    /// value: &[u8]
    /// ```
    pub fn init(
        &self,
        db: &sled::Db,
        contract_id: &ContractId,
        tree_name: &str,
    ) -> Result<sled::Tree> {
        let contract_id_bytes = serialize(contract_id);

        let mut state_pointers: Vec<[u8; 32]> = if self.0.contains_key(&contract_id_bytes)? {
            let bytes = self.0.get(&contract_id_bytes)?.unwrap();
            deserialize(&bytes)?
        } else {
            vec![]
        };

        let mut hasher = blake3::Hasher::new();
        hasher.update(&contract_id_bytes);
        hasher.update(&tree_name.as_bytes());
        let ptr = hasher.finalize();
        let ptr = ptr.as_bytes();

        // If the db was never initialized, it should not be in here.
        if state_pointers.contains(ptr) {
            return Err(ContractAlreadyInitialized)
        }

        // Now we add it so it's marked as initialized
        state_pointers.push(*ptr);
        self.0.insert(&contract_id_bytes, serialize(&state_pointers))?;

        // We open the tree and return its handle
        let tree = db.open_tree(ptr)?;
        Ok(tree)
    }

    pub fn lookup(
        &self,
        db: &sled::Db,
        contract_id: &ContractId,
        tree_name: &str,
    ) -> Result<sled::Tree> {
        let contract_id_bytes = serialize(contract_id);

        // A guard to make sure we went through init()
        if !self.0.contains_key(&contract_id_bytes)? {
            return Err(ContractNotFound(contract_id.to_string()))
        }

        let state_pointers = self.0.get(&contract_id_bytes)?.unwrap();

        let state_pointers: Vec<[u8; 32]> = deserialize(&state_pointers)?;

        let mut hasher = blake3::Hasher::new();
        hasher.update(&contract_id_bytes);
        hasher.update(&tree_name.as_bytes());
        let ptr = hasher.finalize();
        let ptr = ptr.as_bytes();

        // We assume the tree has been created already, so it should be listed in this array.
        // If not, that's an error.
        if !state_pointers.contains(ptr) {
            return Err(ContractStateNotFound)
        }

        // We open the tree and return its handle
        let tree = db.open_tree(ptr)?;
        Ok(tree)
    }
}
