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
    io::{Cursor, Read},
    str::FromStr,
};

use log::info;
use tar::Archive;
use tinyjson::JsonValue;

use darkfi::{
    blockchain::BlockchainOverlay, validator::utils::deploy_native_contracts, Error, Result,
};
use darkfi_sdk::crypto::{ContractId, DAO_CONTRACT_ID, DEPLOYOOOR_CONTRACT_ID, MONEY_CONTRACT_ID};
use darkfi_serial::deserialize;

use crate::{
    store::{
        contract_metadata::{ContractMetaData, ContractSourceFile},
        NATIVE_CONTRACT_SOURCE_ARCHIVES,
    },
    ExplorerService,
};

/// Represents a contract record embellished with details that are not stored on-chain.
#[derive(Debug, Clone)]
pub struct ContractRecord {
    /// The Contract ID as a string
    pub id: String,

    /// The optional name of the contract
    pub name: Option<String>,

    /// The optional description of the contract
    pub description: Option<String>,
}

impl ContractRecord {
    /// Auxiliary function to convert a `ContractRecord` into a `JsonValue` array.
    pub fn to_json_array(&self) -> JsonValue {
        JsonValue::Array(vec![
            JsonValue::String(self.id.clone()),
            JsonValue::String(self.name.clone().unwrap_or_default()),
            JsonValue::String(self.description.clone().unwrap_or_default()),
        ])
    }
}

impl ExplorerService {
    /// Fetches the total contract count of all deployed contracts in the explorer database.
    pub fn get_contract_count(&self) -> usize {
        self.db.blockchain.contracts.wasm.len()
    }

    /// Retrieves all contracts from the store excluding native contracts (DAO, Deployooor, and Money),
    /// transforming them into `Vec` of [`ContractRecord`]s, and returns the result.
    pub fn get_contracts(&self) -> Result<Vec<ContractRecord>> {
        let native_contracts = [*DAO_CONTRACT_ID, *DEPLOYOOOR_CONTRACT_ID, *MONEY_CONTRACT_ID];
        self.get_filtered_contracts(|contract_id| !native_contracts.contains(contract_id))
    }

    /// Retrieves all native contracts (DAO, Deployooor, and Money) from the store, transforming them
    /// into `Vec` of [`ContractRecord`]s and returns the result.
    pub fn get_native_contracts(&self) -> Result<Vec<ContractRecord>> {
        let native_contracts = [*DAO_CONTRACT_ID, *DEPLOYOOOR_CONTRACT_ID, *MONEY_CONTRACT_ID];
        self.get_filtered_contracts(|contract_id| native_contracts.contains(contract_id))
    }

    /// Fetches a list of source code file paths for a given [ContractId], returning an empty vector
    /// if no contracts are found.
    pub fn get_contract_source_paths(&self, contract_id: &ContractId) -> Result<Vec<String>> {
        self.db.contract_meta_store.get_source_paths(contract_id).map_err(|e| {
            Error::DatabaseError(format!(
                "[get_contract_source_paths] Retrieval of contract source code paths failed: {e:?}"
            ))
        })
    }

    /// Fetches [`ContractMetaData`] for a given [`ContractId`], returning `None` if no metadata is found.
    pub fn get_contract_metadata(
        &self,
        contract_id: &ContractId,
    ) -> Result<Option<ContractMetaData>> {
        self.db.contract_meta_store.get(contract_id).map_err(|e| {
            Error::DatabaseError(format!(
                "[get_contract_metadata] Retrieval of contract metadata paths failed: {e:?}"
            ))
        })
    }

    /// Fetches the source code content for a specified [`ContractId`] and `path`, returning `None` if
    /// no source content is found.
    pub fn get_contract_source_content(
        &self,
        contract_id: &ContractId,
        path: &str,
    ) -> Result<Option<String>> {
        self.db.contract_meta_store.get_source_content(contract_id, path).map_err(|e| {
            Error::DatabaseError(format!(
                "[get_contract_source_content] Retrieval of contract source file failed: {e:?}"
            ))
        })
    }

    /// Adds source code for a specified [`ContractId`] from a provided tar file (in bytes).
    ///
    /// This function extracts the tar archive from `tar_bytes`, then loads each source file
    /// into the store. Each file is keyed by its path prefixed with the Contract ID.
    /// Returns a successful result or an error.
    pub fn add_contract_source(&self, contract_id: &ContractId, tar_bytes: &[u8]) -> Result<()> {
        // Untar the source code
        let source = untar_source(tar_bytes)?;

        // Insert contract source code
        self.db.contract_meta_store.insert_source(contract_id, &source).map_err(|e| {
            Error::DatabaseError(format!(
                "[add_contract_source] Adding of contract source code failed: {e:?}"
            ))
        })
    }

    /// Adds provided [`ContractId`] with corresponding [`ContractMetaData`] pairs into the contract
    /// metadata store, returning a successful result upon success.
    pub fn add_contract_metadata(
        &self,
        contract_ids: &[ContractId],
        metadata: &[ContractMetaData],
    ) -> Result<()> {
        self.db.contract_meta_store.insert_metadata(contract_ids, metadata).map_err(|e| {
            Error::DatabaseError(format!(
                "[add_contract_metadata] Upload of contract source code failed: {e:?}"
            ))
        })
    }

    /// Deploys native contracts required for gas calculation and retrieval.
    pub async fn deploy_native_contracts(&self) -> Result<()> {
        let overlay = BlockchainOverlay::new(&self.db.blockchain)?;
        deploy_native_contracts(&overlay, 10).await?;
        overlay.lock().unwrap().overlay.lock().unwrap().apply()?;
        Ok(())
    }

    /// Loads native contract source code into the explorer database by extracting it from tar archives
    /// created during the explorer build process. The extracted source code is associated with
    /// the corresponding [`ContractId`] for each loaded contract and stored.
    pub fn load_native_contract_sources(&self) -> Result<()> {
        // Iterate each native contract source archive
        for (contract_id_str, archive_bytes) in NATIVE_CONTRACT_SOURCE_ARCHIVES.iter() {
            // Untar the native contract source code
            let source_code = untar_source(archive_bytes)?;

            // Parse contract id into a contract id instance
            let contract_id = &ContractId::from_str(contract_id_str)?;

            // Add source code into the `ContractMetaStore`
            self.db.contract_meta_store.insert_source(contract_id, &source_code)?;
            info!(target: "explorerd: load_native_contract_sources", "Loaded native contract source {contract_id_str}");
        }
        Ok(())
    }

    /// Loads [`ContractMetaData`] for deployed native contracts into the explorer database by adding descriptive
    /// information (e.g., name and description) used to display contract details.
    pub fn load_native_contract_metadata(&self) -> Result<()> {
        let contract_ids = [*MONEY_CONTRACT_ID, *DAO_CONTRACT_ID, *DEPLOYOOOR_CONTRACT_ID];

        // Create pre-defined native contract metadata
        let metadatas = [
            ContractMetaData::new(
                "Money".to_string(),
                "Facilitates money transfers, atomic swaps, minting, freezing, and staking of consensus tokens".to_string(),
            ),
            ContractMetaData::new(
                "DAO".to_string(),
                "Provides functionality for Anonymous DAOs".to_string(),
            ),
            ContractMetaData::new(
                "Deployoor".to_string(),
                "Handles non-native smart contract deployments".to_string(),
            ),
        ];

        // Load contract metadata into the `ContractMetaStore`
        self.db.contract_meta_store.insert_metadata(&contract_ids, &metadatas)?;
        info!(target: "explorerd: load_native_contract_metadata", "Loaded metadata for native contracts");

        Ok(())
    }

    /// Converts a [`ContractId`] into a [`ContractRecord`].
    ///
    /// This function retrieves the [`ContractMetaData`] associated with the provided Contract ID
    /// and uses any found metadata to construct a contract record. Upon success, the function
    /// returns a [`ContractRecord`] containing relevant details about the contract.
    fn to_contract_record(&self, contract_id: &ContractId) -> Result<ContractRecord> {
        let metadata = self.db.contract_meta_store.get(contract_id)?;
        let name: Option<String>;
        let description: Option<String>;

        // Set name and description based on the presence of metadata
        if let Some(metadata) = metadata {
            name = Some(metadata.name);
            description = Some(metadata.description);
        } else {
            name = None;
            description = None;
        }

        // Return transformed contract record
        Ok(ContractRecord { id: contract_id.to_string(), name, description })
    }

    /// Auxiliary function that retrieves [`ContractRecord`]s filtered by a provided `filter_fn` closure.
    ///
    /// This function accepts a filter function `Fn(&ContractId) -> bool` that determines
    /// which contracts are included based on their [`ContractId`]. It iterates over
    /// Contract IDs stored in the blockchain's contract tree, applying the filter function to decide inclusion.
    /// Converts the filtered Contract IDs into [`ContractRecord`] instances, returning them as a `Vec`,
    /// or an empty `Vec` if no contracts are found.
    fn get_filtered_contracts<F>(&self, filter_fn: F) -> Result<Vec<ContractRecord>>
    where
        F: Fn(&ContractId) -> bool,
    {
        let contract_keys = self.db.blockchain.contracts.wasm.iter().keys();

        // Iterate through stored Contract IDs, filtering out the contracts based filter
        contract_keys
            .filter_map(|serialized_contract_id| {
                // Deserialize the serialized Contract ID
                let contract_id: ContractId = match serialized_contract_id
                    .map_err(Error::from)
                    .and_then(|id_bytes| deserialize(&id_bytes).map_err(Error::from))
                {
                    Ok(id) => id,
                    Err(e) => {
                        return Some(Err(Error::DatabaseError(format!(
                            "[get_filtered_contracts] Contract ID retrieval or deserialization failed: {e:?}"
                        ))));
                    }
                };

                // Apply the filter
                if filter_fn(&contract_id) {
                    // Convert the matching Contract ID into a `ContractRecord`, return result
                    return match self.to_contract_record(&contract_id).map_err(|e| {
                        Error::DatabaseError(format!("[get_filtered_contracts] Failed to convert contract: {e:?}"))
                    }) {
                        Ok(record) => Some(Ok(record)),
                        Err(e) => Some(Err(e)),
                    };
                }

                // Skip contracts that do not match the filter
                None
            })
            .collect::<Result<Vec<ContractRecord>>>()
    }
}

/// Auxiliary function that extracts source code files from a TAR archive provided as a byte slice [`&[u8]`],
/// returning a `Vec` of [`ContractSourceFile`]s representing the extracted file paths and their contents.
pub fn untar_source(tar_bytes: &[u8]) -> Result<Vec<ContractSourceFile>> {
    // Use a Cursor and archive to read the tar file
    let cursor = Cursor::new(tar_bytes);
    let mut archive = Archive::new(cursor);

    // Vectors to hold the source paths and source contents
    let mut source: Vec<ContractSourceFile> = Vec::new();

    // Iterate through the entries in the tar archive
    for tar_entry in archive.entries()? {
        let mut tar_entry = tar_entry?;
        let path = tar_entry.path()?.to_path_buf();

        // Check if the entry is a file
        if tar_entry.header().entry_type().is_file() {
            let mut content = Vec::new();
            tar_entry.read_to_end(&mut content)?;

            // Convert the contents into a string
            let source_content = String::from_utf8(content)
                .map_err(|_| Error::ParseFailed("Failed converting source code to a string"))?;

            // Collect source paths and contents
            let path_str = path.to_string_lossy().into_owned();
            source.push(ContractSourceFile::new(path_str, source_content));
        }
    }

    Ok(source)
}

/// This test module ensures the correctness of the [`ExplorerService`] functionality with
/// respect to smart contracts.
///
/// The tests in this module cover adding, loading, storing, retrieving, and validating contract
/// metadata and source code. The primary goal is to validate the accuracy and reliability of
/// the `ExplorerService` when handling contract-related operations.
#[cfg(test)]
mod tests {
    use std::{fs::File, io::Read, path::Path, sync::Arc};

    use tar::Archive;
    use tempdir::TempDir;

    use darkfi::Error::Custom;
    use darkfi_sdk::crypto::MONEY_CONTRACT_ID;

    use super::*;
    use crate::{rpc::DarkfidRpcClient, test_utils::init_logger};

    /// Tests the adding of [`ContractMetaData`] to the store by adding
    /// metadata, and verifying the inserted data matches the expected results.
    #[test]
    fn test_add_metadata() -> Result<()> {
        // Setup test, returning initialized service
        let service = setup()?;

        // Unique identifier for contracts in tests
        let contract_id: ContractId = *MONEY_CONTRACT_ID;

        // Declare expected metadata used for test
        let expected_metadata: ContractMetaData = ContractMetaData::new(
            "Money Contract".to_string(),
            "Money Contract Description".to_string(),
        );

        // Add the metadata
        service.add_contract_metadata(&[contract_id], &[expected_metadata.clone()])?;

        // Get the metadata that was loaded as actual results
        let actual_metadata = service.get_contract_metadata(&contract_id)?;

        // Verify existence of loaded metadata
        assert!(actual_metadata.is_some());

        // Confirm actual metadata match expected results
        assert_eq!(actual_metadata.unwrap(), expected_metadata.clone());

        Ok(())
    }

    /// This test validates the loading and retrieval of native contract metadata. It sets up the
    /// explorer service, loads native contract metadata, and then verifies metadata retrieval
    /// for each native contract.
    #[test]
    fn test_load_native_contract_metadata() -> Result<()> {
        // Setup test, returning initialized service
        let service = setup()?;

        // Load native contract metadata
        service.load_native_contract_metadata()?;

        // Define Contract IDs used to retrieve loaded metadata
        let native_contract_ids = [*DAO_CONTRACT_ID, *DEPLOYOOOR_CONTRACT_ID, *MONEY_CONTRACT_ID];

        // For each native contract, verify metadata was loaded
        for contract_id in native_contract_ids.iter() {
            let metadata = service.get_contract_metadata(contract_id)?;
            assert!(metadata.is_some());
        }

        Ok(())
    }

    /// This test validates the loading, storage, and retrieval of native contract source code. It sets up the
    /// explorer service, loads native contract sources, and then verifies both the source paths and content
    /// for each native contract. The test compares the retrieved source paths and content against the expected
    /// results from the corresponding tar archives.
    #[test]
    fn test_load_native_contracts() -> Result<()> {
        // Setup test, returning initialized service
        let service = setup()?;

        // Load native contracts
        service.load_native_contract_sources()?;

        // Define contract archive paths
        let native_contract_tars = [
            "native_contracts_src/dao_contract_src.tar",
            "native_contracts_src/deployooor_contract_src.tar",
            "native_contracts_src/money_contract_src.tar",
        ];

        // Define Contract IDs to associate with each contract source archive
        let native_contract_ids = [*DAO_CONTRACT_ID, *DEPLOYOOOR_CONTRACT_ID, *MONEY_CONTRACT_ID];

        // Iterate archive and verify actual match expected results
        for (&tar_file, &contract_id) in native_contract_tars.iter().zip(&native_contract_ids) {
            // Verify that source paths match
            verify_source_paths(&service, tar_file, contract_id)?;

            // Verify that source content match
            verify_source_content(&service, tar_file, contract_id)?;
        }

        Ok(())
    }

    /// This test validates the transformation of a [`ContractId`] into a [`ContractRecord`].
    /// It sets up the explorer service, adds test metadata for a specific Contract ID, and then verifies the
    /// correct transformation of this Contract ID into a ContractRecord.
    #[test]
    fn test_to_contract_record() -> Result<()> {
        // Setup test, returning initialized service
        let service = setup()?;

        // Unique identifier for contracts in tests
        let contract_id: ContractId = *MONEY_CONTRACT_ID;

        // Declare expected metadata used for test
        let expected_metadata: ContractMetaData = ContractMetaData::new(
            "Money Contract".to_string(),
            "Money Contract Description".to_string(),
        );

        // Load contract metadata used for test
        service.add_contract_metadata(&[contract_id], &[expected_metadata.clone()])?;

        // Transform Contract ID to a `ContractRecord`
        let contract_record = service.to_contract_record(&contract_id)?;

        // Verify that name and description exist
        assert!(
            contract_record.name.is_some(),
            "Expected to_contract_record to return a contract with name"
        );
        assert!(
            contract_record.description.is_some(),
            "Expected to_contract_record to return a contract with description"
        );

        // Verify that id, name, and description match expected results
        assert_eq!(contract_id.to_string(), contract_record.id);
        assert_eq!(expected_metadata.name, contract_record.name.unwrap());
        assert_eq!(expected_metadata.description, contract_record.description.unwrap());

        Ok(())
    }

    /// Sets up a test case for contract metadata store testing by initializing the logger
    /// and returning an initialized [`ExplorerService`].
    fn setup() -> Result<ExplorerService> {
        // Initialize logger to show execution output
        init_logger(simplelog::LevelFilter::Off, vec!["sled", "runtime", "net"]);

        // Create a temporary directory for sled DB
        let temp_dir = TempDir::new("test")?;

        // Initialize a sled DB instance using the temporary directory's path
        let db_path = temp_dir.path().join("sled_db");

        // Initialize the explorer service
        ExplorerService::new(
            db_path.to_string_lossy().into_owned(),
            Arc::new(DarkfidRpcClient::new()),
        )
    }

    /// This Auxiliary function verifies that the loaded native contract source paths match the expected results
    /// from a given contract archive. This function extracts source paths from the specified `tar_file`, retrieves
    /// the actual paths for the [`ContractId`] from the ExplorerService, and compares them to ensure they match.
    fn verify_source_paths(
        service: &ExplorerService,
        tar_file: &str,
        contract_id: ContractId,
    ) -> Result<()> {
        // Read the tar file and extract source paths
        let tar_bytes = std::fs::read(tar_file)?;
        let mut expected_source_paths = extract_file_paths_from_tar(&tar_bytes)?;

        // Retrieve and sort actual source paths for the provided Contract ID
        let mut actual_source_paths = service.get_contract_source_paths(&contract_id)?;

        // Sort paths to ensure they are in the same order needed for assert
        expected_source_paths.sort();
        actual_source_paths.sort();

        // Verify actual source matches expected result
        assert_eq!(
            expected_source_paths, actual_source_paths,
            "Mismatch between expected and actual source paths for tar file: {tar_file}"
        );

        Ok(())
    }

    /// This auxiliary function verifies that the loaded native contract source content matches the
    /// expected results from a given contract source archive. It extracts source files from the specified
    /// `tar_file`, retrieves the actual content for each file path using the [`ContractId`] from the
    /// ExplorerService, and compares them to ensure the content match.
    fn verify_source_content(
        service: &ExplorerService,
        tar_file: &str,
        contract_id: ContractId,
    ) -> Result<()> {
        // Read the tar file
        let tar_bytes = std::fs::read(tar_file)?;
        let expected_source_paths = extract_file_paths_from_tar(&tar_bytes)?;

        // Validate contents of tar archive source code content
        for file_path in expected_source_paths {
            // Get the source code content
            let actual_source = service.get_contract_source_content(&contract_id, &file_path)?;

            // Verify source content exists
            assert!(
                actual_source.is_some(),
                "Actual source `{file_path}` is missing in the store."
            );

            // Read the source content from the tar archive
            let expected_source = read_file_from_tar(tar_file, &file_path)?;

            // Verify actual source matches expected results
            assert_eq!(
                actual_source.unwrap(),
                expected_source,
                "Actual source does not match expected results `{file_path}`."
            );
        }

        Ok(())
    }

    /// Auxiliary function that reads the contents of specified `file_path` within a tar archive.
    fn read_file_from_tar(tar_path: &str, file_path: &str) -> Result<String> {
        let file = File::open(tar_path)?;
        let mut archive = Archive::new(file);
        for entry in archive.entries()? {
            let mut entry = entry?;
            if let Ok(path) = entry.path() {
                if path == Path::new(file_path) {
                    let mut content = String::new();
                    entry.read_to_string(&mut content)?;
                    return Ok(content);
                }
            }
        }

        Err(Custom(format!("File {file_path} not found in tar archive.")))
    }

    /// Auxiliary function that extracts all file paths from the given `tar_bytes` tar archive.
    pub fn extract_file_paths_from_tar(tar_bytes: &[u8]) -> Result<Vec<String>> {
        let cursor = Cursor::new(tar_bytes);
        let mut archive = Archive::new(cursor);

        // Collect paths from the tar archive
        let mut file_paths = Vec::new();
        for entry in archive.entries()? {
            let entry = entry?;
            let path = entry.path()?;

            // Skip directories and only include files
            if entry.header().entry_type().is_file() {
                file_paths.push(path.to_string_lossy().to_string());
            }
        }

        Ok(file_paths)
    }
}
