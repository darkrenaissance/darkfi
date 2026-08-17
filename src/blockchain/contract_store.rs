/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::{collections::BTreeMap, io::Cursor, str::FromStr};

use darkfi_sdk::{
    crypto::contract_id::{
        ContractId, NATIVE_CONTRACT_IDS_BYTES, SMART_CONTRACT_MONOTREE_DB_NAME,
        SMART_CONTRACT_ZKAS_DB_NAME,
    },
    monotree::{Hash as StateHash, KvdbOverlayDb, KvdbTreeDb, Monotree, EMPTY_HASH},
};
use darkfi_serial::{deserialize, serialize};
use kvdb_overlay::{Database, DatabaseOverlayStateDiff, Tree};
use tracing::{debug, error};

use crate::{
    zk::{empty_witnesses, VerifyingKey, ZkCircuit},
    zkas::ZkBinary,
    Error, Result,
};

use super::{parse_record, KvdbOverlayPtr};

pub const KVDB_CONTRACTS_TREE: &str = "_contracts";
pub const KVDB_CONTRACTS_TREES_TREE: &str = "_contracts_trees";
// blake3 hash of `_contracts_monotree`
pub const KVDB_CONTRACTS_MONOTREE_TREE: &str =
    "52a17c61e4f3c54b0b563cd6f118406456309f93fe745e11a516270395787aaf";
pub const KVDB_BINCODE_TREE: &str = "_wasm_bincode";

/// The `ContractStore` is a structure representing all kvdb trees related
/// to storing the blockchain's contracts information.
#[derive(Clone)]
pub struct ContractStore {
    /// Main pointer to the kvdb connection
    pub kvdb: Database,
    /// The kvdb tree storing the wasm bincode for deployed contracts.
    /// The layout looks like this:
    /// ```plaintext
    ///  tree: "_wasm_bincode"
    ///   key: ContractId
    /// value: Vec<u8>
    pub wasm: Tree,
    /// The kvdb tree storing the pointers to contracts' databases.
    /// See the rustdoc for the impl functions for more info.
    /// The layout looks like this:
    /// ```plaintext
    ///  tree: "_contracts"
    ///   key: ContractId
    /// value: Vec<blake3(ContractId || tree_name)>
    /// ```
    /// These values get mutated with `init()` and `remove()`.
    pub state: Tree,
    /// The kvdb tree storing the inverse pointers to contracts'
    /// databases. See the rustdoc for the impl functions for more
    /// info.
    /// The layout looks like this:
    /// ```plaintext
    ///  tree: "_contracts_trees"
    ///   key: blake3(ContractId || tree_name)
    /// value: ContractId
    /// ```
    /// These values get mutated with `init()` and `remove()`.
    pub state_trees: Tree,
    /// The kvdb tree storing the full contracts states monotree,
    /// excluding native contracts wasm bincodes.
    /// The layout looks like this:
    /// ```plaintext
    ///  tree: "blake3(_contracts_monotree)"
    ///   key: blake3(ContractId)
    /// value: blake3(contract monotree root)
    /// ```
    /// These values get mutated on each block/proposal append with
    /// `update_state_monotree()`.
    pub state_monotree: Tree,
}

impl ContractStore {
    /// Opens a new or existing `ContractStore` on the given key-value
    /// database.
    pub fn new(kvdb: &Database) -> Result<Self> {
        let wasm = kvdb.open_tree_default(KVDB_BINCODE_TREE)?;
        let state = kvdb.open_tree_default(KVDB_CONTRACTS_TREE)?;
        let state_trees = kvdb.open_tree_default(KVDB_CONTRACTS_TREES_TREE)?;
        let state_monotree = kvdb.open_tree_default(KVDB_CONTRACTS_MONOTREE_TREE)?;
        Ok(Self { kvdb: kvdb.clone(), wasm, state, state_trees, state_monotree })
    }

    /// Fetches the bincode for a given ContractId from the store's wasm tree.
    /// Returns an error if the bincode is not found.
    pub fn get(&self, contract_id: ContractId) -> Result<Vec<u8>> {
        if let Some(bincode) = self.wasm.get(&serialize(&contract_id))? {
            return Ok(bincode.to_vec())
        }

        Err(Error::WasmBincodeNotFound)
    }

    /// Do a lookup of an existing contract state. In order to succeed, the
    /// state must have been previously initialized with `init()`. If the
    /// state has been found, a handle to it will be returned. Otherwise, we
    /// return an error.
    pub fn lookup(&self, contract_id: &ContractId, tree_name: &str) -> Result<Tree> {
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
        let tree = self.kvdb.open_tree_default(&blake3::Hash::from(ptr).to_string())?;
        Ok(tree)
    }

    /// Attempt to remove an existing contract state. In order to succeed, the
    /// state must have been previously initialized with `init()`. If the state
    /// has been found, its contents in the tree will be cleared, and the pointer
    /// will be removed from the main `ContractStateStore`. If anything is not
    /// found as initialized, an error is returned.
    /// NOTE: this function is not used right now, we keep it for future proofing,
    ///       and its obviously untested.
    pub fn remove(&self, contract_id: &ContractId, tree_name: &str) -> Result<()> {
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
        if !self.state_trees.contains_key(&ptr)? {
            return Err(Error::ContractStateNotFound)
        }

        // Remove the deleted tree from the state pointer set.
        state_pointers.retain(|x| *x != ptr);
        self.state.insert(&contract_id_bytes, &serialize(&state_pointers))?;
        self.state_trees.remove(&ptr)?;

        // Drop the deleted tree from the database
        self.kvdb.drop_tree(&blake3::Hash::from(ptr).to_string())?;

        Ok(())
    }

    /// Abstraction function for fetching a `ZkBinary` and its respective `VerifyingKey`
    /// from a contract's zkas kvdb tree.
    pub fn get_zkas(
        &self,
        contract_id: &ContractId,
        zkas_ns: &str,
    ) -> Result<(ZkBinary, VerifyingKey)> {
        debug!(target: "blockchain::contractstore::get_zkas", "Looking up \"{contract_id}:{zkas_ns}\" zkas circuit & vk");

        let zkas_tree = self.lookup(contract_id, SMART_CONTRACT_ZKAS_DB_NAME)?;

        let Some(zkas_bytes) = zkas_tree.get(&serialize(&zkas_ns))? else {
            return Err(Error::ZkasBincodeNotFound)
        };

        // If anything in this function panics, that means corrupted data managed
        // to get into this kvdb tree. This should not be possible.
        let (zkbin, vkbin): (Vec<u8>, Vec<u8>) = deserialize(&zkas_bytes).unwrap();

        // The first vec is the compiled zkas binary
        let zkbin = ZkBinary::decode(&zkbin, false).unwrap();

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

    /// Retrieve provided key value bytes from a contract's zkas kvdb tree.
    pub fn get_state_tree_value(
        &self,
        contract_id: &ContractId,
        tree_name: &str,
        key: &[u8],
    ) -> Result<Vec<u8>> {
        debug!(target: "blockchain::contractstore::get_state_tree_value", "Looking up state tree value for {contract_id}:{tree_name}");

        // Grab the state tree
        let state_tree = self.lookup(contract_id, tree_name)?;

        // Grab the key value
        match state_tree.get(key)? {
            Some(value) => Ok(value.to_vec()),
            None => Err(Error::DatabaseError(format!(
                "State tree {contract_id}:{tree_name} doesn't contain key: {key:?}"
            ))),
        }
    }

    /// Retrieve all records from a contract's zkas kvdb tree, as a `BTreeMap`.
    /// Be careful as this will try to load everything in memory.
    pub fn get_state_tree_records(
        &self,
        contract_id: &ContractId,
        tree_name: &str,
    ) -> Result<BTreeMap<Vec<u8>, Vec<u8>>> {
        debug!(target: "blockchain::contractstore::get_state_tree_records", "Looking up state tree records for {contract_id}:{tree_name}");

        // Grab the state tree
        let state_tree = self.lookup(contract_id, tree_name)?;

        // Retrieve its records
        let mut ret = BTreeMap::new();
        for record in state_tree.iter() {
            let (key, value) = record.unwrap();
            ret.insert(key.to_vec(), value.to_vec());
        }

        Ok(ret)
    }

    /// Retrieve contracts states Monotree(SMT) current root.
    ///
    /// Note: native contracts wasm bincodes are excluded.
    pub fn get_state_monotree_root(&self) -> Result<StateHash> {
        let monotree_db = KvdbTreeDb::new(&self.state_monotree);
        let monotree = Monotree::new(monotree_db);
        Ok(match monotree.get_headroot()? {
            Some(hash) => hash,
            None => *EMPTY_HASH,
        })
    }
}

/// Overlay structure over a [`ContractStore`] instance.
pub struct ContractStoreOverlay(KvdbOverlayPtr);

impl ContractStoreOverlay {
    pub fn new(overlay: &KvdbOverlayPtr) -> Result<Self> {
        overlay.lock().unwrap().open_tree_default(KVDB_BINCODE_TREE, true)?;
        overlay.lock().unwrap().open_tree_default(KVDB_CONTRACTS_TREE, true)?;
        overlay.lock().unwrap().open_tree_default(KVDB_CONTRACTS_TREES_TREE, true)?;
        overlay.lock().unwrap().open_tree_default(KVDB_CONTRACTS_MONOTREE_TREE, true)?;
        Ok(Self(overlay.clone()))
    }

    /// Fetches the bincode for a given ContractId from the overlay's wasm tree.
    /// Returns an error if the bincode is not found.
    pub fn get(&self, contract_id: ContractId) -> Result<Vec<u8>> {
        if let Some(bincode) =
            self.0.lock().unwrap().get(KVDB_BINCODE_TREE, &serialize(&contract_id))?
        {
            return Ok(bincode.to_vec())
        }

        Err(Error::WasmBincodeNotFound)
    }

    /// Inserts or replaces the bincode for a given ContractId into the overlay's
    /// wasm tree.
    pub fn insert(&self, contract_id: ContractId, bincode: &[u8]) -> Result<()> {
        if let Err(e) =
            self.0.lock().unwrap().insert(KVDB_BINCODE_TREE, &serialize(&contract_id), bincode)
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
            if lock.contains_key(KVDB_CONTRACTS_TREE, &contract_id_bytes)? {
                let bytes = lock.get(KVDB_CONTRACTS_TREE, &contract_id_bytes)?.unwrap();
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
        lock.insert(KVDB_CONTRACTS_TREE, &contract_id_bytes, &serialize(&state_pointers))?;
        lock.insert(KVDB_CONTRACTS_TREES_TREE, &ptr, &contract_id_bytes)?;
        lock.open_tree_default(&blake3::Hash::from(ptr).to_string(), false)?;

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
        if !lock.contains_key(KVDB_CONTRACTS_TREE, &contract_id_bytes)? {
            return Err(Error::ContractNotFound(contract_id.to_string()))
        }

        let state_pointers = lock.get(KVDB_CONTRACTS_TREE, &contract_id_bytes)?.unwrap();
        let state_pointers: Vec<[u8; 32]> = deserialize(&state_pointers)?;

        // We assume the tree has been created already, so it should be listed
        // in this array. If not, that's an error.
        let ptr = contract_id.hash_state_id(tree_name);
        if !state_pointers.contains(&ptr) {
            return Err(Error::ContractStateNotFound)
        }
        if !lock.contains_key(KVDB_CONTRACTS_TREES_TREE, &ptr)? {
            return Err(Error::ContractStateNotFound)
        }

        // We open the tree and return its handle
        lock.open_tree_default(&blake3::Hash::from(ptr).to_string(), false)?;
        Ok(ptr)
    }

    /// Abstraction function for fetching a `ZkBinary` and its respective `VerifyingKey`
    /// from a contract's zkas kvdb tree.
    pub fn get_zkas(
        &self,
        contract_id: &ContractId,
        zkas_ns: &str,
    ) -> Result<(ZkBinary, VerifyingKey)> {
        debug!(target: "blockchain::contractstoreoverlay::get_zkas", "Looking up \"{contract_id}:{zkas_ns}\" zkas circuit & vk");

        let zkas_tree =
            blake3::Hash::from(self.lookup(contract_id, SMART_CONTRACT_ZKAS_DB_NAME)?).to_string();

        let Some(zkas_bytes) = self.0.lock().unwrap().get(&zkas_tree, &serialize(&zkas_ns))? else {
            return Err(Error::ZkasBincodeNotFound)
        };

        // If anything in this function panics, that means corrupted data managed
        // to get into this kvdb tree. This should not be possible.
        let (zkbin, vkbin): (Vec<u8>, Vec<u8>) = deserialize(&zkas_bytes).unwrap();

        // The first vec is the compiled zkas binary
        let zkbin = ZkBinary::decode(&zkbin, false).unwrap();

        // Construct the circuit to be able to read the VerifyingKey
        let circuit = ZkCircuit::new(empty_witnesses(&zkbin).unwrap(), &zkbin);

        // The second one is the serialized VerifyingKey for it
        let mut vk_buf = Cursor::new(vkbin);
        let vk = VerifyingKey::read::<Cursor<Vec<u8>>, ZkCircuit>(&mut vk_buf, circuit).unwrap();

        Ok((zkbin, vk))
    }

    /// Retrieve contracts states Monotree(SMT) current root.
    ///
    /// Note: native contracts wasm bincodes are excluded.
    pub fn get_state_monotree_root(&self) -> Result<StateHash> {
        let mut lock = self.0.lock().unwrap();
        let monotree_db = KvdbOverlayDb::new(&mut lock, KVDB_CONTRACTS_MONOTREE_TREE)?;
        let monotree = Monotree::new(monotree_db);
        Ok(match monotree.get_headroot()? {
            Some(hash) => hash,
            None => *EMPTY_HASH,
        })
    }

    /// Retrieve all updated contracts states and wasm bincodes from
    /// provided overlay diff, update their monotrees in the overlay
    /// their root records in the contracts states Monotree(SMT) and
    /// return its current root. The provided diff must always append
    /// new changes to the monotrees and it shouldn't contain dropped
    /// contracts.
    ///
    /// Note: native contracts wasm bincodes are excluded.
    pub fn update_state_monotree(&self, diff: &DatabaseOverlayStateDiff) -> Result<StateHash> {
        // Grab lock over the overlay
        let mut lock = self.0.lock().unwrap();
        debug!(target: "blockchain::contractstoreoverlay::update_state_monotree", "Retrieving contracts updates...");

        // Iterate over diff caches to find all contracts updates
        let mut contracts_updates: BTreeMap<[u8; 32], ContractMonotreeUpdates> = BTreeMap::new();
        for (state_key, (state_cache, _)) in &diff.caches {
            // Grab new/redeployed contracts wasm bincodes to include them
            // in their monotrees, excluding native ones.
            if state_key == KVDB_BINCODE_TREE {
                for (contract_id_bytes, (_, value)) in &state_cache.cache {
                    // Grab the actual contract ID bytes
                    let contract_id_bytes = deserialize(contract_id_bytes)?;

                    // Skip native ones
                    if NATIVE_CONTRACT_IDS_BYTES.contains(&contract_id_bytes) {
                        continue
                    }

                    // Grab its contract monotree state pointer
                    let contract_id: ContractId = deserialize(&contract_id_bytes)?;
                    let monotree_pointer =
                        contract_id.hash_state_id(SMART_CONTRACT_MONOTREE_DB_NAME);

                    // Grab its record from the map
                    let mut contract_updates = match contracts_updates.remove(&contract_id_bytes) {
                        Some(r) => r,
                        None => ContractMonotreeUpdates::new(monotree_pointer),
                    };

                    // Create the new/updated wasm bincode record
                    let key = blake3::hash(&contract_id_bytes);
                    let value = blake3::hash(value);
                    contract_updates.inserts.push((key, value));

                    // Insert the update record
                    contracts_updates.insert(contract_id_bytes, contract_updates);
                }
                continue
            }

            // Check if that cache is not a contract state one.
            // Overlay protected trees are all the native/non-contract
            // ones.
            if lock.state.protected_tree_names.contains(state_key) {
                continue
            }

            // Grab the actual state key
            let state_key: [u8; 32] = *blake3::Hash::from_str(state_key)?.as_bytes();

            // Grab its contract id
            let Some(contract_id_bytes) = lock.get(KVDB_CONTRACTS_TREES_TREE, &state_key)? else {
                return Err(Error::ContractStateNotFound)
            };
            let contract_id_bytes: [u8; 32] = deserialize(&contract_id_bytes)?;
            let contract_id: ContractId = deserialize(&contract_id_bytes)?;

            // Skip the actual monotree state cache
            let monotree_pointer = contract_id.hash_state_id(SMART_CONTRACT_MONOTREE_DB_NAME);
            if monotree_pointer == state_key {
                continue
            }

            // Grab its record from the map
            let mut contract_updates = match contracts_updates.remove(&contract_id_bytes) {
                Some(r) => r,
                None => ContractMonotreeUpdates::new(monotree_pointer),
            };

            // Handle the contract zkas tree
            if contract_id.hash_state_id(SMART_CONTRACT_ZKAS_DB_NAME) == state_key {
                // Grab the new/updated keys
                for (key, (_, value)) in &state_cache.cache {
                    // Prefix key with its tree name
                    let mut hasher = blake3::Hasher::new();
                    hasher.update(&state_key);
                    hasher.update(key);
                    let key = hasher.finalize();

                    // Exclude its verifying key from the value
                    let (zkbin, _): (Vec<u8>, Vec<u8>) = deserialize(value)?;
                    let value = blake3::hash(&zkbin);
                    contract_updates.inserts.push((key, value));
                }

                // Insert the update record
                contracts_updates.insert(contract_id_bytes, contract_updates);
                continue
            }

            // Grab the new/updated keys
            for (key, (_, value)) in &state_cache.cache {
                // Prefix key with its tree name
                let mut hasher = blake3::Hasher::new();
                hasher.update(&state_key);
                hasher.update(key);
                let key = hasher.finalize();
                let value = blake3::hash(value);
                contract_updates.inserts.push((key, value));
            }

            // Grab the dropped keys
            for key in state_cache.removed.keys() {
                // Prefix key with its tree name
                let mut hasher = blake3::Hasher::new();
                hasher.update(&state_key);
                hasher.update(key);
                let key = hasher.finalize();
                contract_updates.removals.push(key);
            }

            // Insert the update record
            contracts_updates.insert(contract_id_bytes, contract_updates);
        }

        // Apply all contracts updates and grab their new roots
        let mut contracts_roots = BTreeMap::new();
        for (contract_id_bytes, contract_updates) in contracts_updates {
            let contract_id: ContractId = deserialize(&contract_id_bytes)?;
            debug!(target: "blockchain::contractstoreoverlay::update_state_monotree", "Updating monotree for contract: {contract_id}");

            // Grab its monotree
            let monotree_ptr = blake3::Hash::from(contract_updates.monotree_pointer).to_string();
            let monotree_db = KvdbOverlayDb::new(&mut lock, &monotree_ptr)?;
            let mut monotree = Monotree::new(monotree_db);
            let mut monotree_root = monotree.get_headroot()?;
            let monotree_root_hash = match monotree_root {
                Some(hash) => blake3::Hash::from(hash),
                None => blake3::Hash::from(*EMPTY_HASH),
            };
            debug!(target: "blockchain::contractstoreoverlay::update_state_monotree", "Current root: {monotree_root_hash}");

            // Update or insert new records
            for (key, value) in &contract_updates.inserts {
                debug!(target: "blockchain::contractstoreoverlay::update_state_monotree", "Inserting key {key} with value: {value}");
                monotree_root =
                    monotree.insert(monotree_root.as_ref(), key.as_bytes(), value.as_bytes())?;
            }

            // Remove dropped records
            for key in &contract_updates.removals {
                debug!(target: "blockchain::contractstoreoverlay::update_state_monotree", "Removing key: {key}");
                monotree_root = monotree.remove(monotree_root.as_ref(), key.as_bytes())?;
            }

            // Set root
            monotree.set_headroot(monotree_root.as_ref())?;

            // Keep track of the new root for the main monotree
            let monotree_root = match monotree_root {
                Some(hash) => hash,
                None => *EMPTY_HASH,
            };
            debug!(target: "blockchain::contractstoreoverlay::update_state_monotree", "New root: {}", blake3::Hash::from(monotree_root));
            contracts_roots.insert(contract_id_bytes, monotree_root);
        }

        // Grab the contracts states monotree
        let monotree_db = KvdbOverlayDb::new(&mut lock, KVDB_CONTRACTS_MONOTREE_TREE)?;
        let mut monotree = Monotree::new(monotree_db);
        let mut monotree_root = monotree.get_headroot()?;
        let monotree_root_hash = match monotree_root {
            Some(hash) => blake3::Hash::from(hash),
            None => blake3::Hash::from(*EMPTY_HASH),
        };
        debug!(target: "blockchain::contractstoreoverlay::update_state_monotree", "Updating global monotree with root: {monotree_root_hash}");

        // Insert new/updated contracts monotrees roots
        for (contract_id_bytes, contract_monotree_root) in &contracts_roots {
            let contract_id: ContractId = deserialize(contract_id_bytes)?;
            debug!(target: "blockchain::contractstoreoverlay::update_state_monotree", "Inserting key {contract_id} with value: {}", blake3::Hash::from(*contract_monotree_root));
            monotree_root = monotree.insert(
                monotree_root.as_ref(),
                contract_id_bytes,
                contract_monotree_root,
            )?;
        }

        // Set new global root
        monotree.set_headroot(monotree_root.as_ref())?;

        // Return its hash
        let monotree_root = match monotree_root {
            Some(hash) => hash,
            None => *EMPTY_HASH,
        };
        debug!(target: "blockchain::contractstoreoverlay::update_state_monotree", "New global root: {}", blake3::Hash::from(monotree_root));

        Ok(monotree_root)
    }
}

/// Auxiliary struct representing a contract monotree updates.
struct ContractMonotreeUpdates {
    monotree_pointer: [u8; 32],
    inserts: Vec<(blake3::Hash, blake3::Hash)>,
    removals: Vec<blake3::Hash>,
}

impl ContractMonotreeUpdates {
    fn new(monotree_pointer: [u8; 32]) -> Self {
        Self { monotree_pointer, inserts: vec![], removals: vec![] }
    }
}
