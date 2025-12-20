/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use std::{collections::BTreeMap, io::Cursor};

use darkfi_sdk::{
    crypto::contract_id::{
        ContractId, NATIVE_CONTRACT_IDS_BYTES, NATIVE_CONTRACT_ZKAS_DB_NAMES,
        SMART_CONTRACT_MONOTREE_DB_NAME, SMART_CONTRACT_ZKAS_DB_NAME,
    },
    monotree::{MemoryDb, Monotree, SledOverlayDb, SledTreeDb, EMPTY_HASH},
};
use darkfi_serial::{deserialize, serialize};
use sled_overlay::{serial::parse_record, sled};
use tracing::{debug, error};

use crate::{
    zk::{empty_witnesses, VerifyingKey, ZkCircuit},
    zkas::ZkBinary,
    Error, Result,
};

use super::SledDbOverlayPtr;

pub const SLED_CONTRACTS_TREE: &[u8] = b"_contracts";
pub const SLED_CONTRACTS_TREES_TREE: &[u8] = b"_contracts_trees";
pub const SLED_BINCODE_TREE: &[u8] = b"_wasm_bincode";

/// The `ContractStore` is a structure representing all `sled` trees related
/// to storing the blockchain's contracts information.
#[derive(Clone)]
pub struct ContractStore {
    /// The `sled` tree storing the wasm bincode for deployed contracts.
    /// The layout looks like this:
    /// ```plaintext
    ///  tree: "_wasm_bincode"
    ///   key: ContractId
    /// value: Vec<u8>
    pub wasm: sled::Tree,
    /// The `sled` tree storing the pointers to contracts' databases.
    /// See the rustdoc for the impl functions for more info.
    /// The layout looks like this:
    /// ```plaintext
    ///  tree: "_contracts"
    ///   key: ContractId
    /// value: Vec<blake3(ContractId || tree_name)>
    /// ```
    /// These values get mutated with `init()` and `remove()`.
    pub state: sled::Tree,
    /// The `sled` tree storing the inverse pointers to contracts'
    /// databases. See the rustdoc for the impl functions for more
    /// info.
    /// The layout looks like this:
    /// ```plaintext
    ///  tree: "_contracts_trees"
    ///   key: blake3(ContractId || tree_name)
    /// value: ContractId
    /// ```
    /// These values get mutated with `init()` and `remove()`.
    pub state_trees: sled::Tree,
}

impl ContractStore {
    /// Opens a new or existing `ContractStore` on the given sled database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let wasm = db.open_tree(SLED_BINCODE_TREE)?;
        let state = db.open_tree(SLED_CONTRACTS_TREE)?;
        let state_trees = db.open_tree(SLED_CONTRACTS_TREES_TREE)?;
        Ok(Self { wasm, state, state_trees })
    }

    /// Fetches the bincode for a given ContractId from the store's wasm tree.
    /// Returns an error if the bincode is not found.
    pub fn get(&self, contract_id: ContractId) -> Result<Vec<u8>> {
        if let Some(bincode) = self.wasm.get(serialize(&contract_id))? {
            return Ok(bincode.to_vec())
        }

        Err(Error::WasmBincodeNotFound)
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
        debug!(target: "blockchain::contractstore", "Looking up state tree for {contract_id}:{tree_name}");

        // A guard to make sure we went through init()
        let contract_id_bytes = serialize(contract_id);
        if !self.state.contains_key(&contract_id_bytes)? {
            return Err(Error::ContractNotFound(contract_id.to_string()))
        }

        let state_pointers = self.state.get(&contract_id_bytes)?.unwrap();
        let state_pointers: Vec<[u8; 32]> = deserialize(&state_pointers)?;

        // We assume the tree has been created already, so it should be listed
        // in this array. If not, that's an error.
        let ptr = contract_id.hash_state_id(tree_name);
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
        debug!(target: "blockchain::contractstore", "Removing state tree for {contract_id}:{tree_name}");

        // A guard to make sure we went through init()
        let contract_id_bytes = serialize(contract_id);
        if !self.state.contains_key(&contract_id_bytes)? {
            return Err(Error::ContractNotFound(contract_id.to_string()))
        }

        let state_pointers = self.state.get(&contract_id_bytes)?.unwrap();
        let mut state_pointers: Vec<[u8; 32]> = deserialize(&state_pointers)?;

        // We assume the tree has been created already, so it should be listed
        // in this array. If not, that's an error.
        let ptr = contract_id.hash_state_id(tree_name);
        if !state_pointers.contains(&ptr) {
            return Err(Error::ContractStateNotFound)
        }
        if !self.state_trees.contains_key(ptr)? {
            return Err(Error::ContractStateNotFound)
        }

        // Remove the deleted tree from the state pointer set.
        state_pointers.retain(|x| *x != ptr);
        self.state.insert(contract_id_bytes, serialize(&state_pointers))?;
        self.state_trees.remove(ptr)?;

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
        debug!(target: "blockchain::contractstore", "Looking up \"{contract_id}:{zkas_ns}\" zkas circuit & vk");

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

    /// Retrieve all wasm bincodes from the store's wasm tree in the form
    /// of a tuple (`contract_id`, `bincode`).
    /// Be careful as this will try to load everything in memory.
    pub fn get_all_wasm(&self) -> Result<Vec<(ContractId, Vec<u8>)>> {
        let mut bincodes = vec![];

        for bincode in self.wasm.iter() {
            let bincode = bincode.unwrap();
            let contract_id = deserialize(&bincode.0)?;
            bincodes.push((contract_id, bincode.1.to_vec()));
        }

        Ok(bincodes)
    }

    /// Retrieve all contract states from the store's state tree in the
    /// form of a tuple (`contract_id`, `state_hashes`).
    /// Be careful as this will try to load everything in memory.
    pub fn get_all_states(&self) -> Result<Vec<(ContractId, Vec<blake3::Hash>)>> {
        let mut contracts = vec![];

        for contract in self.state.iter() {
            contracts.push(parse_record(contract.unwrap())?);
        }

        Ok(contracts)
    }

    /// Retrieve provided key value bytes from a contract's zkas sled tree.
    pub fn get_state_tree_value(
        &self,
        db: &sled::Db,
        contract_id: &ContractId,
        tree_name: &str,
        key: &[u8],
    ) -> Result<Vec<u8>> {
        debug!(target: "blockchain::contractstore", "Looking up state tree value for {contract_id}:{tree_name}");

        // Grab the state tree
        let state_tree = self.lookup(db, contract_id, tree_name)?;

        // Grab the key value
        match state_tree.get(key)? {
            Some(value) => Ok(value.to_vec()),
            None => Err(Error::DatabaseError(format!(
                "State tree {contract_id}:{tree_name} doesn't contain key: {key:?}"
            ))),
        }
    }

    /// Retrieve all records from a contract's zkas sled tree, as a `BTreeMap`.
    /// Be careful as this will try to load everything in memory.
    pub fn get_state_tree_records(
        &self,
        db: &sled::Db,
        contract_id: &ContractId,
        tree_name: &str,
    ) -> Result<BTreeMap<Vec<u8>, Vec<u8>>> {
        debug!(target: "blockchain::contractstore", "Looking up state tree records for {contract_id}:{tree_name}");

        // Grab the state tree
        let state_tree = self.lookup(db, contract_id, tree_name)?;

        // Retrieve its records
        let mut ret = BTreeMap::new();
        for record in state_tree.iter() {
            let (key, value) = record.unwrap();
            ret.insert(key.to_vec(), value.to_vec());
        }

        Ok(ret)
    }

    /// Generate a Monotree(SMT) containing all contracts states
    /// roots, along with the wasm bincodes monotree root.
    ///
    /// Note: native contracts zkas tree and wasm bincodes are excluded.
    pub fn get_state_monotree(&self, db: &sled::Db) -> Result<Monotree<MemoryDb>> {
        // Initialize the monotree
        let mut root = None;
        let monotree_db = MemoryDb::new();
        let mut tree = Monotree::new(monotree_db);

        // Iterate over current contracts states records
        for state_record in self.state.iter() {
            // Grab its monotree pointer
            let (contract_id, state_pointers): (ContractId, Vec<[u8; 32]>) =
                parse_record(state_record?)?;
            let state_monotree_ptr = contract_id.hash_state_id(SMART_CONTRACT_MONOTREE_DB_NAME);

            // Check it exists
            if !state_pointers.contains(&state_monotree_ptr) {
                return Err(Error::ContractStateNotFound)
            }
            if !self.state_trees.contains_key(state_monotree_ptr)? {
                return Err(Error::ContractStateNotFound)
            }

            // Grab its monotree
            let state_tree = db.open_tree(state_monotree_ptr)?;
            let state_monotree_db = SledTreeDb::new(&state_tree);
            let state_monotree = Monotree::new(state_monotree_db);

            // Insert its root to the global monotree
            let state_monotree_root = match state_monotree.get_headroot()? {
                Some(hash) => hash,
                None => *EMPTY_HASH,
            };
            root = tree.insert(root.as_ref(), &contract_id.to_bytes(), &state_monotree_root)?;
        }

        // Iterate over current contracts wasm bincodes to compute its monotree root
        let mut wasm_monotree_root = None;
        let wasm_monotree_db = MemoryDb::new();
        let mut wasm_monotree = Monotree::new(wasm_monotree_db);
        for record in self.wasm.iter() {
            let (key, value) = record?;

            // Skip native ones
            if NATIVE_CONTRACT_IDS_BYTES.contains(&deserialize(&key)?) {
                continue
            }

            // Insert record
            wasm_monotree_root = wasm_monotree.insert(
                wasm_monotree_root.as_ref(),
                blake3::hash(&key).as_bytes(),
                blake3::hash(&value).as_bytes(),
            )?;
        }

        // Insert wasm bincodes root to the global monotree
        let wasm_monotree_root = match wasm_monotree_root {
            Some(hash) => hash,
            None => *EMPTY_HASH,
        };
        root = tree.insert(
            root.as_ref(),
            blake3::hash(SLED_BINCODE_TREE).as_bytes(),
            &wasm_monotree_root,
        )?;
        tree.set_headroot(root.as_ref());

        Ok(tree)
    }
}

/// Overlay structure over a [`ContractStore`] instance.
pub struct ContractStoreOverlay(SledDbOverlayPtr);

impl ContractStoreOverlay {
    pub fn new(overlay: &SledDbOverlayPtr) -> Result<Self> {
        overlay.lock().unwrap().open_tree(SLED_BINCODE_TREE, true)?;
        overlay.lock().unwrap().open_tree(SLED_CONTRACTS_TREE, true)?;
        overlay.lock().unwrap().open_tree(SLED_CONTRACTS_TREES_TREE, true)?;
        Ok(Self(overlay.clone()))
    }

    /// Fetches the bincode for a given ContractId from the overlay's wasm tree.
    /// Returns an error if the bincode is not found.
    pub fn get(&self, contract_id: ContractId) -> Result<Vec<u8>> {
        if let Some(bincode) =
            self.0.lock().unwrap().get(SLED_BINCODE_TREE, &serialize(&contract_id))?
        {
            return Ok(bincode.to_vec())
        }

        Err(Error::WasmBincodeNotFound)
    }

    /// Inserts or replaces the bincode for a given ContractId into the overlay's
    /// wasm tree.
    pub fn insert(&self, contract_id: ContractId, bincode: &[u8]) -> Result<()> {
        if let Err(e) =
            self.0.lock().unwrap().insert(SLED_BINCODE_TREE, &serialize(&contract_id), bincode)
        {
            error!(target: "blockchain::contractstoreoverlay", "Failed to insert bincode to Wasm tree: {e}");
            return Err(e.into())
        }

        Ok(())
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
        debug!(target: "blockchain::contractstoreoverlay", "Initializing state overlay tree for {contract_id}:{tree_name}");
        let mut lock = self.0.lock().unwrap();

        // See if there are existing state trees.
        // If not, just start with an empty vector.
        let contract_id_bytes = serialize(contract_id);
        let mut state_pointers: Vec<[u8; 32]> =
            if lock.contains_key(SLED_CONTRACTS_TREE, &contract_id_bytes)? {
                let bytes = lock.get(SLED_CONTRACTS_TREE, &contract_id_bytes)?.unwrap();
                deserialize(&bytes)?
            } else {
                vec![]
            };

        // If the db was never initialized, it should not be in here.
        let ptr = contract_id.hash_state_id(tree_name);
        if state_pointers.contains(&ptr) {
            return Err(Error::ContractAlreadyInitialized)
        }

        // Now we add it so it's marked as initialized and create its tree.
        state_pointers.push(ptr);
        lock.insert(SLED_CONTRACTS_TREE, &contract_id_bytes, &serialize(&state_pointers))?;
        lock.insert(SLED_CONTRACTS_TREES_TREE, &ptr, &contract_id_bytes)?;
        lock.open_tree(&ptr, false)?;

        Ok(ptr)
    }

    /// Do a lookup of an existing contract state. In order to succeed, the
    /// state must have been previously initialized with `init()`. If the
    /// state has been found, a handle to it will be returned. Otherwise, we
    /// return an error.
    pub fn lookup(&self, contract_id: &ContractId, tree_name: &str) -> Result<[u8; 32]> {
        debug!(target: "blockchain::contractstoreoverlay", "Looking up state tree for {contract_id}:{tree_name}");
        let mut lock = self.0.lock().unwrap();

        // A guard to make sure we went through init()
        let contract_id_bytes = serialize(contract_id);
        if !lock.contains_key(SLED_CONTRACTS_TREE, &contract_id_bytes)? {
            return Err(Error::ContractNotFound(contract_id.to_string()))
        }

        let state_pointers = lock.get(SLED_CONTRACTS_TREE, &contract_id_bytes)?.unwrap();
        let state_pointers: Vec<[u8; 32]> = deserialize(&state_pointers)?;

        // We assume the tree has been created already, so it should be listed
        // in this array. If not, that's an error.
        let ptr = contract_id.hash_state_id(tree_name);
        if !state_pointers.contains(&ptr) {
            return Err(Error::ContractStateNotFound)
        }
        if !lock.contains_key(SLED_CONTRACTS_TREES_TREE, &ptr)? {
            return Err(Error::ContractStateNotFound)
        }

        // We open the tree and return its handle
        lock.open_tree(&ptr, false)?;
        Ok(ptr)
    }

    /// Abstraction function for fetching a `ZkBinary` and its respective `VerifyingKey`
    /// from a contract's zkas sled tree.
    pub fn get_zkas(
        &self,
        contract_id: &ContractId,
        zkas_ns: &str,
    ) -> Result<(ZkBinary, VerifyingKey)> {
        debug!(target: "blockchain::contractstore", "Looking up \"{contract_id}:{zkas_ns}\" zkas circuit & vk");

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

    /// Generate a Monotree(SMT) containing all contracts states
    /// roots, along with the wasm bincodes monotree roots.
    /// Be carefull as this will open all states monotrees in the overlay.
    ///
    /// Note: native contracts zkas tree and wasm bincodes are excluded.
    pub fn get_state_monotree(&self) -> Result<Monotree<MemoryDb>> {
        let mut lock = self.0.lock().unwrap();

        // Grab all states monotrees pointers
        let mut states_monotrees_pointers = vec![];
        for state_record in lock.iter(SLED_CONTRACTS_TREE)? {
            // Grab its monotree pointer
            let (contract_id, state_pointers): (ContractId, Vec<[u8; 32]>) =
                parse_record(state_record?)?;
            let state_monotree_ptr = contract_id.hash_state_id(SMART_CONTRACT_MONOTREE_DB_NAME);

            // Check it exists
            if !state_pointers.contains(&state_monotree_ptr) {
                return Err(Error::ContractStateNotFound)
            }
            if !lock.contains_key(SLED_CONTRACTS_TREES_TREE, &state_monotree_ptr)? {
                return Err(Error::ContractStateNotFound)
            }

            states_monotrees_pointers.push((contract_id, state_monotree_ptr));
        }

        // Initialize the monotree
        let mut root = None;
        let monotree_db = MemoryDb::new();
        let mut tree = Monotree::new(monotree_db);

        // Iterate over contract states monotrees pointers
        for (contract_id, state_monotree_ptr) in states_monotrees_pointers {
            // Grab its monotree
            let state_monotree_db = SledOverlayDb::new(&mut lock, &state_monotree_ptr)?;
            let state_monotree = Monotree::new(state_monotree_db);

            // Insert its root to the global monotree
            let state_monotree_root = match state_monotree.get_headroot()? {
                Some(hash) => hash,
                None => *EMPTY_HASH,
            };
            root = tree.insert(root.as_ref(), &contract_id.to_bytes(), &state_monotree_root)?;
        }

        // Iterate over current contracts wasm bincodes to compute its monotree root
        let mut wasm_monotree_root = None;
        let wasm_monotree_db = MemoryDb::new();
        let mut wasm_monotree = Monotree::new(wasm_monotree_db);
        for record in lock.iter(SLED_BINCODE_TREE)? {
            let (key, value) = record?;

            // Skip native ones
            if NATIVE_CONTRACT_IDS_BYTES.contains(&deserialize(&key)?) {
                continue
            }

            // Insert record
            wasm_monotree_root = wasm_monotree.insert(
                wasm_monotree_root.as_ref(),
                blake3::hash(&key).as_bytes(),
                blake3::hash(&value).as_bytes(),
            )?;
        }

        // Insert wasm bincodes root to the global monotree
        let wasm_monotree_root = match wasm_monotree_root {
            Some(hash) => hash,
            None => *EMPTY_HASH,
        };
        root = tree.insert(
            root.as_ref(),
            blake3::hash(SLED_BINCODE_TREE).as_bytes(),
            &wasm_monotree_root,
        )?;
        tree.set_headroot(root.as_ref());
        drop(lock);

        // Update the monotree to the latest overlay changes
        self.update_state_monotree(&mut tree)?;

        Ok(tree)
    }

    /// Retrieve all updated contracts states and wasm bincodes
    /// monotrees roots and update their records in the provided
    /// Monotree(SMT).
    ///
    /// Note: native contracts zkas tree and wasm bincodes are excluded.
    pub fn update_state_monotree(&self, tree: &mut Monotree<MemoryDb>) -> Result<()> {
        let mut lock = self.0.lock().unwrap();

        // Iterate over overlay's caches
        let mut root = tree.get_headroot()?;
        let mut states_monotrees_pointers = vec![];
        for (state_key, state_cache) in &lock.state.caches {
            // Check if that cache is a contract state one.
            // Overlay protected trees are all the native/non-contract ones.
            if !lock.state.protected_tree_names.contains(state_key) {
                let state_key = deserialize(state_key)?;

                // Skip native zkas tree
                if NATIVE_CONTRACT_ZKAS_DB_NAMES.contains(&state_key) {
                    continue
                }

                // Grab its contract id
                let Some(record) = lock.get(SLED_CONTRACTS_TREES_TREE, &state_key)? else {
                    return Err(Error::ContractStateNotFound)
                };
                let contract_id: ContractId = deserialize(&record)?;

                // Skip the actual monotree state cache
                let state_monotree_ptr = contract_id.hash_state_id(SMART_CONTRACT_MONOTREE_DB_NAME);
                if state_monotree_ptr == state_key {
                    continue
                }

                // Grab its monotree pointer and its cache state
                states_monotrees_pointers.push((
                    contract_id,
                    state_monotree_ptr,
                    state_cache.state.removed.clone(),
                    state_cache.state.cache.clone(),
                ));
                continue
            }

            // Skip if its not the wasm bincodes cache
            if state_key != SLED_BINCODE_TREE {
                continue
            }

            // Check if wasm bincodes cache is updated
            if state_cache.state.cache.is_empty() && state_cache.state.removed.is_empty() {
                continue
            }

            // Iterate over current contracts wasm bincodes to compute its monotree root
            debug!(target: "blockchain::contractstore::update_state_monotree", "Updating wasm bincodes monotree...");
            let mut wasm_monotree_root = None;
            let wasm_monotree_db = MemoryDb::new();
            let mut wasm_monotree = Monotree::new(wasm_monotree_db);
            for record in state_cache.iter() {
                let (key, value) = record?;

                // Skip native ones
                if NATIVE_CONTRACT_IDS_BYTES.contains(&deserialize(&key)?) {
                    continue
                }

                // Insert record
                let key = blake3::hash(&key);
                let value = blake3::hash(&value);
                debug!(target: "blockchain::contractstore::update_state_monotree", "Inserting key {key} with value: {value}");
                wasm_monotree_root = wasm_monotree.insert(
                    wasm_monotree_root.as_ref(),
                    key.as_bytes(),
                    value.as_bytes(),
                )?;
            }

            // Insert wasm bincodes root to the global monotree
            let wasm_monotree_root = match wasm_monotree_root {
                Some(hash) => hash,
                None => *EMPTY_HASH,
            };
            debug!(target: "blockchain::contractstore::update_state_monotree", "New root: {}", blake3::Hash::from(wasm_monotree_root));
            root = tree.insert(
                root.as_ref(),
                blake3::hash(SLED_BINCODE_TREE).as_bytes(),
                &wasm_monotree_root,
            )?;
            debug!(target: "blockchain::contractstore::update_state_monotree", "New global root: {}", blake3::Hash::from(root.unwrap()));
        }

        // Iterate over contract states monotrees pointers
        for (contract_id, state_monotree_ptr, removed, cache) in states_monotrees_pointers {
            debug!(target: "blockchain::contractstore::update_state_monotree", "Updating monotree for contract: {contract_id}");
            let state_monotree_db = SledOverlayDb::new(&mut lock, &state_monotree_ptr)?;
            let mut state_monotree = Monotree::new(state_monotree_db);
            let mut state_monotree_root = state_monotree.get_headroot()?;

            // Remove dropped records
            for key in &removed {
                let key = blake3::hash(key);
                debug!(target: "blockchain::contractstore::update_state_monotree", "Removed key: {key}");
                state_monotree_root =
                    state_monotree.remove(state_monotree_root.as_ref(), key.as_bytes())?;
            }

            // Update or insert new records
            for (key, value) in &cache {
                let key = blake3::hash(key);
                let value = blake3::hash(value);
                debug!(target: "blockchain::contractstore::update_state_monotree", "Updating key {key} with value: {value}");
                state_monotree_root = state_monotree.insert(
                    state_monotree_root.as_ref(),
                    key.as_bytes(),
                    value.as_bytes(),
                )?;
            }
            state_monotree.set_headroot(state_monotree_root.as_ref());

            // Insert its root to the global monotree
            let state_monotree_root = match state_monotree_root {
                Some(hash) => hash,
                None => *EMPTY_HASH,
            };
            debug!(target: "blockchain::contractstore::update_state_monotree", "New root: {}", blake3::Hash::from(state_monotree_root));
            root = tree.insert(root.as_ref(), &contract_id.to_bytes(), &state_monotree_root)?;
            debug!(target: "blockchain::contractstore::update_state_monotree", "New global root: {}", blake3::Hash::from(root.unwrap()));
        }
        tree.set_headroot(root.as_ref());

        Ok(())
    }
}
