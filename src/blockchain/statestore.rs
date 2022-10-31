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

use darkfi_serial::serialize;

use crate::{crypto::contract_id::ContractId, Result};

const SLED_STATES_TREE: &[u8] = b"_states";

/// The `StateStore` is a `sled` tree storing states of deployed contracts.
/// The states themselves are data that is allocated and stored as raw bytes.
/// These bytes are (de)serialized by the code in wasm and the contracts can
/// operate on the state data themselves. Regarding on the (byte) size of the
/// state, the contract deployer should allocate and pay for a certain size of
/// their state stored by all the nodes. The cost should be linear to the byte
/// size used.
#[derive(Clone)]
pub struct StateStore(sled::Tree);

impl StateStore {
    /// Opens a new or existing `StateStore` on the given sled database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_STATES_TREE)?;
        Ok(Self(tree))
    }

    /// Insert a state into the store. This will replace the previous state.
    /// The contract's ID is used as a key, while the value is the contract
    /// state serialized to bytes.
    pub fn insert(&self, contract_id: &ContractId, contract_state: &[u8]) -> Result<()> {
        self.0.insert(serialize(contract_id), contract_state.to_vec())?;
        Ok(())
    }

    /// Check if the `StateStore` contains a state for the given `ContractId`.
    pub fn contains(&self, contract_id: &ContractId) -> Result<bool> {
        Ok(self.0.contains_key(serialize(contract_id))?)
    }

    /// Retrieve a state from the `StateStore` given a `ContractId` if it exists.
    pub fn get(&self, contract_id: &ContractId) -> Result<Option<Vec<u8>>> {
        if let Some(data) = self.0.get(serialize(contract_id))? {
            return Ok(Some(data.to_vec()))
        }

        Ok(None)
    }
}
