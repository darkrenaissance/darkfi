/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
r* This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use darkfi_sdk::crypto::ContractId;
use darkfi_serial::{deserialize, serialize};
use log::{debug, error};

use crate::{Error, Result};

const SLED_CONTRACTS_TREE: &[u8] = b"_contracts";
const SLED_BINCODE_TREE: &[u8] = b"_wasm_bincode";

// Logger targets
const CS_TGT_INIT: &str = "blockchain::contractstatestore::init";
const CS_TGT_LKUP: &str = "blockchain::contractstatestore::lookup";
const CS_TGT_DROP: &str = "blockchain::contractstatestore::remove";

/// The `WasmStore` is a `sled` tree that stores the wasm bincode for deployed
/// contracts.
#[derive(Clone)]
pub struct WasmStore(sled::Tree);

impl WasmStore {
    /// Opens or creates a `WasmStore`. This tree holds the wasm bincode.
    /// The layout looks like this:
    /// ```plaintext
    ///  tree: "_wasm_bincode"
    ///   key: ContractId
    /// value: Vec<u8>
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_BINCODE_TREE)?;
        Ok(Self(tree))
    }

    /// Fetches the bincode for a given ContractId
    /// Returns an error if the bincode is not found.
    pub fn get(&self, contract_id: ContractId) -> Result<Vec<u8>> {
        if let Some(bincode) = self.0.get(&serialize(&contract_id))? {
            return Ok(bincode.to_vec())
        }

        Err(Error::WasmBincodeNotFound)
    }

    /// Inserts or replaces the bincode for a given ContractId
    pub fn insert(&self, contract_id: ContractId, bincode: &[u8]) -> Result<()> {
        if let Err(e) = self.0.insert(&serialize(&contract_id), bincode) {
            error!("Failed to insert bincode to WasmStore: {}", e);
            return Err(e.into())
        }

        Ok(())
    }
}

/// The `ContractStateStore` is a `sled` tree that stores pointers to contracts'
/// databases. See the rustdoc for the impl functions for more info.
#[derive(Clone)]
pub struct ContractStateStore(sled::Tree);

impl ContractStateStore {
    /// Opens or creates a `ContractStateStore`. This main tree holds the links
    /// of contracts' states.
    /// The layout looks like this:
    /// ```plaintext
    ///  tree: "_contracts"
    ///   key: ContractId
    /// value: Vec<blake3(ContractId || tree_name)>
    /// ```
    /// These values get mutated with `init()` and `remove()`.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let tree = db.open_tree(SLED_CONTRACTS_TREE)?;
        Ok(Self(tree))
    }

    /// Try to initialize a new contract state. Contracts can create a number
    /// of trees, separated by `tree_name`, which they can then use from the
    /// smart contract API. `init()` will look into the main `ContractStateStore`
    /// tree to check if the smart contract was already deployed, and if so
    /// it will fetch a vector of these states that were initialized. If the
    /// state was already found, this function will return an error, because
    /// in this case the handle should be fetched using `lookup()`.
    /// If the tree was not initialized previously, it will be appended to
    /// the main `ContractStateStore` tree and a `sled::Tree` handle will be
    /// returned.
    pub fn init(
        &self,
        db: &sled::Db,
        contract_id: &ContractId,
        tree_name: &str,
    ) -> Result<sled::Tree> {
        debug!(target: CS_TGT_INIT, "Initializing state tree for {}:{}", contract_id, tree_name);

        let contract_id_bytes = serialize(contract_id);
        let ptr = contract_id.hash_state_id(tree_name);

        // See if there are existing state trees. If not, just start with an
        // empty vector.
        let mut state_pointers: Vec<[u8; 32]> = if self.0.contains_key(&contract_id_bytes)? {
            let bytes = self.0.get(&contract_id_bytes)?.unwrap();
            deserialize(&bytes)?
        } else {
            vec![]
        };

        // If the db was never initialized, it should not be in here.
        if state_pointers.contains(&ptr) {
            return Err(Error::ContractAlreadyInitialized)
        }

        // Now we add it so it's marked as initialized
        state_pointers.push(ptr);

        // We do this as a batch so in case of not being able to open the tree
        // we don't write that it's initialized.
        let mut batch = sled::Batch::default();
        batch.insert(contract_id_bytes, serialize(&state_pointers));

        // We open the tree and return its handle
        let tree = db.open_tree(ptr)?;

        // On success, apply the batch
        self.0.apply_batch(batch)?;

        Ok(tree)
    }

    /// Do a lookup of an existing contract state. In order to succeed, the
    /// state must have been previously initialized with `init()`. If the
    /// state has been found, a handle to it will be returned. Otherwise, we
    /// return an error.
    pub fn lookup(
        &self,
        db: &sled::Db,
        contract_id: &ContractId,
        tree_name: &str,
    ) -> Result<sled::Tree> {
        debug!(target: CS_TGT_LKUP, "Looking up state tree for {}:{}", contract_id, tree_name);

        let contract_id_bytes = serialize(contract_id);
        let ptr = contract_id.hash_state_id(tree_name);

        // A guard to make sure we went through init()
        if !self.0.contains_key(&contract_id_bytes)? {
            return Err(Error::ContractNotFound(contract_id.to_string()))
        }

        let state_pointers = self.0.get(&contract_id_bytes)?.unwrap();
        let state_pointers: Vec<[u8; 32]> = deserialize(&state_pointers)?;

        // We assume the tree has been created already, so it should be listed
        // in this array. If not, that's an error.
        if !state_pointers.contains(&ptr) {
            return Err(Error::ContractStateNotFound)
        }

        // We open the tree and return its handle
        let tree = db.open_tree(ptr)?;
        Ok(tree)
    }

    /// Attempt to remove an existing contract state. In order to succeed, the
    /// state must have been previously initialized with `init()`. If the state
    /// has been found, its contents in the tree will be cleared, and the pointer
    /// will be removed from the main `ContractStateStore`. If anything is not
    /// found as initialized, an error is returned.
    pub fn remove(&self, db: &sled::Db, contract_id: &ContractId, tree_name: &str) -> Result<()> {
        debug!(target: CS_TGT_DROP, "Removing state tree for {}:{}", contract_id, tree_name);

        let contract_id_bytes = serialize(contract_id);
        let ptr = contract_id.hash_state_id(tree_name);

        // A guard to make sure we went through init()
        if !self.0.contains_key(&contract_id_bytes)? {
            return Err(Error::ContractNotFound(contract_id.to_string()))
        }

        let state_pointers = self.0.get(&contract_id_bytes)?.unwrap();
        let mut state_pointers: Vec<[u8; 32]> = deserialize(&state_pointers)?;

        // We assume the tree has been created already, so it should be listed
        // in this array. If not, that's an error.
        if !state_pointers.contains(&ptr) {
            return Err(Error::ContractStateNotFound)
        }

        // We open the tree and clear it. This is unfortunately not atomic.
        let tree = db.open_tree(ptr)?;
        tree.clear()?;

        state_pointers.retain(|x| *x != ptr);
        self.0.insert(contract_id_bytes, serialize(&state_pointers))?;

        Ok(())
    }
}
