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
use sled_overlay::{serial::parse_record, sled, SledDbOverlayStateDiff};
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
        debug!(target: "blockchain::contractstore::lookup", "Looking up state tree for {contract_id}:{tree_name}");

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
        debug!(target: "blockchain::contractstore::remove", "Removing state tree for {contract_id}:{tree_name}");

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
        debug!(target: "blockchain::contractstore::get_zkas", "Looking up \"{contract_id}:{zkas_ns}\" zkas circuit & vk");

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
        debug!(target: "blockchain::contractstore::get_state_tree_value", "Looking up state tree value for {contract_id}:{tree_name}");

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
        debug!(target: "blockchain::contractstore::get_state_tree_records", "Looking up state tree records for {contract_id}:{tree_name}");

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
        debug!(target: "blockchain::contractstore::get_state_monotree", "Initializing global monotree...");
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
            debug!(target: "blockchain::contractstore::get_state_monotree", "Contract {contract_id} root: {}", blake3::Hash::from(state_monotree_root));
            root = tree.insert(root.as_ref(), &contract_id.to_bytes(), &state_monotree_root)?;
            debug!(target: "blockchain::contractstore::get_state_monotree", "New global root: {}", blake3::Hash::from(root.unwrap()));
        }

        // Iterate over current contracts wasm bincodes to compute its monotree root
        debug!(target: "blockchain::contractstore::get_state_monotree", "Initializing wasm bincodes monotree...");
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
            let key = blake3::hash(&key);
            let value = blake3::hash(&value);
            debug!(target: "blockchain::contractstore::get_state_monotree", "Inserting key {key} with value: {value}");
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
        debug!(target: "blockchain::contractstore::get_state_monotree", "New root: {}", blake3::Hash::from(wasm_monotree_root));
        root = tree.insert(
            root.as_ref(),
            blake3::hash(SLED_BINCODE_TREE).as_bytes(),
            &wasm_monotree_root,
        )?;
        debug!(target: "blockchain::contractstore::get_state_monotree", "New global root: {}", blake3::Hash::from(root.unwrap()));
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
            error!(target: "blockchain::contractstoreoverlay::insert", "Failed to insert bincode to Wasm tree: {e}");
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
        debug!(target: "blockchain::contractstoreoverlay::init", "Initializing state overlay tree for {contract_id}:{tree_name}");
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
        debug!(target: "blockchain::contractstoreoverlay::lookup", "Looking up state tree for {contract_id}:{tree_name}");
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
        debug!(target: "blockchain::contractstoreoverlay::get_zkas", "Looking up \"{contract_id}:{zkas_ns}\" zkas circuit & vk");

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
    /// Be carefull as this will open all states monotrees in the
    /// overlay, and all contract state trees if their monotrees
    /// need rebuild.
    ///
    /// Note: native contracts zkas tree and wasm bincodes are
    /// excluded.
    pub fn get_state_monotree(&self) -> Result<Monotree<MemoryDb>> {
        let mut lock = self.0.lock().unwrap();

        // Grab all states pointers
        debug!(target: "blockchain::contractstoreoverlay::get_state_monotree", "Retrieving state pointers...");
        let mut states_monotrees_pointers = vec![];
        for state_record in lock.iter(SLED_CONTRACTS_TREE)? {
            // Grab its monotree pointer
            let (contract_id, mut state_pointers): (ContractId, Vec<[u8; 32]>) =
                parse_record(state_record?)?;
            let state_monotree_ptr = contract_id.hash_state_id(SMART_CONTRACT_MONOTREE_DB_NAME);

            // Check it exists
            if !state_pointers.contains(&state_monotree_ptr) {
                return Err(Error::ContractStateNotFound)
            }
            if !lock.contains_key(SLED_CONTRACTS_TREES_TREE, &state_monotree_ptr)? {
                return Err(Error::ContractStateNotFound)
            }

            // Skip native zkas trees
            if NATIVE_CONTRACT_IDS_BYTES.contains(&contract_id.to_bytes()) {
                state_pointers.retain(|ptr| !NATIVE_CONTRACT_ZKAS_DB_NAMES.contains(ptr));
            }

            states_monotrees_pointers.push((contract_id, state_pointers, state_monotree_ptr));
        }

        // Initialize the monotree
        debug!(target: "blockchain::contractstoreoverlay::get_state_monotree", "Initializing global monotree...");
        let mut root = None;
        let monotree_db = MemoryDb::new();
        let mut tree = Monotree::new(monotree_db);

        // Iterate over contract states monotrees pointers
        for (contract_id, state_pointers, state_monotree_ptr) in states_monotrees_pointers {
            // Iterate over contract state pointers to find their
            // inserted keys. If any of them has dropped keys, we must
            // rebuild the contract state monotree.
            debug!(target: "blockchain::contractstoreoverlay::get_state_monotree", "Updating monotree for contract: {contract_id}");
            let mut rebuild = false;
            let mut inserts = vec![];
            'outer: for state_ptr in &state_pointers {
                // Skip the actual monotree state pointer
                if state_ptr == &state_monotree_ptr {
                    continue
                }

                // Look for it in the overlay
                for (state_key, state_cache) in &lock.state.caches {
                    if state_key != state_ptr {
                        continue
                    }

                    // Check if it has dropped keys
                    if !state_cache.state.removed.is_empty() {
                        rebuild = true;
                        break 'outer
                    }

                    // Grab the new/updated keys
                    for (key, value) in &state_cache.state.cache {
                        let key = blake3::hash(key);
                        let value = blake3::hash(value);
                        inserts.push((key, value))
                    }
                    break
                }
            }

            // Check if we need to rebuild it
            if rebuild {
                // Iterate over all contract states to grab the monotree keys
                debug!(target: "blockchain::contractstoreoverlay::get_state_monotree", "Rebuilding monotree...");
                inserts = vec![];
                for state_ptr in state_pointers {
                    // Open the contract state
                    lock.open_tree(&state_ptr, false)?;

                    // If the pointer is the monotree one, clear it
                    if state_ptr == state_monotree_ptr {
                        lock.clear(&state_ptr)?;
                        continue
                    }

                    // Grab all its keys
                    for record in lock.iter(&state_ptr)? {
                        let (key, value) = record?;
                        let key = blake3::hash(&key);
                        let value = blake3::hash(&value);
                        inserts.push((key, value))
                    }
                }
            }

            // Grab its monotree
            let state_monotree_db = SledOverlayDb::new(&mut lock, &state_monotree_ptr)?;
            let mut state_monotree = Monotree::new(state_monotree_db);
            let mut state_monotree_root =
                if rebuild { None } else { state_monotree.get_headroot()? };
            let state_monotree_root_str = match state_monotree_root {
                Some(hash) => blake3::Hash::from(hash),
                None => blake3::Hash::from(*EMPTY_HASH),
            };
            debug!(target: "blockchain::contractstoreoverlay::get_state_monotree", "Current root: {state_monotree_root_str}");

            // Update or insert new records
            for (key, value) in &inserts {
                debug!(target: "blockchain::contractstoreoverlay::get_state_monotree", "Inserting key {key} with value: {value}");
                state_monotree_root = state_monotree.insert(
                    state_monotree_root.as_ref(),
                    key.as_bytes(),
                    value.as_bytes(),
                )?;
            }

            // Set root
            state_monotree.set_headroot(state_monotree_root.as_ref());

            // Insert its root to the global monotree
            let state_monotree_root = match state_monotree_root {
                Some(hash) => hash,
                None => *EMPTY_HASH,
            };
            debug!(target: "blockchain::contractstoreoverlay::get_state_monotree", "New root: {}", blake3::Hash::from(state_monotree_root));
            root = tree.insert(root.as_ref(), &contract_id.to_bytes(), &state_monotree_root)?;
        }

        // Iterate over current contracts wasm bincodes to compute its monotree root
        debug!(target: "blockchain::contractstoreoverlay::get_state_monotree", "Initializing wasm bincodes monotree...");
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
            let key = blake3::hash(&key);
            let value = blake3::hash(&value);
            debug!(target: "blockchain::contractstoreoverlay::get_state_monotree", "Inserting key {key} with value: {value}");
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
        debug!(target: "blockchain::contractstoreoverlay::get_state_monotree", "New root: {}", blake3::Hash::from(wasm_monotree_root));
        root = tree.insert(
            root.as_ref(),
            blake3::hash(SLED_BINCODE_TREE).as_bytes(),
            &wasm_monotree_root,
        )?;
        debug!(target: "blockchain::contractstoreoverlay::get_state_monotree", "New global root: {}", blake3::Hash::from(root.unwrap()));
        tree.set_headroot(root.as_ref());

        Ok(tree)
    }

    /// Retrieve all updated contracts states and wasm bincodes from
    /// provided overlay diff, update their monotrees in the overlay
    /// and their records in the provided Monotree(SMT).
    ///
    /// Note: native contracts zkas tree and wasm bincodes are
    /// excluded.
    pub fn update_state_monotree(
        &self,
        diff: &SledDbOverlayStateDiff,
        tree: &mut Monotree<MemoryDb>,
    ) -> Result<()> {
        // If a contract was dropped, we must rebuild the monotree from
        // scratch.
        if let Some((state_cache, _)) = diff.caches.get(SLED_CONTRACTS_TREE) {
            if !state_cache.removed.is_empty() {
                debug!(target: "blockchain::contractstoreoverlay::update_state_monotree", "Rebuilding global monotree...");
                *tree = self.get_state_monotree()?;
                return Ok(());
            }
        }

        // Grab lock over the overlay
        let mut lock = self.0.lock().unwrap();
        debug!(target: "blockchain::contractstoreoverlay::update_state_monotree", "Retrieving contracts updates...");

        // If a contract tree was dropped, we must rebuild its monotree
        // from scratch.
        let mut contracts_updates = BTreeMap::new();
        if let Some((state_cache, _)) = diff.caches.get(SLED_CONTRACTS_TREES_TREE) {
            // Mark all the contracts of dropped trees for rebuild
            for contract_id_bytes in state_cache.removed.values() {
                contracts_updates.insert(contract_id_bytes.clone(), (true, vec![]));
            }
        }

        // Iterate over diff caches to find all contracts updates
        for (state_key, state_cache) in &diff.caches {
            // Check if that cache is not a contract state one.
            // Overlay protected trees are all the native/non-contract
            // ones.
            if lock.state.protected_tree_names.contains(state_key) {
                continue
            }

            // Grab the actual state key
            let state_key = deserialize(state_key)?;

            // Skip native zkas tree
            if NATIVE_CONTRACT_ZKAS_DB_NAMES.contains(&state_key) {
                continue
            }

            // Grab its contract id
            let Some(contract_id_bytes) = lock.get(SLED_CONTRACTS_TREES_TREE, &state_key)? else {
                return Err(Error::ContractStateNotFound)
            };
            let contract_id: ContractId = deserialize(&contract_id_bytes)?;

            // Skip the actual monotree state cache
            let state_monotree_ptr = contract_id.hash_state_id(SMART_CONTRACT_MONOTREE_DB_NAME);
            if state_monotree_ptr == state_key {
                continue
            }

            // Grab its record from the map
            let (rebuild, mut inserts) = match contracts_updates.get(&contract_id_bytes) {
                Some(r) => r.clone(),
                None => (false, vec![]),
            };

            // Check if the contract monotree is already marked for
            // rebuild.
            if rebuild {
                continue
            }

            // If records have been dropped, mark the contract monotree
            // for rebuild.
            if !state_cache.0.removed.is_empty() {
                contracts_updates.insert(contract_id_bytes, (true, vec![]));
                continue
            }

            // Grab the new/updated keys
            for (key, (_, value)) in &state_cache.0.cache {
                let key = blake3::hash(key);
                let value = blake3::hash(value);
                inserts.push((key, value))
            }
            contracts_updates.insert(contract_id_bytes, (rebuild, inserts));
        }

        // Grab current root
        let mut root = tree.get_headroot()?;
        let root_str = match root {
            Some(hash) => blake3::Hash::from(hash),
            None => blake3::Hash::from(*EMPTY_HASH),
        };
        debug!(target: "blockchain::contractstoreoverlay::update_state_monotree", "Updating global monotree with root: {root_str}");

        // Iterate over contracts updates
        for (contract_id_bytes, (rebuild, mut inserts)) in contracts_updates {
            let contract_id: ContractId = deserialize(&contract_id_bytes)?;
            let state_monotree_ptr = contract_id.hash_state_id(SMART_CONTRACT_MONOTREE_DB_NAME);
            debug!(target: "blockchain::contractstoreoverlay::update_state_monotree", "Updating monotree for contract: {contract_id}");

            // Check if we need to rebuild it
            if rebuild {
                // Grab its state pointers
                let state_pointers = lock.get(SLED_CONTRACTS_TREE, &contract_id_bytes)?.unwrap();
                let mut state_pointers: Vec<[u8; 32]> = deserialize(&state_pointers)?;

                // Skip native zkas trees
                if NATIVE_CONTRACT_IDS_BYTES.contains(&contract_id.to_bytes()) {
                    state_pointers.retain(|ptr| !NATIVE_CONTRACT_ZKAS_DB_NAMES.contains(ptr));
                }

                // Iterate over all contract states to grab the monotree keys
                debug!(target: "blockchain::contractstoreoverlay::update_state_monotree", "Rebuilding monotree...");
                for state_ptr in state_pointers {
                    // Open the contract state
                    lock.open_tree(&state_ptr, false)?;

                    // If the pointer is the monotree one, clear it
                    if state_ptr == state_monotree_ptr {
                        lock.clear(&state_ptr)?;
                        continue
                    }

                    // Grab all its keys
                    for record in lock.iter(&state_ptr)? {
                        let (key, value) = record?;
                        let key = blake3::hash(&key);
                        let value = blake3::hash(&value);
                        inserts.push((key, value))
                    }
                }
            }

            // Grab its monotree
            let state_monotree_db = SledOverlayDb::new(&mut lock, &state_monotree_ptr)?;
            let mut state_monotree = Monotree::new(state_monotree_db);
            let mut state_monotree_root =
                if rebuild { None } else { state_monotree.get_headroot()? };
            let state_monotree_root_str = match state_monotree_root {
                Some(hash) => blake3::Hash::from(hash),
                None => blake3::Hash::from(*EMPTY_HASH),
            };
            debug!(target: "blockchain::contractstoreoverlay::update_state_monotree", "Current root: {state_monotree_root_str}");

            // Update or insert new records
            for (key, value) in &inserts {
                debug!(target: "blockchain::contractstoreoverlay::update_state_monotree", "Inserting key {key} with value: {value}");
                state_monotree_root = state_monotree.insert(
                    state_monotree_root.as_ref(),
                    key.as_bytes(),
                    value.as_bytes(),
                )?;
            }

            // Set root
            state_monotree.set_headroot(state_monotree_root.as_ref());

            // Insert its root to the global monotree
            let state_monotree_root = match state_monotree_root {
                Some(hash) => hash,
                None => *EMPTY_HASH,
            };
            debug!(target: "blockchain::contractstoreoverlay::update_state_monotree", "New root: {}", blake3::Hash::from(state_monotree_root));
            root = tree.insert(root.as_ref(), &contract_id.to_bytes(), &state_monotree_root)?;
        }

        // Check if wasm bincodes cache exists
        let Some((state_cache, _)) = diff.caches.get(SLED_CONTRACTS_TREES_TREE) else {
            debug!(target: "blockchain::contractstoreoverlay::update_state_monotree", "New global root: {}", blake3::Hash::from(root.unwrap()));
            tree.set_headroot(root.as_ref());
            return Ok(())
        };

        // Check if wasm bincodes cache is updated
        if state_cache.cache.is_empty() && state_cache.removed.is_empty() {
            debug!(target: "blockchain::contractstoreoverlay::update_state_monotree", "New global root: {}", blake3::Hash::from(root.unwrap()));
            tree.set_headroot(root.as_ref());
            return Ok(())
        }

        // Iterate over current contracts wasm bincodes to compute its monotree root
        debug!(target: "blockchain::contractstoreoverlay::update_state_monotree", "Updating wasm bincodes monotree...");
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
            let key = blake3::hash(&key);
            let value = blake3::hash(&value);
            debug!(target: "blockchain::contractstoreoverlay::update_state_monotree", "Inserting key {key} with value: {value}");
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
        debug!(target: "blockchain::contractstoreoverlay::update_state_monotree", "New root: {}", blake3::Hash::from(wasm_monotree_root));
        root = tree.insert(
            root.as_ref(),
            blake3::hash(SLED_BINCODE_TREE).as_bytes(),
            &wasm_monotree_root,
        )?;
        debug!(target: "blockchain::contractstoreoverlay::update_state_monotree", "New global root: {}", blake3::Hash::from(root.unwrap()));
        tree.set_headroot(root.as_ref());

        Ok(())
    }
}
