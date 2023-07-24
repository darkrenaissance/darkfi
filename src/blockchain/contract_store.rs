/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use std::io::Cursor;

use darkfi_sdk::crypto::ContractId;
use darkfi_serial::{deserialize, serialize};
use log::{debug, error};

use crate::{
    runtime::vm_runtime::SMART_CONTRACT_ZKAS_DB_NAME,
    zk::{empty_witnesses, VerifyingKey, ZkCircuit},
    zkas::ZkBinary,
    Error, Result,
};

use super::SledDbOverlayPtr;

const SLED_CONTRACTS_TREE: &[u8] = b"_contracts";
const SLED_BINCODE_TREE: &[u8] = b"_wasm_bincode";

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
        if let Some(bincode) = self.0.get(serialize(&contract_id))? {
            return Ok(bincode.to_vec())
        }

        Err(Error::WasmBincodeNotFound)
    }
}

/// Overlay structure over a [`WasmStore`] instance.
pub struct WasmStoreOverlay(SledDbOverlayPtr);

impl WasmStoreOverlay {
    pub fn new(overlay: &SledDbOverlayPtr) -> Result<Self> {
        overlay.lock().unwrap().open_tree(SLED_BINCODE_TREE)?;
        Ok(Self(overlay.clone()))
    }

    /// Fetches the bincode for a given ContractId
    /// Returns an error if the bincode is not found.
    pub fn get(&self, contract_id: ContractId) -> Result<Vec<u8>> {
        if let Some(bincode) =
            self.0.lock().unwrap().get(SLED_BINCODE_TREE, &serialize(&contract_id))?
        {
            return Ok(bincode.to_vec())
        }

        Err(Error::WasmBincodeNotFound)
    }

    /// Inserts or replaces the bincode for a given ContractId
    pub fn insert(&self, contract_id: ContractId, bincode: &[u8]) -> Result<()> {
        if let Err(e) =
            self.0.lock().unwrap().insert(SLED_BINCODE_TREE, &serialize(&contract_id), bincode)
        {
            error!(target: "blockchain::contractstoreoverlay", "Failed to insert bincode to WasmStore: {}", e);
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
        debug!(target: "blockchain::contractstore", "Looking up state tree for {}:{}", contract_id, tree_name);

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
    /// NOTE: this function is not used right now, we keep it for future proofing,
    ///       and its obviously untested.
    pub fn remove(&self, db: &sled::Db, contract_id: &ContractId, tree_name: &str) -> Result<()> {
        debug!(target: "blockchain::contractstore", "Removing state tree for {}:{}", contract_id, tree_name);

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

        // Remove the deleted tree from the state pointer set.
        state_pointers.retain(|x| *x != ptr);
        self.0.insert(contract_id_bytes, serialize(&state_pointers))?;

        // Drop the deleted tree from the database
        db.drop_tree(ptr)?;

        Ok(())
    }

    /// Abstraction function for fetching a `ZkBinary` and its respective `VerifyingKey`
    /// from a contract's zkas sled tree.
    pub fn get_zkas(
        &self,
        db: &sled::Db,
        contract_id: &ContractId,
        zkas_ns: &str,
    ) -> Result<(ZkBinary, VerifyingKey)> {
        debug!(target: "blockchain::contractstore", "Looking up \"{}:{}\" zkas circuit & vk", contract_id, zkas_ns);

        let zkas_tree = self.lookup(db, contract_id, SMART_CONTRACT_ZKAS_DB_NAME)?;

        let Some(zkas_bytes) = zkas_tree.get(serialize(&zkas_ns))? else {
            return Err(Error::ZkasBincodeNotFound)
        };

        // If anything in this function panics, that means corrupted data managed
        // to get into this sled tree. This should not be possible.
        let (zkbin, vkbin): (Vec<u8>, Vec<u8>) = deserialize(&zkas_bytes).unwrap();

        // The first vec is the compiled zkas binary
        let zkbin = ZkBinary::decode(&zkbin).unwrap();

        // Construct the circuit to be able to read the VerifyingKey
        let circuit = ZkCircuit::new(empty_witnesses(&zkbin).unwrap(), &zkbin);

        // The second one is the serialized VerifyingKey for it
        let mut vk_buf = Cursor::new(vkbin);
        let vk = VerifyingKey::read::<Cursor<Vec<u8>>, ZkCircuit>(&mut vk_buf, circuit).unwrap();

        Ok((zkbin, vk))
    }
}

/// Overlay structure over a [`ContractStateStore`] instance.
pub struct ContractStateStoreOverlay(SledDbOverlayPtr);

impl ContractStateStoreOverlay {
    pub fn new(overlay: &SledDbOverlayPtr) -> Result<Self> {
        overlay.lock().unwrap().open_tree(SLED_CONTRACTS_TREE)?;
        Ok(Self(overlay.clone()))
    }

    /// Try to initialize a new contract state. Contracts can create a number
    /// of trees, separated by `tree_name`, which they can then use from the
    /// smart contract API. `init()` will look into the main `ContractStateStoreOverlay`
    /// tree to check if the smart contract was already deployed, and if so
    /// it will fetch a vector of these states that were initialized. If the
    /// state was already found, this function will return an error, because
    /// in this case the handle should be fetched using `lookup()`.
    /// If the tree was not initialized previously, it will be appended to
    /// the main `ContractStateStoreOverlay` tree and a handle to it will be
    /// returned.
    pub fn init(&self, contract_id: &ContractId, tree_name: &str) -> Result<[u8; 32]> {
        debug!(target: "blockchain::contractstoreoverlay", "Initializing state overlay tree for {}:{}", contract_id, tree_name);

        let contract_id_bytes = serialize(contract_id);
        let ptr = contract_id.hash_state_id(tree_name);
        let mut lock = self.0.lock().unwrap();

        // See if there are existing state trees.
        // If not, just start with an empty vector.
        let mut state_pointers: Vec<[u8; 32]> =
            if lock.contains_key(SLED_CONTRACTS_TREE, &contract_id_bytes)? {
                let bytes = lock.get(SLED_CONTRACTS_TREE, &contract_id_bytes)?.unwrap();
                deserialize(&bytes)?
            } else {
                vec![]
            };

        // If the db was never initialized, it should not be in here.
        if state_pointers.contains(&ptr) {
            return Err(Error::ContractAlreadyInitialized)
        }

        // Now we add it so it's marked as initialized and create its tree.
        state_pointers.push(ptr);
        lock.insert(SLED_CONTRACTS_TREE, &contract_id_bytes, &serialize(&state_pointers))?;
        lock.open_tree(&ptr)?;

        Ok(ptr)
    }

    /// Do a lookup of an existing contract state. In order to succeed, the
    /// state must have been previously initialized with `init()`. If the
    /// state has been found, a handle to it will be returned. Otherwise, we
    /// return an error.
    pub fn lookup(&self, contract_id: &ContractId, tree_name: &str) -> Result<[u8; 32]> {
        debug!(target: "blockchain::contractstoreoverlay", "Looking up state tree for {}:{}", contract_id, tree_name);

        let contract_id_bytes = serialize(contract_id);
        let ptr = contract_id.hash_state_id(tree_name);
        let mut lock = self.0.lock().unwrap();

        // A guard to make sure we went through init()
        if !lock.contains_key(SLED_CONTRACTS_TREE, &contract_id_bytes)? {
            return Err(Error::ContractNotFound(contract_id.to_string()))
        }

        let state_pointers = lock.get(SLED_CONTRACTS_TREE, &contract_id_bytes)?.unwrap();
        let state_pointers: Vec<[u8; 32]> = deserialize(&state_pointers)?;

        // We assume the tree has been created already, so it should be listed
        // in this array. If not, that's an error.
        if !state_pointers.contains(&ptr) {
            return Err(Error::ContractStateNotFound)
        }

        // We open the tree and return its handle
        lock.open_tree(&ptr)?;
        Ok(ptr)
    }

    /// Abstraction function for fetching a `ZkBinary` and its respective `VerifyingKey`
    /// from a contract's zkas sled tree.
    pub fn get_zkas(
        &self,
        contract_id: &ContractId,
        zkas_ns: &str,
    ) -> Result<(ZkBinary, VerifyingKey)> {
        debug!(target: "blockchain::contractstore", "Looking up \"{}:{}\" zkas circuit & vk", contract_id, zkas_ns);

        let zkas_tree = self.lookup(contract_id, SMART_CONTRACT_ZKAS_DB_NAME)?;

        let Some(zkas_bytes) = self.0.lock().unwrap().get(&zkas_tree, &serialize(&zkas_ns))? else {
            return Err(Error::ZkasBincodeNotFound)
        };

        // If anything in this function panics, that means corrupted data managed
        // to get into this sled tree. This should not be possible.
        let (zkbin, vkbin): (Vec<u8>, Vec<u8>) = deserialize(&zkas_bytes).unwrap();

        // The first vec is the compiled zkas binary
        let zkbin = ZkBinary::decode(&zkbin).unwrap();

        // Construct the circuit to be able to read the VerifyingKey
        let circuit = ZkCircuit::new(empty_witnesses(&zkbin).unwrap(), &zkbin);

        // The second one is the serialized VerifyingKey for it
        let mut vk_buf = Cursor::new(vkbin);
        let vk = VerifyingKey::read::<Cursor<Vec<u8>>, ZkCircuit>(&mut vk_buf, circuit).unwrap();

        Ok((zkbin, vk))
    }
}
