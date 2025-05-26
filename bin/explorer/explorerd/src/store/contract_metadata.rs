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

use std::{
    str::FromStr,
    sync::{Arc, Mutex, MutexGuard},
};

use log::debug;
use sled_overlay::{sled, SledDbOverlay};

use darkfi::{blockchain::SledDbOverlayPtr, Error, Result};
use darkfi_sdk::{crypto::ContractId, tx::TransactionHash};

use darkfi_serial::{async_trait, deserialize, serialize, SerialDecodable, SerialEncodable};

/// Contract metadata tree name.
pub const SLED_CONTRACT_METADATA_TREE: &[u8] = b"_contact_metadata";

/// Contract source code tree name.
pub const SLED_CONTRACT_SOURCE_CODE_TREE: &[u8] = b"_contact_source_code";

/// Contract source code tree name.
pub const SLED_CONTRACT_STATE_TREE: &[u8] = b"_contract_state";

/// Represents contract metadata containing additional contract information that is not stored on-chain.
#[derive(Debug, Clone, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct ContractMetaData {
    pub name: String,
    pub description: String,
}

impl ContractMetaData {
    pub fn new(name: String, description: String) -> Self {
        Self { name, description }
    }
}

/// Represents a source file containing its file path as a string and its content as a vector of bytes.
#[derive(Debug, Clone)]
pub struct ContractSourceFile {
    pub path: String,
    pub content: String,
}

impl ContractSourceFile {
    /// Creates a `ContractSourceFile` instance.
    pub fn new(path: String, content: String) -> Self {
        Self { path, content }
    }
}

pub struct ContractMetaStore {
    /// Pointer to the underlying sled database used by the store and its associated overlay.
    pub sled_db: sled::Db,

    /// Primary sled tree for storing contract metadata, utilizing [`ContractId::to_string`] as keys
    /// and serialized [`ContractMetaData`] as values.
    pub main: sled::Tree,

    /// Sled tree for storing contract source code, utilizing source file paths as keys pre-appended with a contract id
    /// and serialized contract source code [`ContractSourceFile`] content as values.
    pub source_code: sled::Tree,

    /// Sled tree for storing contract runtime state, utilizing keys
    /// `block_height/contract_id` and serialized WASAM state data as values.
    pub contract_state: sled::Tree,
}

impl ContractMetaStore {
    /// Creates a `ContractMetaStore` instance.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let main = db.open_tree(SLED_CONTRACT_METADATA_TREE)?;
        let source_code = db.open_tree(SLED_CONTRACT_SOURCE_CODE_TREE)?;
        let contract_state = db.open_tree(SLED_CONTRACT_STATE_TREE)?;

        Ok(Self { sled_db: db.clone(), main, source_code, contract_state })
    }

    /// Retrieves associated contract metadata for a given [`ContractId`],
    /// returning an `Option` of [`ContractMetaData`] upon success.
    pub fn get(&self, contract_id: &ContractId) -> Result<Option<ContractMetaData>> {
        let opt = self.main.get(contract_id.to_string().as_bytes())?;
        opt.map(|bytes| deserialize(&bytes).map_err(Error::from)).transpose()
    }

    /// Provides the number of stored [`ContractMetaData`].
    pub fn len(&self) -> usize {
        self.main.len()
    }

    /// Checks if there is contract metadata stored.
    pub fn is_empty(&self) -> bool {
        self.main.is_empty()
    }

    /// Retrieves all the source file paths associated for provided [`ContractId`].
    ///
    /// This function uses provided [`ContractId`] as a prefix to filter relevant paths
    /// stored in the underlying sled tree, ensuring only files belonging to
    /// the given contract ID are included. Returns a `Vec` of [`String`]
    /// representing source code paths.
    pub fn get_source_paths(&self, contract_id: &ContractId) -> Result<Vec<String>> {
        let prefix = format!("{}/", contract_id);

        // Get all the source paths for provided `ContractId`
        let mut entries = self
            .source_code
            .scan_prefix(&prefix)
            .filter_map(|item| {
                let (key, _) = item.ok()?;
                let key_str = String::from_utf8(key.to_vec()).ok()?;
                key_str.strip_prefix(&prefix).map(|path| path.to_string())
            })
            .collect::<Vec<String>>();

        // Sort the entries to ensure a consistent order
        entries.sort();

        Ok(entries)
    }

    /// Retrieves a source content as a [`String`] given a [`ContractId`] and path.
    pub fn get_source_content(
        &self,
        contract_id: &ContractId,
        source_path: &str,
    ) -> Result<Option<String>> {
        let key = format!("{}/{}", contract_id, source_path);
        match self.source_code.get(key.as_bytes())? {
            Some(ivec) => Ok(Some(String::from_utf8(ivec.to_vec()).map_err(|e| {
                Error::Custom(format!(
                    "[get_source_content] Failed to retrieve source content: {e:?}"
                ))
            })?)),
            None => Ok(None),
        }
    }

    /// Retrieves the runtime state for a specific [`ContractId`], block height, and `TransactionHash`
    /// from the store.
    ///
    /// Uses a compound key generated by combining the `block_height`, `ContractId`, and `TransactionHash`:
    /// `<block_height>/<ContractId>/<tx_hash>` to uniquely identify the state entry.
    /// Returns `None` if no state exists for the provided key.
    pub fn get_contract_state(
        &self,
        block_height: u32,
        contract_id: &ContractId,
        tx_hash: &TransactionHash,
    ) -> Result<Option<Vec<u8>>> {
        let key = format!("{}/{}/{}", block_height, contract_id, tx_hash);
        match self.contract_state.get(key.as_bytes())? {
            Some(value) => Ok(Some(value.to_vec())),
            None => Ok(None),
        }
    }

    /// Retrieves all contract IDs and their associated `TransactionHash` stored for a specified block height.
    ///
    /// Scans the `contract_state` tree for keys prefixed with `<block_height>/` and extracts
    /// the contract IDs and `TransactionHash`. Useful for retrieving states associated with a height
    /// during operations like reorgs.
    pub fn get_state_contract_ids_by_height(
        &self,
        block_height: u32,
    ) -> Result<Vec<(ContractId, TransactionHash)>> {
        let height_prefix = format!("{}/", block_height);
        let mut results = Vec::new();

        // Iterate over keys with the given block height as prefix
        for result in self.contract_state.scan_prefix(height_prefix.as_bytes()) {
            let (key, _) = result?;

            // Parse the key
            if let Ok(key_str) = std::str::from_utf8(&key) {
                // Extract `<block_height>/<contract_id>/<tx_hash>`
                let parts: Vec<_> = key_str.split('/').collect();
                if parts.len() == 3 {
                    let contract_id = ContractId::from_str(parts[1])?;
                    let tx_hash = TransactionHash::from_str(parts[2])?;
                    results.push((contract_id, tx_hash));
                }
            }
        }

        Ok(results)
    }

    /// Adds contract source code [`ContractId`] and `Vec` of [`ContractSourceFile`]s to the store,
    /// deleting existing source associated with the provided contract id before doing so.
    ///
    /// Delegates operation to [`ContractMetadataStoreOverlay::insert_source`],
    /// whose documentation provides more details.
    pub fn insert_source(
        &self,
        contract_id: &ContractId,
        source: &[ContractSourceFile],
    ) -> Result<()> {
        let existing_source_paths = self.get_source_paths(contract_id)?;
        let overlay = ContractMetadataStoreOverlay::new(self.sled_db.clone())?;
        overlay.insert_source(contract_id, source, Some(&existing_source_paths))?;
        Ok(())
    }

    /// Adds contract metadata using provided [`ContractId`] and [`ContractMetaData`] pairs to the store.
    ///
    /// Delegates operation to [`ContractMetadataStoreOverlay::insert_metadata`], whose documentation
    /// provides more details.
    pub fn insert_metadata(
        &self,
        contract_ids: &[ContractId],
        metadata: &[ContractMetaData],
    ) -> Result<()> {
        let overlay = ContractMetadataStoreOverlay::new(self.sled_db.clone())?;
        overlay.insert_metadata(contract_ids, metadata)?;
        Ok(())
    }
    /// Adds contract runtime state using the provided [`ContractId`], block height, `TransactionHash`,
    /// and state data to the store.
    ///
    /// Delegates the operation to [`ContractMetadataStoreOverlay::save_contract_state`], whose
    /// documentation provides more details.
    pub fn update_contract_state(
        &self,
        block_height: u32,
        contract_id: &ContractId,
        tx_hash: &TransactionHash,
        state: &[u8],
    ) -> Result<()> {
        let overlay = ContractMetadataStoreOverlay::new(self.sled_db.clone())?;
        overlay.save_contract_state(contract_id, block_height, tx_hash, state)?;
        Ok(())
    }

    /// Resets all contract runtime states associated with the specified block height.
    ///
    /// Delegates the logic to [`ContractMetadataStoreOverlay::reset_contract_state_by_height`],
    /// whose documentation provides more details.
    pub fn reset_contract_state_by_height(&self, height: u32) -> Result<()> {
        let overlay = ContractMetadataStoreOverlay::new(self.sled_db.clone())?;
        overlay.reset_contract_state_by_height(height)?;
        Ok(())
    }

    /// Resets all contract runtime states associated with the provided range of block heights.
    ///
    /// Delegates the logic to [`ContractMetadataStoreOverlay::reset_contract_state_range`],
    /// whose documentation provides more details.
    pub fn reset_contract_state_range(&self, start_height: u32, end_height: u32) -> Result<()> {
        let overlay = ContractMetadataStoreOverlay::new(self.sled_db.clone())?;
        overlay.reset_contract_state_range(start_height, end_height)?;
        Ok(())
    }
}

/// The `ContractMetadataStoreOverlay` provides write operations for managing contract metadata in
/// underlying sled database. It supports inserting new [`ContractMetaData`] and contract source code
/// [`ContractSourceFile`] content and deleting existing source code.
struct ContractMetadataStoreOverlay {
    /// Pointer to the overlay used for accessing and performing database write operations on the store.
    overlay: SledDbOverlayPtr,
    /// Pointer managed by the [`ContractMetadataStore`] that references the sled instance on which the overlay operates.
    db: sled::Db,
}

impl ContractMetadataStoreOverlay {
    /// Instantiate a [`ContractMetadataStoreOverlay`] over the provided [`sled::Db`] instance.
    pub fn new(db: sled::Db) -> Result<Self> {
        // Create overlay pointer
        let overlay = Arc::new(Mutex::new(SledDbOverlay::new(&db, vec![])));
        Ok(Self { overlay: overlay.clone(), db: db.clone() })
    }

    /// Inserts [`ContractSourceFile`]s associated with provided [`ContractId`] into the store's
    /// [`SLED_CONTRACT_SOURCE_CODE_TREE`], committing the changes upon success.
    ///
    /// This function locks the overlay, then inserts the provided source files into the store while
    /// handling serialization and potential errors. The provided contract ID is used to create a key
    /// for each source file by prepending the contract ID to each source code path. On success, the
    /// contract source code is persisted and made available for use.
    ///
    /// If optional `source_paths_to_delete` is provided, the function first deletes the existing
    /// source code associated with these paths before inserting the provided source code.
    pub fn insert_source(
        &self,
        contract_id: &ContractId,
        source: &[ContractSourceFile],
        source_paths_to_delete: Option<&[String]>,
    ) -> Result<()> {
        // Obtain lock
        let mut lock = self.lock(SLED_CONTRACT_SOURCE_CODE_TREE)?;

        // Delete existing source when existing paths are provided
        if let Some(paths_to_delete) = source_paths_to_delete {
            self.delete_source(contract_id, paths_to_delete, &mut lock)?;
        };

        // Insert each source code file
        for source_file in source.iter() {
            // Create key by pre-pending contract id to the source code path
            let key = format!("{}/{}", contract_id, source_file.path);
            // Insert the source code
            lock.insert(
                SLED_CONTRACT_SOURCE_CODE_TREE,
                key.as_bytes(),
                source_file.content.as_bytes(),
            )?;
            debug!(target: "explorerd::contract_meta_store::insert_source", "Inserted contract source for path {}", key);
        }

        // Commit the changes
        lock.apply()?;

        Ok(())
    }

    /// Deletes source code associated with provided [`ContractId`] from the store's [`SLED_CONTRACT_SOURCE_CODE_TREE`],
    /// committing the changes upon success.
    ///
    /// This auxiliary function locks the overlay, then removes the code associated with the provided
    /// contract ID from the store, handling serialization and potential errors. The contract ID is
    /// prepended to each source code path to create the keys for deletion. On success, the contract
    /// source code is permanently deleted.
    fn delete_source(
        &self,
        contract_id: &ContractId,
        source_paths: &[String],
        lock: &mut MutexGuard<SledDbOverlay>,
    ) -> Result<()> {
        // Delete each source file associated with provided paths
        for path in source_paths.iter() {
            // Create key by pre-pending contract id to the source code path
            let key = format!("{}/{}", contract_id, path);
            // Delete the source code
            lock.remove(SLED_CONTRACT_SOURCE_CODE_TREE, key.as_bytes())?;
            debug!(target: "explorerd::contract_meta_store::delete_source", "Deleted contract source for path {}", key);
        }

        Ok(())
    }

    /// Inserts [`ContractId`] and [`ContractMetaData`] pairs into the store's [`SLED_CONTRACT_METADATA_TREE`],
    /// committing the changes upon success.
    ///
    /// This function locks the overlay, verifies that the contract_ids and metadata arrays have matching lengths,
    /// then inserts them into the store while handling serialization and potential errors. On success,
    /// contract metadata is persisted and available for use.
    pub fn insert_metadata(
        &self,
        contract_ids: &[ContractId],
        metadata: &[ContractMetaData],
    ) -> Result<()> {
        let mut lock = self.lock(SLED_CONTRACT_METADATA_TREE)?;

        // Ensure lengths of contract_ids and metadata arrays match
        if contract_ids.len() != metadata.len() {
            return Err(Error::Custom(String::from(
                "The lengths of contract_ids and metadata arrays must match",
            )));
        }

        // Insert each contract id and metadata pair
        for (contract_id, metadata) in contract_ids.iter().zip(metadata.iter()) {
            // Serialize the gas data
            let serialized_metadata = serialize(metadata);

            // Insert serialized gas data
            lock.insert(
                SLED_CONTRACT_METADATA_TREE,
                contract_id.to_string().as_bytes(),
                &serialized_metadata,
            )?;
            debug!(target: "explorerd::contract_meta_store::insert_metadata",
                "Inserted contract metadata for contract_id {}: {metadata:?}", contract_id);
        }

        // Commit the changes
        lock.apply()?;

        Ok(())
    }

    /// Saves the contract runtime state for [`ContractId`], block height, and `TransactionHash`
    /// into the store's [`SLED_CONTRACT_STATE_TREE`], ensuring the changes are committed.
    ///
    /// Constructs a compound key `<block_height>/<ContractId>/<tx_hash>` to uniquely identify
    /// the state entry.
    pub fn save_contract_state(
        &self,
        contract_id: &ContractId,
        block_height: u32,
        tx_hash: &TransactionHash,
        state: &[u8],
    ) -> Result<()> {
        let mut lock = self.lock(SLED_CONTRACT_STATE_TREE)?;

        let key = format!("{}/{}/{}", block_height, contract_id, tx_hash);

        // Save the state
        lock.insert(SLED_CONTRACT_STATE_TREE, key.as_bytes(), state)?;

        // Commit the change
        lock.apply()?;
        Ok(())
    }

    /// Resets contract states in the provided range `reset_height` to `last_height`.
    ///
    /// This will invoke `reset_contract_state_by_height` for each height in the range
    /// to remove all contract state entries associated with those block heights.
    pub fn reset_contract_state_range(&self, start_height: u32, end_height: u32) -> Result<()> {
        if start_height > end_height {
            return Err(Error::Custom(format!(
                "Invalid height range: reset_height ({start_height}) > last_height ({end_height})"
            )));
        }

        for height in start_height..=end_height {
            self.reset_contract_state_by_height(height)?;
        }

        debug!("Successfully reset contract states for heights {start_height} to {end_height}");

        Ok(())
    }

    /// Resets contract states associated with the specified block height.
    ///
    /// This involves removing all entries in the `contract_state` for the given
    /// `block_height`. 
    pub fn reset_contract_state_by_height(&self, block_height: u32) -> Result<()> {
        let mut lock = self.lock(SLED_CONTRACT_STATE_TREE)?;

        // Build the prefix for scanning keys by `block_height`
        let prefix = format!("{}/", block_height);

        // Iterate and remove state associated with prefixed height
        for result in self.db.open_tree(SLED_CONTRACT_STATE_TREE)?.scan_prefix(prefix.as_bytes()) {
            let (key, _) = result?;
            lock.remove(SLED_CONTRACT_STATE_TREE, &key)?;
        }

        // Commit changes
        lock.apply()?;

        Ok(())
    }

    /// Acquires a lock on the database, opening a specified tree for write operations, returning a
    /// [`MutexGuard<SledDbOverlay>`] representing the locked state.
    pub fn lock(&self, tree_name: &[u8]) -> Result<MutexGuard<SledDbOverlay>> {
        // Lock the database, open tree, and return lock
        let mut lock = self.overlay.lock().unwrap();
        lock.open_tree(tree_name, true)?;
        Ok(lock)
    }
}

#[cfg(test)]
///  This test module verifies the correct insertion and retrieval of contract metadata and source code.
mod tests {
    use super::*;
    use crate::test_utils::init_logger;
    use darkfi_sdk::crypto::{
        contract_id::ContractId, DAO_CONTRACT_ID, DEPLOYOOOR_CONTRACT_ID, MONEY_CONTRACT_ID,
    };
    use sled_overlay::sled::Config;

    // Test source paths data
    const TEST_SOURCE_PATHS: &[&str] = &["test/source1.rs", "test/source2.rs"];

    // Test source code data
    const TEST_SOURCE_CONTENT: &[&str] =
        &["fn main() { println!(\"Hello, world!\"); }", "fn add(a: i32, b: i32) -> i32 { a + b }"];

    /// Tests the storing of contract source code by setting up the store, retrieving loaded source paths
    /// and verifying that the retrieved paths match against expected results.
    #[test]
    fn test_add_contract_source() -> Result<()> {
        // Setup test, returning initialized contract metadata store
        let store = setup()?;

        // Load source code tests data
        let contract_id = load_source_code(&store)?;

        // Initialize expected source paths
        let expected_source_paths: Vec<String> =
            TEST_SOURCE_PATHS.iter().map(|s| s.to_string()).collect();

        // Retrieve actual loaded source files
        let actual_source_paths = store.get_source_paths(contract_id)?;

        // Verify that loaded source code matches expected results
        assert_eq!(expected_source_paths, actual_source_paths);

        Ok(())
    }

    /// Validates the retrieval of a contract source file from the metadata store by setting up the store,
    /// loading test source code data, and verifying that loaded source contents match against
    /// expected content.
    #[test]
    fn test_get_contract_source() -> Result<()> {
        // Setup test, returning initialized contract metadata store
        let store = setup()?;

        // Load source code tests data
        let contract_id = load_source_code(&store)?;

        // Iterate through test data
        for (source_path, expected_content) in
            TEST_SOURCE_PATHS.iter().zip(TEST_SOURCE_CONTENT.iter())
        {
            // Get the content of the source path from the store
            let actual_source = store.get_source_content(contract_id, source_path)?;

            // Verify that the source code content is the store
            assert!(actual_source.is_some(), "No content found for path: {}", source_path);

            // Validate that the source content matches expected results
            assert_eq!(
                actual_source.unwrap(),
                expected_content.to_string(),
                "Actual source does not match the expected results for path: {}",
                source_path
            );
        }

        Ok(())
    }

    /// Tests the addition of [`ContractMetaData`] to the store by setting up the store, inserting
    /// metadata, and verifying the inserted data matches the expected results.
    #[test]
    fn test_add_metadata() -> Result<()> {
        // Setup test, returning initialized contract metadata store
        let store = setup()?;

        // Unique identifier for contracts in tests
        let contract_id: ContractId = *MONEY_CONTRACT_ID;

        // Declare expected metadata used for test
        let expected_metadata: ContractMetaData = ContractMetaData::new(
            "Money Contract".to_string(),
            "Money Contract Description".to_string(),
        );

        // Add metadata for the source code to the test
        store.insert_metadata(&[contract_id], &[expected_metadata.clone()])?;

        // Get the metadata content from the store
        let actual_metadata = store.get(&contract_id)?;

        // Verify that the metadata exists in the store
        assert!(actual_metadata.is_some());

        // Verify actual metadata matches expected results
        assert_eq!(actual_metadata.unwrap(), expected_metadata.clone());

        Ok(())
    }

    /// Tests updating and retrieving the runtime state for a contract by initializing the store,
    /// saving state data, and verifying the retrieved data matches the expected result.
    #[test]
    fn test_save_and_get_contract_state() -> Result<()> {
        // Setup test, returning initialized contract metadata store
        let store = setup()?;

        let contract_id: ContractId = *MONEY_CONTRACT_ID;
        let block_height: u32 = 1;
        let tx_hash = TransactionHash::from_str(
            "c2ad39c6f136d8f4d346c7d2505257e8fc8457de51b10b03296b9fead3d5e25f",
        )?;
        let expected_state = b"state_data";

        // Update the contract state with the transaction hash
        store.update_contract_state(block_height, &contract_id, &tx_hash, expected_state)?;

        // Retrieve the saved contract state
        let actual_state = store.get_contract_state(block_height, &contract_id, &tx_hash)?;

        // Verify expeceted results
        assert_eq!(actual_state, Some(expected_state.to_vec()));

        Ok(())
    }

    #[test]
    /// Tests retrieving contract IDs associated with contract state for a specific block height by
    /// by initializing the store, inserting test states, querying the IDs, and verifying the returned
    /// results match the expected contract IDs.
    fn test_get_state_contract_ids_by_height() -> Result<()> {
        // Setup test, returning initialized contract metadata store
        let store = setup()?;

        // Transaction hash test data
        let tx_hash_1 = TransactionHash::from_str(
            "c2ad39c6f136d8f4d346c7d2505257e8fc8457de51b10b03296b9fead3d5e25f",
        )?;
        let tx_hash_2 = TransactionHash::from_str(
            "d2bd39c6f236d8f4d346c7d2505257e8fc8457be51b10b03296b8fead3d5e24e",
        )?;
        let tx_hash_3 = TransactionHash::from_str(
            "e3cd49d7f346e9f5d357c8e3606268f9fd9567fe62c21c14397b9fece3e6f36f",
        )?;

        // Insert test states with block heights and transaction hashes
        store.update_contract_state(1, &MONEY_CONTRACT_ID, &tx_hash_1, b"state1")?;
        store.update_contract_state(2, &MONEY_CONTRACT_ID, &tx_hash_2, b"state2")?;
        store.update_contract_state(2, &DAO_CONTRACT_ID, &tx_hash_3, b"state1")?;
        store.update_contract_state(2, &DEPLOYOOOR_CONTRACT_ID, &tx_hash_1, b"state1")?;

        // Fetch contract IDs and transaction hashes for block height 2
        let actual_state = store.get_state_contract_ids_by_height(2)?;

        // Verify actual matches expected results
        assert_eq!(actual_state.len(), 3);
        assert!(actual_state.contains(&(*MONEY_CONTRACT_ID, tx_hash_2)));
        assert!(actual_state.contains(&(*DAO_CONTRACT_ID, tx_hash_3)));
        assert!(actual_state.contains(&(*DEPLOYOOOR_CONTRACT_ID, tx_hash_1)));

        Ok(())
    }

    #[test]
    /// Tests resetting contract states within a specific block height range,
    /// verifying that entries in the range are deleted and others remain intact.
    fn test_reset_contract_state_range() -> Result<()> {
        // Setup test, returning initialized contract metadata store
        let store = setup()?;

        // Transaction hash test data
        let tx_hash_1 = TransactionHash::from_str(
            "e3cd49d7f346e9f5d357c8e3606268f9fd9567fe62c21c14397b9fece3e6f36f",
        )?;
        let tx_hash_2 = TransactionHash::from_str(
            "d5ae49d7f236d8f5d448c7d2516167f9fd9578fe62d21c24397c8fecd4e6f47e",
        )?;
        let tx_hash_3 = TransactionHash::from_str(
            "c4bf59d8f346daf6f568c8e371726afafe9587fe63f32d354a8a9feee4f8f58f",
        )?;
        let tx_hash_4 = TransactionHash::from_str(
            "f5d179f9f457dbf7f679e9fe8397371af0a0798ff84f43e365b9eaed95f7f69f",
        )?;
        let tx_hash_5 = TransactionHash::from_str(
            "b6f28afaf568ecf8f78ae22694a8482bf1b18aaf095054f465c9fbee97f9f79f",
        )?;

        // Insert test states with various block heights
        store.update_contract_state(1, &MONEY_CONTRACT_ID, &tx_hash_1, b"state1")?;
        store.update_contract_state(2, &DAO_CONTRACT_ID, &tx_hash_2, b"state2")?;
        store.update_contract_state(3, &DEPLOYOOOR_CONTRACT_ID, &tx_hash_3, b"state3")?;
        store.update_contract_state(4, &MONEY_CONTRACT_ID, &tx_hash_4, b"state4")?;
        store.update_contract_state(5, &MONEY_CONTRACT_ID, &tx_hash_5, b"state5")?;

        // Reset range 2 to height 4
        store.reset_contract_state_range(2, 4)?;

        // Verify that states within the range are removed
        assert_eq!(store.get_contract_state(2, &DAO_CONTRACT_ID, &tx_hash_2)?, None);
        assert_eq!(store.get_contract_state(3, &DEPLOYOOOR_CONTRACT_ID, &tx_hash_3)?, None);
        assert_eq!(store.get_contract_state(4, &MONEY_CONTRACT_ID, &tx_hash_4)?, None);

        // Verify that states outside the range are not removed
        assert!(store.get_contract_state(1, &MONEY_CONTRACT_ID, &tx_hash_1)?.is_some());
        assert!(store.get_contract_state(5, &MONEY_CONTRACT_ID, &tx_hash_5)?.is_some());

        Ok(())
    }

    #[test]
    /// Tests resetting contract states for a specific block height,
    /// verifying that entries for the height are deleted.
    fn test_reset_contract_state_by_height() -> Result<()> {
        // Setup test, returning initialized contract metadata store
        let store = setup()?;

        // Transaction hashes for testing
        let tx_hash_1 = TransactionHash::from_str(
            "c2ad39c6f136d8f4d346c7d2505257e8fc8457de51b10b03296b9fead3d5e25f",
        )?;
        let tx_hash_2 = TransactionHash::from_str(
            "d3be49d7f236d9f5e457c8d3616258f9fd9577fe62d21c24397c8fecd4e6f46e",
        )?;
        let tx_hash_3 = TransactionHash::from_str(
            "e4cf59e8f346eaf6f568c9e4727269fafe9688fe73f32d354a8d9feed5f7f57f",
        )?;

        // Insert test states for different block heights
        store.update_contract_state(1, &MONEY_CONTRACT_ID, &tx_hash_1, b"state1")?;
        store.update_contract_state(1, &DAO_CONTRACT_ID, &tx_hash_2, b"state2")?;
        store.update_contract_state(2, &DEPLOYOOOR_CONTRACT_ID, &tx_hash_3, b"state3")?;

        // Reset states at height 1
        store.reset_contract_state_by_height(1)?;

        // Verify states for height 1 are removed
        assert_eq!(store.get_contract_state(1, &MONEY_CONTRACT_ID, &tx_hash_1)?, None);
        assert_eq!(store.get_contract_state(1, &DAO_CONTRACT_ID, &tx_hash_2)?, None);

        // Verify states for other heights are still present
        assert!(store.get_contract_state(2, &DEPLOYOOOR_CONTRACT_ID, &tx_hash_3)?.is_some());

        Ok(())
    }

    /// Tests retrieving the runtime state for a contract when no state has been saved,
    /// ensuring the result is `None`.
    #[test]
    fn test_missing_contract_state() -> Result<()> {
        // Setup test, returning initialized contract metadata store
        let store = setup()?;

        // Define the contract ID, block height, and transaction hash
        let contract_id: ContractId = *MONEY_CONTRACT_ID;
        let block_height: u32 = 1;
        let tx_hash = TransactionHash::from_str(
            "c2ad39c6f136d8f4d346c7d2505257e8fc8457de51b10b03296b9fead3d5e25f",
        )?;

        // Retrieve the state for the given key
        let result = store.get_contract_state(block_height, &contract_id, &tx_hash)?;

        // Validate no state was found
        assert!(result.is_none());

        Ok(())
    }

    /// Sets up a test case for contract metadata store testing by initializing the logger
    /// and returning an initialized [`ContractMetaStore`].
    fn setup() -> Result<ContractMetaStore> {
        // Initialize logger to show execution output
        init_logger(simplelog::LevelFilter::Off, vec!["sled", "runtime", "net"]);

        // Initialize an in-memory sled db instance
        let db = Config::new().temporary(true).open()?;

        // Initialize the contract store
        ContractMetaStore::new(&db)
    }

    /// Loads [`TEST_SOURCE_PATHS`] and [`TEST_SOURCE_CONTENT`] into the provided
    /// [`ContractMetaStore`] to test source code insertion and retrieval.
    fn load_source_code(store: &ContractMetaStore) -> Result<&'static ContractId> {
        // Define the contract ID for testing
        let contract_id = &MONEY_CONTRACT_ID;

        // Define sample source files for testing using the shared paths and content
        let test_sources: Vec<ContractSourceFile> = TEST_SOURCE_PATHS
            .iter()
            .zip(TEST_SOURCE_CONTENT.iter())
            .map(|(path, content)| ContractSourceFile::new(path.to_string(), content.to_string()))
            .collect();

        // Add test source code to the store
        store.insert_source(contract_id, &test_sources)?;

        Ok(contract_id)
    }
}
