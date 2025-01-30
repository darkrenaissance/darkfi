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

use std::sync::{Arc, Mutex, MutexGuard};

use log::{debug, info};
use sled_overlay::{sled, SledDbOverlay};

use darkfi::{blockchain::SledDbOverlayPtr, Error, Result};
use darkfi_sdk::crypto::ContractId;
use darkfi_serial::{async_trait, deserialize, serialize, SerialDecodable, SerialEncodable};

/// Contract metadata tree name.
pub const SLED_CONTRACT_METADATA_TREE: &[u8] = b"_contact_metadata";

/// Contract source code tree name.
pub const SLED_CONTRACT_SOURCE_CODE_TREE: &[u8] = b"_contact_source_code";

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
}

impl ContractMetaStore {
    /// Creates a `ContractMetaStore` instance.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let main = db.open_tree(SLED_CONTRACT_METADATA_TREE)?;
        let source_code = db.open_tree(SLED_CONTRACT_SOURCE_CODE_TREE)?;

        Ok(Self { sled_db: db.clone(), main, source_code })
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
}

/// The `ContractMetadataStoreOverlay` provides write operations for managing contract metadata in
/// underlying sled database. It supports inserting new [`ContractMetaData`] and contract source code
/// [`ContractSourceFile`] content and deleting existing source code.
struct ContractMetadataStoreOverlay {
    /// Pointer to the overlay used for accessing and performing database write operations on the store.
    overlay: SledDbOverlayPtr,
}

impl ContractMetadataStoreOverlay {
    /// Instantiate a [`ContractMetadataStoreOverlay`] over the provided [`sled::Db`] instance.
    pub fn new(db: sled::Db) -> Result<Self> {
        // Create overlay pointer
        let overlay = Arc::new(Mutex::new(SledDbOverlay::new(&db, vec![])));
        Ok(Self { overlay: overlay.clone() })
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
            info!(target: "explorerd::contract_meta_store::insert_source", "Inserted contract source for path {}", key);
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
            info!(target: "explorerd::contract_meta_store::insert_metadata",
                "Inserted contract metadata for contract_id {}: {metadata:?}", contract_id.to_string());
        }

        // Commit the changes
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
    use darkfi_sdk::crypto::MONEY_CONTRACT_ID;
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
