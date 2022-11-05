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

impl ContractStore {
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_CONTRACTS_TREE)?;
        Ok(Self(tree))
    }

    pub fn init(
        &self,
        db: &sled::Db,
        contract_id: &ContractId,
        tree_name: &str,
    ) -> Result<sled::Tree> {
        let contract_id_bytes = serialize(contract_id);

        // If the db was never initialized, it should not be in here.
        if self.0.contains_key(&contract_id_bytes)? {
            return Err(ContractAlreadyInitialized)
        }

        let mut hasher = blake3::Hasher::new();
        hasher.update(&contract_id_bytes);
        hasher.update(&tree_name.as_bytes());
        let ptr = hasher.finalize();

        // Now we add it so it's marked as initialized
        self.0.insert(&contract_id_bytes, ptr.as_bytes())?;

        // We open the tree and return its handle
        let tree = db.open_tree(ptr.as_bytes())?;
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

        let Some(state_pointers) = self.0.get(&contract_id_bytes)? else {
            return Err(ContractNotFound(contract_id.to_string()))
        };

        let state_pointers: Vec<[u8; 32]> = deserialize(&state_pointers)?;

        let mut hasher = blake3::Hasher::new();
        hasher.update(&contract_id_bytes);
        hasher.update(&tree_name.as_bytes());
        let ptr = hasher.finalize();

        // We assume the tree has been created already, so it should be listed in this array.
        // If not, that's an error.
        if !state_pointers.contains(ptr.as_bytes()) {
            return Err(ContractStateNotFound)
        }

        // We open the tree and return its handle
        let tree = db.open_tree(ptr.as_bytes())?;
        Ok(tree)
    }
}
