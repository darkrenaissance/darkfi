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
    fmt,
    sync::{Arc, Mutex, MutexGuard},
};

use log::{debug, info};
use sled_overlay::{sled, SledDbOverlay};

use darkfi::{
    blockchain::SledDbOverlayPtr,
    util::time::{DateTime, Timestamp},
    validator::fees::GasData,
    Error, Result,
};
use darkfi_sdk::{num_traits::ToBytes, tx::TransactionHash};
use darkfi_serial::{async_trait, deserialize, serialize, SerialDecodable, SerialEncodable};

/// Gas metrics tree name.
pub const SLED_GAS_METRICS_TREE: &[u8] = b"_gas_metrics";

/// Gas metrics `by_height` tree that contains all metrics by height.
pub const SLED_GAS_METRICS_BY_HEIGHT_TREE: &[u8] = b"_gas_metrics_by_height";

/// Transaction gas data tree name.
pub const SLED_TX_GAS_DATA_TREE: &[u8] = b"_tx_gas_data";

/// The time interval for [`GasMetricsKey`]s in the main tree, specified in seconds.
/// Metrics are stored in hourly intervals (3600 seconds), meaning all metrics accumulated
/// within a specific hour are stored using a key representing the start of that hour.
pub const GAS_METRICS_KEY_TIME_INTERVAL: u64 = 3600;

#[derive(Debug, Clone, Default, Eq, PartialEq, SerialEncodable, SerialDecodable)]
/// Represents metrics used to capture key statistical data.
pub struct Metrics {
    /// An aggregate value that represents the sum of the metrics.
    pub sum: u64,
    /// The smallest value in the series of measured metrics.
    pub min: u64,
    /// The largest value in the series of measured metrics.
    pub max: u64,
}

// Temporarily disable unused warnings until the store is integrated with the explorer
#[allow(dead_code)]
impl Metrics {
    /// Constructs a [`Metrics`] instance with provided parameters.
    pub fn new(sum: u64, min: u64, max: u64) -> Self {
        Self { sum, min, max }
    }
}

/// Structure for managing gas metrics across all transactions in the store.
///
/// This struct maintains running totals, extrema, and transaction counts to efficiently calculate
/// metrics without the need to iterate through previous transactions when new data is added. It is used to build a
/// comprehensive view of gas metrics across the blockchain's history, including total gas, WASM gas,
/// ZK circuit gas, and signature gas. The structure allows for O(1) performance in calculating
/// averages and updating min/max values.
#[derive(Clone, Default, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct GasMetrics {
    /// Represents the total count of transactions tracked by the metrics store.
    pub txs_count: u64,
    /// Overall gas consumed metrics across all transactions.
    pub total_gas: Metrics,
    /// Gas used across all executed wasm transactions.
    pub wasm_gas: Metrics,
    /// Gas consumed across all zk circuit computations.
    pub zk_circuits_gas: Metrics,
    /// Gas used metrics related to signatures across transactions.
    pub signatures_gas: Metrics,
    /// Gas consumed for deployments across transactions.
    pub deployments_gas: Metrics,
    /// The time the metrics was calculated
    pub timestamp: Timestamp,
}

// Temporarily disable unused warnings until the store is integrated with the explorer
#[allow(dead_code)]
impl GasMetrics {
    /// Creates a [`GasMetrics`] instance.
    pub fn new(
        txs_count: u64,
        total_gas: Metrics,
        wasm_gas: Metrics,
        zk_circuit_gas: Metrics,
        signature_gas: Metrics,
        deployment_gas: Metrics,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            txs_count,
            total_gas,
            wasm_gas,
            zk_circuits_gas: zk_circuit_gas,
            signatures_gas: signature_gas,
            deployments_gas: deployment_gas,
            timestamp,
        }
    }

    /// Provides the average of the total gas used.
    pub fn avg_total_gas_used(&self) -> u64 {
        self.total_gas.sum.checked_div(self.txs_count).unwrap_or_default()
    }

    /// Provides the average of the gas used across WASM transactions.
    pub fn avg_wasm_gas_used(&self) -> u64 {
        self.wasm_gas.sum.checked_div(self.txs_count).unwrap_or_default()
    }

    /// Provides the average of the gas consumed across Zero-Knowledge Circuit computations.
    pub fn avg_zk_circuits_gas_used(&self) -> u64 {
        self.zk_circuits_gas.sum.checked_div(self.txs_count).unwrap_or_default()
    }

    /// Provides the average of the gas used to sign transactions.
    pub fn avg_signatures_gas_used(&self) -> u64 {
        self.signatures_gas.sum.checked_div(self.txs_count).unwrap_or_default()
    }

    /// Provides the average of the gas used for deployments.
    pub fn avg_deployments_gas_used(&self) -> u64 {
        self.deployments_gas.sum.checked_div(self.txs_count).unwrap_or_default()
    }

    /// Adds new [`GasData`] to the existing accumulated values.
    ///
    /// This method updates running totals, transaction counts, and min/max values
    /// for various gas metric categories. It accumulates new data without reading existing
    /// averages, minimums, or maximums from the database to optimize performance.
    pub fn add(&mut self, tx_gas_data: &[GasData]) {
        for gas_data in tx_gas_data {
            // Increment number of transactions included in stats
            self.txs_count += 1;

            // Update the statistics related to total gas
            self.total_gas.sum += gas_data.total_gas_used();

            // Update the statistics related to WASM gas
            self.wasm_gas.sum += gas_data.wasm;

            // Update the statistics related to ZK circuit gas
            self.zk_circuits_gas.sum += gas_data.zk_circuits;

            // Update the statistics related to signature gas
            self.signatures_gas.sum += gas_data.signatures;

            // Update the statistics related to deployment gas
            self.deployments_gas.sum += gas_data.deployments;

            if self.txs_count == 1 {
                // For the first transaction, set min/max to the transaction values
                self.total_gas.min = gas_data.total_gas_used();
                self.total_gas.max = gas_data.total_gas_used();
                self.wasm_gas.min = gas_data.wasm;
                self.wasm_gas.max = gas_data.wasm;
                self.zk_circuits_gas.min = gas_data.zk_circuits;
                self.zk_circuits_gas.max = gas_data.zk_circuits;
                self.signatures_gas.min = gas_data.signatures;
                self.signatures_gas.max = gas_data.signatures;
                self.deployments_gas.min = gas_data.deployments;
                self.deployments_gas.max = gas_data.deployments;
                return;
            }

            // For subsequent transactions, compare with min/max
            self.total_gas.min = self.total_gas.min.min(gas_data.total_gas_used());
            self.total_gas.max = self.total_gas.max.max(gas_data.total_gas_used());
            self.wasm_gas.min = self.wasm_gas.min.min(gas_data.wasm);
            self.wasm_gas.max = self.wasm_gas.max.max(gas_data.wasm);
            self.zk_circuits_gas.min = self.zk_circuits_gas.min.min(gas_data.zk_circuits);
            self.zk_circuits_gas.max = self.zk_circuits_gas.max.max(gas_data.zk_circuits);
            self.signatures_gas.min = self.signatures_gas.min.min(gas_data.signatures);
            self.signatures_gas.max = self.signatures_gas.max.max(gas_data.signatures);
            self.deployments_gas.min = self.deployments_gas.min.min(gas_data.deployments);
            self.deployments_gas.max = self.deployments_gas.max.max(gas_data.deployments);
        }
    }
}

/// Debug formatting support for [`GasMetrics`] instances to include averages.
impl fmt::Debug for GasMetrics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GasMetrics")
            .field("txs_count", &self.txs_count)
            .field("avg_total_gas_used", &self.avg_total_gas_used())
            .field("avg_wasm_gas_used", &self.avg_wasm_gas_used())
            .field("avg_zk_circuits_gas_used", &self.avg_zk_circuits_gas_used())
            .field("avg_signatures_gas_used", &self.avg_signatures_gas_used())
            .field("avg_deployments_gas_used", &self.avg_deployments_gas_used())
            .field("total_gas", &format_args!("{:?}", self.total_gas))
            .field("wasm_gas", &format_args!("{:?}", self.wasm_gas))
            .field("zk_circuits_gas", &format_args!("{:?}", self.zk_circuits_gas))
            .field("signatures_gas", &format_args!("{:?}", self.signatures_gas))
            .field("deployments_gas", &format_args!("{:?}", self.deployments_gas))
            .field("timestamp", &self.timestamp)
            .finish()
    }
}

/// The `MetricStore` serves as the entry point for managing metrics,
/// offering an API for fetching, inserting, and resetting metrics backed by a Sled database.
///
/// It organizes data into separate Sled trees, including main storage for gas metrics by a defined time interval,
/// a tree containing metrics by height for handling reorgs, and a transaction-specific gas data tree.
/// Different keys, such as gas metric keys, block heights, and transaction hashes, are used to handle
/// various use cases.
///
/// The `MetricStore` utilizes an overlay pattern for write operations, allowing unified management of metrics,
/// by internally delegating write-related actions like adding metrics and handling reorgs to [`MetricsStoreOverlay`].
#[derive(Clone)]
pub struct MetricsStore {
    /// Pointer to the underlying sled database used by the store and its associated overlay
    pub sled_db: sled::Db,

    /// Primary sled tree for storing gas metrics, utilizing [`GasMetricsKey`] as keys and
    /// serialized [`GasMetrics`] as values.
    pub main: sled::Tree,

    /// Sled tree for storing gas metrics by height, utilizing block `height` as keys
    /// and serialized [`GasMetrics`] as values.
    pub by_height: sled::Tree,

    /// Sled tree for storing transaction gas data, utilizing [`TransactionHash`] inner value as keys
    /// and serialized [`GasData`] as values.
    pub tx_gas_data: sled::Tree,
}

// Temporarily disable unused warnings until the store is integrated with the explorer
#[allow(dead_code)]
impl MetricsStore {
    /// Creates a [`MetricsStore`] instance by opening the necessary trees in the provided sled database [`Db`]
    pub fn new(db: &sled::Db) -> Result<Self> {
        let main = db.open_tree(SLED_GAS_METRICS_TREE)?;
        let tx_gas_data = db.open_tree(SLED_TX_GAS_DATA_TREE)?;
        let metrcs_by_height = db.open_tree(SLED_GAS_METRICS_BY_HEIGHT_TREE)?;

        Ok(Self { sled_db: db.clone(), main, tx_gas_data, by_height: metrcs_by_height })
    }

    /// Fetches [`GasMetrics`]s associated with the provided slice of [`GasMetricsKey`]s.
    pub fn get(&self, keys: &[GasMetricsKey]) -> Result<Vec<GasMetrics>> {
        let mut ret = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some(metrics_bytes) = self.main.get(key.to_sled_key())? {
                let metrics = deserialize(&metrics_bytes).map_err(Error::from)?;
                ret.push(metrics);
            }
        }
        Ok(ret)
    }

    /// Fetches [`GasMetrics`]s associated with the provided slice of [`u32`] heights.
    pub fn get_by_height(&self, heights: &[u32]) -> Result<Vec<GasMetrics>> {
        let mut ret = Vec::with_capacity(heights.len());
        for height in heights {
            if let Some(metrics_bytes) = self.by_height.get(height.to_be_bytes())? {
                let metrics = deserialize(&metrics_bytes).map_err(Error::from)?;
                ret.push(metrics);
            }
        }
        Ok(ret)
    }

    /// Fetches the most recent [`GasMetrics`] and its associated [`GasMetricsKey`] from the main tree,
    /// returning `None` if no metrics are found.
    pub fn get_last(&self) -> Result<Option<(GasMetricsKey, GasMetrics)>> {
        self.main
            .last()?
            .map(|(key_bytes, metrics_bytes)| {
                // Deserialize gas metrics key and value
                let key = GasMetricsKey::from_sled_key(&key_bytes)?;
                let metrics: GasMetrics = deserialize(&metrics_bytes).map_err(Error::from)?;
                debug!(target: "explorerd::metrics_store::get_last", "Deserialized metrics at key {key}: {metrics:?}");
                Ok((key, metrics))
            })
            .transpose()
    }

    /// Fetches all [`GasMetrics`] from the main tree without corresponding key, returning an empty `Vec`
    /// if no metrics are found.
    pub fn get_all_metrics(&self) -> Result<Vec<GasMetrics>> {
        // Iterate through all metrics, deserialize each one, and collect results
        self.main
            .iter()
            .map(|iter_result| match iter_result {
                Ok((_, metrics_bytes)) => deserialize(&metrics_bytes).map_err(Error::from),
                Err(e) => Err(Error::from(e)),
            })
            .collect()
    }

    /// Fetches the most recent [`GasMetrics`] and its associated `height` from the `by_height` tree, returning `None` if no metrics are found.
    pub fn get_last_by_height(&self) -> Result<Option<(u32, GasMetrics)>> {
        self.by_height
            .last()?
            .map(|(height_bytes, metrics_bytes)| {
                // Deserialize height key and value
                let key_bytes: [u8; 4] = height_bytes.as_ref().try_into().unwrap();
                let height = u32::from_be_bytes(key_bytes);
                let metrics: GasMetrics = deserialize(&metrics_bytes).map_err(Error::from)?;
                debug!(target: "explorerd::metrics_store::get_last_by_height", "Deserialized metrics at height {height:?}: {metrics:?}");
                Ok((height, metrics))
            })
            .transpose()
    }

    /// Fetches the [`GasData`] associated with the provided [`TransactionHash`], or `None` if no gas data is found.
    pub fn get_tx_gas_data(&self, tx_hash: &TransactionHash) -> Result<Option<GasData>> {
        // Query transaction gas data tree using provided hash
        let opt = self.tx_gas_data.get(tx_hash.inner())?;

        // Deserialize gas data, map error if needed, return result
        opt.map(|value| deserialize(&value).map_err(Error::from)).transpose()
    }

    /// Adds gas metrics for a specific block of transactions to the store.
    ///
    /// This function takes block `height`, [`Timestamp`], with associated pairs of [`TransactionHash`] and [`GasData`],
    /// and updates the accumulated gas metrics in the store. It handles the storage of metrics for both regular use and
    /// blockchain reorganizations.
    ///
    /// Delegates operation to [`MetricsStoreOverlay::insert_gas_metrics`], whose documentation
    /// provides more details.
    pub fn insert_gas_metrics(
        &self,
        block_height: u32,
        block_timestamp: &Timestamp,
        tx_hashes: &[TransactionHash],
        tx_gas_data: &[GasData],
    ) -> Result<GasMetricsKey> {
        let overlay = MetricsStoreOverlay::new(self.sled_db.clone())?;
        overlay.insert_gas_metrics(block_height, block_timestamp, tx_hashes, tx_gas_data)
    }

    /// Resets the gas metrics in the store to a specified `height` [`u32`].
    ///
    /// This function reverts all gas metrics data after the given height, effectively
    /// undoing changes made beyond that point. It's useful for handling blockchain
    /// reorganizations.
    ///
    /// Delegates operation to [`MetricsStoreOverlay::reset_gas_metrics`], whose documentation
    /// provides more details.
    pub fn reset_gas_metrics(&self, height: u32) -> Result<()> {
        let overlay = MetricsStoreOverlay::new(self.sled_db.clone())?;
        overlay.reset_gas_metrics(height)
    }

    /// Checks if provided [`GasMetricsKey`] exists in the store's main tree.
    pub fn contains(&self, key: &GasMetricsKey) -> Result<bool> {
        Ok(self.main.contains_key(key.to_sled_key())?)
    }

    /// Provides the number of stored metrics in the main tree.
    pub fn len(&self) -> usize {
        self.main.len()
    }

    /// Provides the number of stored metrics by height.
    pub fn len_by_height(&self) -> usize {
        self.by_height.len()
    }

    /// Returns the number of transaction gas usage metrics stored.
    pub fn len_tx_gas_data(&self) -> usize {
        self.tx_gas_data.len()
    }

    /// Checks if there are any gas metrics stored.
    pub fn is_empty(&self) -> bool {
        self.main.is_empty()
    }

    /// Checks if transaction gas data metrics are stored.
    pub fn is_empty_tx_gas_data(&self) -> bool {
        self.tx_gas_data.is_empty()
    }
}

/// The `MetricsStoreOverlay` provides write operations for managing metrics in conjunction with the
/// underlying sled database. It supports inserting new [`GasData`] into the stored accumulated metrics,
/// adding transaction gas data, and reverting metric changes after a specified height.
struct MetricsStoreOverlay {
    /// Pointer to the overlay used for accessing and performing database write operations to the store.
    overlay: SledDbOverlayPtr,
    /// Pointer managed by the [`MetricsStore`] that references the sled instance on which the overlay operates.
    db: sled::Db,
}

impl MetricsStoreOverlay {
    /// Instantiate a [`MetricsStoreOverlay`] over the provided [`SledDbPtr`] instance.
    pub fn new(db: sled::Db) -> Result<Self> {
        // Create overlay pointer
        let overlay = Arc::new(Mutex::new(SledDbOverlay::new(&db, vec![])));

        // Open trees
        overlay.lock().unwrap().open_tree(SLED_GAS_METRICS_TREE, true)?;
        overlay.lock().unwrap().open_tree(SLED_GAS_METRICS_BY_HEIGHT_TREE, true)?;
        overlay.lock().unwrap().open_tree(SLED_TX_GAS_DATA_TREE, true)?;

        Ok(Self { overlay: overlay.clone(), db })
    }

    /// Adds the provided [`TransactionHash`] and [`GasData`] pairs to the accumulated [`GasMetrics`]
    /// in the store's [`SLED_GAS_METRICS_BY_HEIGHT_TREE`] and [`SLED_GAS_METRICS_TREE`] trees, while
    /// also storing transaction gas data in the [`SLED_TX_GAS_DATA_TREE`], committing all changes upon success.
    ///
    /// This function retrieves the latest recorded metrics, updates them with the new gas data, and
    /// stores the accumulated result. It uses the provided `block_timestamp` to create a normalied time-sequenced
    /// [`GasMetricsKey`] for metrics storage. The `block_height` is used as a key to store metrics by height
    /// which are used to handle chain reorganizations. After updating the aggregate metrics, it stores
    /// the transaction gas data for each transaction in the block.
    ///
    /// Returns the created [`GasMetricsKey`] that can be used to retrieve the metric upon success.
    pub fn insert_gas_metrics(
        &self,
        block_height: u32,
        block_timestamp: &Timestamp,
        tx_hashes: &[TransactionHash],
        tx_gas_data: &[GasData],
    ) -> Result<GasMetricsKey> {
        // Ensure lengths of tx_hashes and gas_data arrays match
        if tx_hashes.len() != tx_gas_data.len() {
            return Err(Error::Custom(String::from(
                "The lengths of tx_hashes and gas_data arrays must match",
            )));
        }

        // Ensure gas data is provided
        if tx_gas_data.is_empty() {
            return Err(Error::Custom(String::from("No transaction gas data was provided")));
        }

        // Lock the database
        let mut lock = self.overlay.lock().unwrap();

        // Retrieve latest recorded metrics, returning default if not exist
        let mut metrics = match self.get_last_by_height(&mut lock)? {
            None => GasMetrics::default(),
            Some((_, metrics)) => metrics,
        };

        // Update the accumulated metrics with the provided transaction gas data
        metrics.add(tx_gas_data);

        // Update the time that the metrics was recorded
        metrics.timestamp = *block_timestamp;

        // Insert metrics by height
        self.insert_by_height(&[block_height], &[metrics.clone()], &mut lock)?;

        // Create metrics key based on block_timestamp
        let metrics_key = GasMetricsKey::new(block_timestamp)?;

        // Normalize metric timestamp based on the key's time interval
        metrics.timestamp = GasMetricsKey::normalize_timestamp(block_timestamp)?;

        // Insert the gas metrics using metrics key
        self.insert(&[metrics_key.clone()], &[metrics], &mut lock)?;

        // Insert the transaction gas data for each transaction in the block
        self.insert_tx_gas_data(tx_hashes, tx_gas_data, &mut lock)?;

        // Commit the changes
        lock.apply()?;

        Ok(metrics_key)
    }

    /// Inserts [`TransactionHash`] and [`GasData`] pairs into the store's [`SLED_TX_GAS_DATA_TREE`],
    /// committing the changes upon success.
    ///
    /// This function locks the overlay, verifies that the tx_hashes and gas_data arrays have matching lengths,
    /// then inserts them into the store while handling serialization and potential errors. Returns a
    /// successful result upon success.
    fn insert_tx_gas_data(
        &self,
        tx_hashes: &[TransactionHash],
        gas_data: &[GasData],
        lock: &mut MutexGuard<SledDbOverlay>,
    ) -> Result<()> {
        // Ensure lengths of tx_hashes and gas_data arrays match
        if tx_hashes.len() != gas_data.len() {
            return Err(Error::Custom(String::from(
                "The lengths of tx_hashes and gas_data arrays must match",
            )));
        }

        // Insert each transaction hash and gas data pair
        for (tx_hash, gas_data) in tx_hashes.iter().zip(gas_data.iter()) {
            // Serialize the gas data
            let serialized_gas_data = serialize(gas_data);

            // Insert serialized gas data
            lock.insert(SLED_TX_GAS_DATA_TREE, tx_hash.inner(), &serialized_gas_data)?;
            info!(target: "explorerd::metrics_store::insert_tx_gas_data", "Inserted gas data for transaction {}: {gas_data:?}", tx_hash);
        }

        Ok(())
    }

    /// Resets gas metrics in the [`SLED_GAS_METRICS_TREE`] and [`SLED_GAS_METRICS_BY_HEIGHT_TREE`]
    /// to a specified block height, undoing all entries after provided height and committing the
    /// changes upon success.
    ///
    /// This function first obtains a lock on the overlay, then reverts changes by calling
    /// [`Self::revert_by_height_metrics`] and [`Self::revert_metrics`]. Upon successful revert,
    /// all modifications made after the specified height are permanently reverted.
    pub fn reset_gas_metrics(&self, height: u32) -> Result<()> {
        // Obtain lock
        let mut lock = self.overlay.lock().unwrap();

        // Revert the metrics by height
        self.revert_by_height_metrics(height, &mut lock)?;

        // Revert the main metrics entries now that `by_height` tree is reset
        self.revert_metrics(&mut lock)?;

        // Commit the changes
        lock.apply()?;

        Ok(())
    }

    /// Inserts [`GasMetricsKey`] and [`GasMetrics`] pairs into the store's [`SLED_GAS_METRICS_TREE`].
    ///
    /// This function verifies that the provided keys and metrics arrays have matching lengths,
    /// then inserts each pair while handling serialization. Returns a successful result
    /// if all insertions are completed without errors.
    fn insert(
        &self,
        keys: &[GasMetricsKey],
        metrics: &[GasMetrics],
        lock: &mut MutexGuard<SledDbOverlay>,
    ) -> Result<()> {
        // Ensure lengths of keys and metrics match
        if keys.len() != metrics.len() {
            return Err(Error::Custom(String::from(
                "The lengths of keys and metrics arrays must match",
            )));
        }

        // Insert each metric corresponding to respective gas metrics key
        for (key, metric) in keys.iter().zip(metrics.iter()) {
            // Insert metric
            lock.insert(SLED_GAS_METRICS_TREE, &key.to_sled_key(), &serialize(metric))?;
            info!(target: "explorerd::metrics_store::insert", "Added gas metrics using key {key}: {metric:?}");
        }

        Ok(())
    }

    /// Inserts provided [`u32`] height and [`GasMetrics`] pairs into the store's [`SLED_GAS_METRICS_BY_HEIGHT_TREE`].
    ///
    /// This function verifies matching lengths of provided heights and metrics arrays,
    /// and inserts each pair while handling serialization and errors. Returns a successful result
    /// if all insertions are completed without errors.
    fn insert_by_height(
        &self,
        heights: &[u32],
        metrics: &[GasMetrics],
        lock: &mut MutexGuard<SledDbOverlay>,
    ) -> Result<()> {
        // Ensure lengths of heights and metrics match
        if heights.len() != metrics.len() {
            return Err(Error::Custom(String::from(
                "The lengths of heights and metrics arrays must match",
            )));
        }

        // Insert each metric corresponding to respective height
        for (height, metric) in heights.iter().zip(metrics.iter()) {
            // Serialize the metric and handle potential errors
            let serialized_metric = serialize(metric);

            // Insert the serialized metric
            lock.insert(
                SLED_GAS_METRICS_BY_HEIGHT_TREE,
                &height.to_be_bytes(),
                &serialized_metric,
            )?;
            info!(target: "explorerd::metrics_store::insert_by_height", "Added gas metrics using height {height}: {metric:?}");
        }

        Ok(())
    }

    /// This function reverts gas metric entries in the [`SLED_GAS_METRICS_TREE`] to align
    /// with the latest metrics state in the [`SLED_GAS_METRICS_BY_HEIGHT_TREE`].
    ///
    /// It first determines the target timestamp to revert to based on the latest entry
    /// in the by_height tree timestamp. Then, it iteratively removes entries from the main metrics
    /// tree that are newer than the target timestamp. Once all that is complete, it adds the latest
    /// metrics by height to the main metrics tree, returning a successful result if revert processes
    /// without error.
    fn revert_metrics(&self, lock: &mut MutexGuard<SledDbOverlay>) -> Result<()> {
        /*** Determine Metrics To Revert ***/

        // Get the last metrics by height and determine the target timestamp to revert to
        let latest_by_height = self.get_last_by_height(lock)?;
        let target_timestamp = match &latest_by_height {
            None => 0,
            Some((_, metrics)) => GasMetricsKey::normalize_timestamp(&metrics.timestamp)?.inner(),
        };

        // Get the timestamp of the latest metrics entry in the metrics store
        let mut current_timestamp = match self.get_last(lock)? {
            None => return Ok(()),
            Some((_, metrics)) => metrics.timestamp.inner(),
        };

        /*** Revert Main Tree Gas Metrics ***/

        // Iterate through at most the total number of gas metric tree entries
        for _ in 0..self.db.open_tree(SLED_GAS_METRICS_TREE)?.len() {
            // Stop the loop if the current timestamp is less than or equal to the target timestamp,
            // as there are no more entries to revert
            if current_timestamp <= target_timestamp {
                break;
            }

            // Create a `GasMetricsKey` for the current timestamp to locate the entry to be reverted.
            let key_to_revert = GasMetricsKey::new(current_timestamp)?;

            // Remove the corresponding entry from the gas metrics tree.
            lock.remove(SLED_GAS_METRICS_TREE, &key_to_revert.to_sled_key())?;
            info!(target: "explorerd:metrics_store:revert_metrics", "Successfully reverted metrics with key: {}", key_to_revert);

            // Move to the previous valid timestamp by subtracting the defined time interval
            current_timestamp = current_timestamp.saturating_sub(GAS_METRICS_KEY_TIME_INTERVAL);
        }

        /*** Add the Latest Reverted Metrics To Main Tree ***/

        // Retrieve the latest metrics from the `by_height` tree and normalize its timestamp so it can be added to the main tree.
        // If there are no metrics in the `by_height` tree, we may have reset to 0, so return as there is nothing add.
        let latest_metrics = match latest_by_height {
            None => return Ok(()),
            Some((_, mut metrics)) => {
                metrics.timestamp = GasMetricsKey::normalize_timestamp(&metrics.timestamp)?;
                metrics
            }
        };

        // Add the latest metrics to the main tree based on latest reverted metrics by height
        let gas_metrics_key = GasMetricsKey::new(&latest_metrics.timestamp)?;
        self.insert(&[gas_metrics_key], &[latest_metrics], lock)?;

        Ok(())
    }

    /// Reverts gas metric entries from [`SLED_GAS_METRICS_BY_HEIGHT_TREE`] to provided `height`.
    ///
    /// This function iterates through the entries in gas metrics by height tree and removes all entries
    /// with heights greater than the specified `height`, effectively reverting all gas metrics beyond that point.
    fn revert_by_height_metrics(
        &self,
        height: u32,
        lock: &mut MutexGuard<SledDbOverlay>,
    ) -> Result<()> {
        // Retrieve the last stored block height
        let (last_height, _) = match self.get_last_by_height(lock)? {
            None => return Ok(()),
            Some(v) => v,
        };

        // Return early if the requested height is after the last stored height
        if height >= last_height {
            return Ok(());
        }

        // Remove keys greater than `height`
        while let Some((cur_height_bytes, _)) = lock.last(SLED_GAS_METRICS_BY_HEIGHT_TREE)? {
            // Convert height bytes to u32
            let cur_height = u32::from_be_bytes(cur_height_bytes.as_ref().try_into()?);

            // Process all heights that are bigger than provided `height`
            if cur_height <= height {
                break;
            }

            // Remove height being reverted
            lock.remove(SLED_GAS_METRICS_BY_HEIGHT_TREE, &cur_height_bytes)?;
            info!(target: "explorerd:metrics_store:revert_by_height_metrics", "Successfully reverted metrics with height: {}", cur_height);
        }

        Ok(())
    }

    /// Fetches the most recent gas metrics from [`SLED_GAS_METRICS_TREE`], returning an option
    /// containing a metrics key [`GasMetricsKey`] and [`GasMetrics`] pair, or `None` if no metrics exist.
    fn get_last(
        &self,
        lock: &mut MutexGuard<SledDbOverlay>,
    ) -> Result<Option<(GasMetricsKey, GasMetrics)>> {
        // Fetch and deserialize key and metric pair
        lock.last(SLED_GAS_METRICS_TREE)?
            .map(|(key_bytes, metrics_bytes)| {
                // Deserialize the metrics key
                let key = GasMetricsKey::from_sled_key(&key_bytes)?;
                // Deserialize the stored gas metrics
                let metrics: GasMetrics = deserialize(&metrics_bytes).map_err(Error::from)?;
                Ok((key, metrics))
            })
            .transpose()
    }

    /// Fetches the most recent gas metrics from [`SLED_GAS_METRICS_BY_HEIGHT_TREE`], returning an option
    /// containing a height [`u32`] and [`GasMetrics`] pair, or `None` if no metrics exist.
    fn get_last_by_height(
        &self,
        lock: &mut MutexGuard<SledDbOverlay>,
    ) -> Result<Option<(u32, GasMetrics)>> {
        // Fetch and deserialize height and metric pair
        lock.last(SLED_GAS_METRICS_BY_HEIGHT_TREE)?
            .map(|(height_bytes, metrics_bytes)| {
                // Deserialize the height
                let key_bytes: [u8; 4] = height_bytes.as_ref().try_into().unwrap();
                let height = u32::from_be_bytes(key_bytes);
                // Deserialize the stored gas metrics
                let metrics: GasMetrics = deserialize(&metrics_bytes).map_err(Error::from)?;
                Ok((height, metrics))
            })
            .transpose()
    }
}

/// Represents a key used to store and fetch metrics in the metrics store.
///
/// This struct provides methods for creating, serializing, and deserializing gas metrics keys.
/// It supports creation from various time representations through the [`GasMetricsKeySource`] trait
/// and offers conversion methods for use with a sled database.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct GasMetricsKey(pub DateTime);

impl GasMetricsKey {
    /// Creates a new [`GasMetricsKey`] from a source that implements [`GasMetricsKeySource`].
    /// Depending on the use case, the key supports different input sources such as `Timestamp`, `u64` timestamp,
    /// or `&str` timestamp to create the key.
    pub fn new<T: GasMetricsKeySource>(source: T) -> Result<GasMetricsKey> {
        source.to_key()
    }

    /// Gets the inner [`DateTime`] value.
    pub fn inner(&self) -> &DateTime {
        &self.0
    }

    /// Converts the [`GasMetricsKey`] into a key suitable for use with a sled database.
    pub fn to_sled_key(&self) -> Vec<u8> {
        // Create a new vector with a capacity of 28 bytes
        let mut sled_key = Vec::with_capacity(28);

        // Push the byte representations of each field into the vector
        sled_key.extend_from_slice(&self.inner().year.to_be_bytes());
        sled_key.extend_from_slice(&self.inner().month.to_be_bytes());
        sled_key.extend_from_slice(&self.inner().day.to_be_bytes());
        sled_key.extend_from_slice(&self.inner().hour.to_be_bytes());
        sled_key.extend_from_slice(&self.inner().min.to_be_bytes());
        sled_key.extend_from_slice(&self.inner().sec.to_be_bytes());
        sled_key.extend_from_slice(&self.inner().nanos.to_be_bytes());

        // Return sled key
        sled_key
    }

    /// Converts a `sled` key into a [`GasMetricsKey`] by deserializing a slice of bytes.
    pub fn from_sled_key(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 28 {
            return Err(Error::Custom(String::from("Invalid byte length for GasMetricsKey")));
        }

        // Deserialize byte representations into each field
        let key = DateTime {
            year: u32::from_be_bytes(bytes[0..4].try_into()?),
            month: u32::from_be_bytes(bytes[4..8].try_into()?),
            day: u32::from_be_bytes(bytes[8..12].try_into()?),
            hour: u32::from_be_bytes(bytes[12..16].try_into()?),
            min: u32::from_be_bytes(bytes[16..20].try_into()?),
            sec: u32::from_be_bytes(bytes[20..24].try_into()?),
            nanos: u32::from_be_bytes(bytes[24..28].try_into()?),
        };

        Ok(Self(key))
    }

    /// Normalizes the given [`DateTime`] to the start of hour.
    pub fn normalize_date_time(date_time: DateTime) -> DateTime {
        DateTime {
            nanos: 0,
            sec: 0,
            min: 0,
            hour: date_time.hour,
            day: date_time.day,
            month: date_time.month,
            year: date_time.year,
        }
    }

    /// Normalizes a given [`Timestamp`] to the start of the hour.
    pub fn normalize_timestamp(timestamp: &Timestamp) -> Result<Timestamp> {
        let remainder = timestamp.inner() % GAS_METRICS_KEY_TIME_INTERVAL;
        timestamp.checked_sub(Timestamp::from_u64(remainder))
    }
}

impl fmt::Display for GasMetricsKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner())
    }
}

/// Provides a unified method for creating new instances of GasMetricKeys using
/// various time representations: [`Timestamp`], `u64` timestamp, or `&str` timestamp.
pub trait GasMetricsKeySource {
    fn to_key(&self) -> Result<GasMetricsKey>;
}

/// Implements [`GasMetricsKeySource`] for &[`Timestamp`], converting it to a [`GasMetricsKey`].
impl GasMetricsKeySource for &Timestamp {
    fn to_key(&self) -> Result<GasMetricsKey> {
        let date_time = DateTime::from_timestamp(self.inner(), 0);
        Ok(GasMetricsKey(GasMetricsKey::normalize_date_time(date_time)))
    }
}

/// Implements [`GasMetricsKeySource`] for `u64`, converting it to a [`GasMetricsKey`].
impl GasMetricsKeySource for u64 {
    fn to_key(&self) -> Result<GasMetricsKey> {
        let date_time = DateTime::from_timestamp(*self, 0);
        Ok(GasMetricsKey(GasMetricsKey::normalize_date_time(date_time)))
    }
}

/// Implements [`GasMetricsKeySource`] for string slices, converting a `&str` in the `YYYY-MM-DD HH:mm:ss UTC` format
/// to a [`GasMetricsKey`]. Returns an [`Error::ParseFailed`] error if the provided timestamp string slice is invalid.
impl GasMetricsKeySource for &str {
    fn to_key(&self) -> Result<GasMetricsKey> {
        let date_time = DateTime::from_timestamp_str(self)?;
        Ok(GasMetricsKey(GasMetricsKey::normalize_date_time(date_time)))
    }
}

#[cfg(test)]
/// This test module verifies the correct insertion, retrieval, and reset of metrics in the store.
/// It covers adding metrics, searching metrics by time and transaction hash, and resetting metrics with specified heights.
mod tests {

    use darkfi::util::time::DateTime;
    use std::{
        str::FromStr,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };
    use structopt::lazy_static::lazy_static;

    use super::*;
    use crate::test_utils::init_logger;

    /// Number of heights to simulate.
    const HEIGHT: u32 = 10;

    /// Fixed timestamp in seconds since UNIX epoch.
    const FIXED_TIMESTAMP: u64 = 1732042800;

    /// [`FIXED_TIMESTAMP`] timestamp as a string in UTC format.
    const FIXED_TIMESTAMP_STR: &str = "2024-11-19T19:00:00";

    lazy_static! {
        /// Test transaction hash.
        pub static ref TX_HASH: TransactionHash = TransactionHash::from_str(
            "92225ff00a3755d8df93c626b59f6e36cf021d85ebccecdedc38f3f1890a15fc"
        ).expect("Invalid transaction hash");
    }
    /// Tests inserting gas metrics, verifying the correctness of stored metrics.
    #[test]
    fn test_insert_gas_metrics() -> Result<()> {
        // Declare constants used for test
        const EXPECTED_HEIGHT: usize = HEIGHT as usize - 1;

        // Setup test, returning initialized metrics store
        let store = setup()?;

        // Load test data into the store and get the expected metrics results
        let test_data = load_random_metrics(&store, |_, _| {})?;

        // Verify metrics were inserted with the expected counts
        assert_eq!(store.len(), EXPECTED_HEIGHT);

        // Process height 0 test data separately
        let mut test_data_iter = test_data.iter();

        // For height 0, confirm there are no metrics stored in the store
        if let Some(test_data_height0) = test_data_iter.next() {
            let actual_height0 = store.get(&[GasMetricsKey::new(&test_data_height0.timestamp)?])?;
            assert!(
                actual_height0.is_empty(),
                "Timestamp associated with height 0 should not have any metrics stored"
            );
        }

        // Process remaining test data, verifying that each stored metric matches expected results
        for expected in test_data_iter {
            let actual = store.get(&[GasMetricsKey::new(&expected.timestamp)?])?;
            let expected_normalized = normalize_metrics_timestamp(expected)?;
            assert_eq!(&expected_normalized, &actual[0]);
        }

        Ok(())
    }

    /// Tests inserting gas metrics into the `by_height` tree, verifying the correctness of stored metrics.
    #[test]
    fn test_insert_by_height_gas_metrics() -> Result<()> {
        // Declare constants used for test
        const EXPECTED_HEIGHT: usize = HEIGHT as usize - 1;

        // Setup test, returning initialized metrics store
        let store = setup()?;

        // Load test data into the store and get the expected metrics results
        let test_data = load_random_metrics(&store, |_, _| {})?;

        // Verify metrics were inserted with the expected counts
        assert_eq!(store.len(), EXPECTED_HEIGHT);

        // For height 0, confirm there are no metrics stored in metrics by height
        let actual_height0 = store.get_by_height(&[0])?;
        assert!(actual_height0.is_empty(), "Height 0 should not have any metrics stored");

        // Process remaining heights, verifying that each stored metric matches expected results
        for (height, expected) in (1..).zip(test_data.iter().skip(1)) {
            let actual = store.get_by_height(&[height])?;
            assert!(!actual.is_empty(), "No metrics found for height {}", height);
            assert_eq!(expected, &actual[0]);
        }

        Ok(())
    }

    /// Tests searching gas metrics by the hour, verifying the correct metrics are found
    /// and match expected values.
    #[test]
    fn test_search_metrics_by_hour() -> Result<()> {
        // Setup test, returning initialized metrics store
        let store = setup()?;

        // Load test data, initializing expected with the fourth loaded record
        let expected = &load_random_metrics(&store, |_, _| {})?[3];

        // Create search criteria based on the expected timestamp value
        let search_criteria = DateTime::from_timestamp(expected.timestamp.inner(), 0);

        // Search metrics by the hour
        let actual_opt = store.main.iter().find_map(|res| {
            res.ok().and_then(|(k, v)| {
                let key = GasMetricsKey::from_sled_key(&k).ok()?;
                if key.inner().hour == search_criteria.hour {
                    deserialize::<GasMetrics>(&v).ok()
                } else {
                    None
                }
            })
        });

        // Verify the found metrics match expected results
        assert!(actual_opt.is_some());
        assert_eq!(normalize_metrics_timestamp(expected)?, actual_opt.unwrap());

        Ok(())
    }

    /// Tests fetching gas metrics by a timestamp string, verifying the retrieved metrics
    /// match expected values.
    #[test]
    fn test_get_metrics_by_timestamp_str() -> Result<()> {
        // Setup test, returning initialized metrics store
        let store = setup()?;

        // Load fixed data needed for test, initializing expected with the first loaded record
        let (expected, _) = &load_fixed_metrics(&store)?[0];

        // Create gas metrics key using a test fixed timestamp
        let gas_metrics_key = GasMetricsKey::new(FIXED_TIMESTAMP_STR)?;

        // Verify the key retrieves the correct metrics and matches the expected value
        let actual = store.get(&[gas_metrics_key])?;
        assert_eq!(expected, &actual[0]);
        Ok(())
    }

    /// Tests the insertion and retrieval of transaction gas data in the store, verifying expected results.
    /// Additionally, it tests that transactions not found in the store correctly return a `None` result.
    #[test]
    fn test_tx_gas_data() -> Result<()> {
        let tx_hash_not_found: TransactionHash = TransactionHash::from_str(
            "93325ff00a3755d8df93c626b59f6e36cf021d85ebccecdedc38f3f1890a15fc",
        )
        .expect("Invalid hash");

        // Setup test, returning initialized metrics store
        let store = setup()?;
        // Load data needed for test, initializing expected with the first loaded record
        let (_, expected) = &load_fixed_metrics(&store)?[0];

        // Verify that existing transaction is found
        let actual_opt = store.get_tx_gas_data(&TX_HASH)?;
        assert!(actual_opt.is_some());
        assert_eq!(*expected, actual_opt.unwrap());

        // Verify that transactions that do not exist return None result
        let actual_not_found = store.get_tx_gas_data(&tx_hash_not_found)?;
        assert_eq!(None, actual_not_found);

        Ok(())
    }

    /// Tests resetting gas metrics within a specified height range, verifying that both the `by_height` and `main` trees
    /// are properly set to the reset height.
    #[test]
    fn test_reset_metrics_within_height_range() -> Result<()> {
        // Declare constants used for test
        const RESET_HEIGHT: u32 = 6;

        // Setup test, returning initialized metrics store
        let store = setup()?;

        // Load test data into the store and get the expected reset metrics result
        let expected = load_reset_metrics(&store, RESET_HEIGHT)?;

        // Reset metrics
        store.reset_gas_metrics(RESET_HEIGHT)?;

        // Fetch reset metrics by height
        let actual_by_height_opt = store.get_last_by_height()?;
        assert!(actual_by_height_opt.is_some(), "Expected get_last_by_height to return metrics");

        // Verify metrics by height are properly reset
        let (_, actual_by_height) = actual_by_height_opt.unwrap();
        assert_eq!(&expected, &actual_by_height);

        // Fetch reset main metrics
        let actual_main_opt = store.get_last()?;
        assert!(actual_main_opt.is_some(), "Expected get_last to return metrics");

        // Verify main metrics are properly reset
        let (_, actual_main_metrics) = actual_main_opt.unwrap();
        assert_eq!(&normalize_metrics_timestamp(&expected)?, &actual_main_metrics);

        Ok(())
    }

    /// Tests resetting the metrics store to height 0, ensuring it handles the operation gracefully without errors
    /// and verifies that no metrics remain in the store afterward.
    #[test]
    fn test_reset_metrics_height_to_0() -> Result<()> {
        // Declare constants used for test
        const RESET_HEIGHT: u32 = 0;
        const EXPECTED_RESET_HEIGHT: usize = 0;

        // Setup test, returning initialized metrics store
        let store = setup()?;

        // Load reset test data needed for test
        _ = load_reset_metrics(&store, RESET_HEIGHT)?;

        // Reset metrics
        store.reset_gas_metrics(RESET_HEIGHT)?;

        // Verify metrics were reset with the expected counts
        assert_eq!(store.len_by_height(), EXPECTED_RESET_HEIGHT);
        assert_eq!(store.len(), EXPECTED_RESET_HEIGHT);

        // Verify metrics by height are empty
        let actual_by_height_opt = store.get_last_by_height()?;
        assert!(actual_by_height_opt.is_none(), "Expected None from get_last_by_height");

        // Confirm main metrics are empty
        let actual_main_opt = store.get_last()?;
        assert!(actual_main_opt.is_none(), "Expected None from get_last");

        Ok(())
    }

    /// Tests that resetting beyond the number of available metrics does not change
    /// the store and no errors are thrown since there are no metrics to reset.
    #[test]
    fn test_reset_metrics_beyond_height() -> Result<()> {
        // Declare constants used for test
        const RESET_HEIGHT: u32 = HEIGHT + 1;
        const EXPECTED_RESET_HEIGHT: usize = HEIGHT as usize - 1;

        // Setup test, returning initialized metrics store
        let store = setup()?;

        // Load reset test data needed for test, storing the expected result
        let expected = load_reset_metrics(&store, RESET_HEIGHT)?;

        // Reset metrics to given height
        store.reset_gas_metrics(RESET_HEIGHT)?;

        // Verify metrics were reset with the expected counts
        assert_eq!(store.len_by_height(), EXPECTED_RESET_HEIGHT);
        assert_eq!(store.len(), EXPECTED_RESET_HEIGHT);

        // Verify that the last record for metrics by height is correctly reset
        let actual_by_height_opt = store.get_last_by_height()?;
        assert!(actual_by_height_opt.is_some(), "Expected get_last_by_height to return metrics");
        let (_, actual_by_height) = actual_by_height_opt.unwrap();
        assert_eq!(&expected, &actual_by_height);

        // Verify that the last record for main metrics is correctly reset
        let actual_main_opt = store.get_last()?;
        assert!(actual_main_opt.is_some(), "Expected get_last to return metrics");
        let (_, actual_main) = actual_main_opt.unwrap();
        assert_eq!(&normalize_metrics_timestamp(&expected)?, &actual_main);

        Ok(())
    }

    /// Tests resetting metrics at the last available height to verify that the code
    /// can handle the boundary condition.
    #[test]
    fn test_reset_metrics_at_height() -> Result<()> {
        // Declare constants used for test
        const RESET_HEIGHT: u32 = HEIGHT;
        const EXPECTED_RESET_HEIGHT: usize = HEIGHT as usize - 1;

        // Setup test, returning initialized metrics store
        let store = setup()?;

        // Load reset test data needed for test
        let expected = load_reset_metrics(&store, RESET_HEIGHT)?;

        // Reset metrics to given height
        store.reset_gas_metrics(RESET_HEIGHT)?;

        // Verify metrics were reset with the expected counts
        assert_eq!(store.len_by_height(), EXPECTED_RESET_HEIGHT);
        assert_eq!(store.len(), EXPECTED_RESET_HEIGHT);

        // Verify that the last record for metrics by height is correctly reset
        let actual_by_height_opt = store.get_last_by_height()?;
        assert!(actual_by_height_opt.is_some(), "Expected get_last_by_height to return metrics");
        let (_, actual_by_height) = actual_by_height_opt.unwrap();
        assert_eq!(&expected, &actual_by_height);

        // Verify that the last record for main metrics is correctly reset
        let actual_main_opt = store.get_last()?;
        assert!(actual_main_opt.is_some(), "Expected get_last to return metrics");
        let (_, actual_main) = actual_main_opt.unwrap();
        assert_eq!(&normalize_metrics_timestamp(&expected)?, &actual_main);

        Ok(())
    }

    /// Tests that resetting an empty metrics store gracefully handles
    /// the operation without errors and ensures the store remains empty.
    #[test]
    fn test_reset_empty_store() -> Result<()> {
        const RESET_HEIGHT: u32 = 6;

        // Setup test, returning initialized metrics store
        let store = setup()?;

        // Reset metrics with an empty store
        store.reset_gas_metrics(RESET_HEIGHT)?;

        // Verify no metrics with the expected counts
        assert_eq!(store.len_by_height(), 0);
        assert_eq!(store.len(), 0);

        // Verify that metrics by height is empty
        let actual_by_height = store.get_last_by_height()?;
        assert!(actual_by_height.is_none(), "Expected get_last_by_height to return None");

        // Verify main metrics is empty
        let actual_main = store.get_last()?;
        assert!(actual_main.is_none(), "Expected get_last to return None");

        Ok(())
    }

    /// Sets up a test case for metrics store testing by initializing the logger,
    /// creating a temporary database, and returning an initialized metrics store.
    fn setup() -> Result<MetricsStore> {
        // Initialize logger to show execution output
        init_logger(simplelog::LevelFilter::Off, vec!["sled", "runtime", "net"]);

        // Create a temporary directory for the sled database
        let db =
            sled::Config::new().temporary(true).open().expect("Unable to open test sled database");

        // Initialize the metrics store
        let metrics_store = MetricsStore::new(&db.clone())?;

        Ok(metrics_store)
    }

    /// Loads random test gas metrics data into the given metrics store, simulating height 0 as a
    /// genesis block with no metrics.
    ///
    /// Computes the starting block timestamp from the current system time for the first metric,
    /// then inserts each subsequent metric at intervals of [`GAS_METRICS_KEY_TIME_INTERVAL`],
    /// resulting in metrics being inserted one hour apart. The function iterates through a predefined
    /// height range, as defined by [`HEIGHT`], to accumulate and insert gas metrics. After each
    /// metric is stored, the `metric_loaded` closure is invoked, allowing the caller to perform
    /// specific actions as the data is loaded.
    ///
    /// NOTE: A fixed transaction hash is used to insert the metrics, as this test data is solely intended
    /// to validate gas metrics and not transaction-specific gas data.
    ///
    /// Upon success, it returns a list of snapshots of the accumulated metrics that were loaded.
    fn load_random_metrics<F>(
        metrics_store: &MetricsStore,
        mut metrics_loaded: F,
    ) -> Result<Vec<GasMetrics>>
    where
        F: FnMut(u32, &GasMetrics),
    {
        // Calculate the start block timestamp
        let start_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

        // Define variables to store accumulated loaded data
        let mut accumulated_metrics = GasMetrics::default();
        let mut metrics = Vec::with_capacity(HEIGHT as usize);

        // Iterate and load data
        for height in 0..HEIGHT {
            let timestamp_secs = (UNIX_EPOCH
                + Duration::from_secs(start_time + height as u64 * GAS_METRICS_KEY_TIME_INTERVAL))
            .duration_since(UNIX_EPOCH)?
            .as_secs();

            // Initialize simulated block_timestamp
            let block_timestamp = Timestamp::from(timestamp_secs);
            accumulated_metrics.timestamp = block_timestamp;

            // Simulate genesis block, metrics are stored after height 0
            if height > 0 {
                let tx_gas_data = random_gas_data(height as u64 + start_time);
                accumulated_metrics.add(&[tx_gas_data.clone()]);
                metrics_store.insert_gas_metrics(
                    height,
                    &block_timestamp,
                    &[*TX_HASH],
                    &[tx_gas_data],
                )?;
            }

            // Invoke passed in metrics loaded closure
            metrics_loaded(height, &accumulated_metrics);

            // Add a snapshot of the accumulated metrics
            metrics.push(accumulated_metrics.clone());
        }

        Ok(metrics)
    }

    /// Loads fixed test data into the metrics store using fixed timestamps,
    /// returning snapshots of accumulated [`GasMetrics`] with corresponding [`GasData`]
    /// used to update the metrics.
    ///
    /// Currently, this function only loads a single record but is designed to be extendable
    /// to insert additional records in the future without affecting the method's return signature,
    /// making it suitable for use in tests.
    fn load_fixed_metrics(metrics_store: &MetricsStore) -> Result<Vec<(GasMetrics, GasData)>> {
        // Convert the fixed timestamp constant to a Timestamp object
        let fixed_timestamp = Timestamp::from_u64(FIXED_TIMESTAMP);

        // Initialize an empty GasMetrics object to accumulate the data
        let height: u32 = 1;
        let mut accumulated_metrics = GasMetrics::default();
        let mut metrics_vec = Vec::with_capacity(HEIGHT as usize);

        // Initialize the block_timestamp using the fixed timestamp
        let block_timestamp = fixed_timestamp;
        accumulated_metrics.timestamp = block_timestamp;

        // Generate random gas data for the given height
        let gas_data = random_gas_data(height as u64);
        accumulated_metrics.add(&[gas_data.clone()]);

        // Insert the gas metrics into the metrics store
        metrics_store.insert_gas_metrics(
            height,
            &block_timestamp,
            &[*TX_HASH],
            &[gas_data.clone()],
        )?;
        metrics_vec.push((accumulated_metrics, gas_data));

        Ok(metrics_vec)
    }

    /// Loads reset test data into the store, returning the accumulated gas metrics at the specified reset height.
    fn load_reset_metrics(metrics_store: &MetricsStore, reset_height: u32) -> Result<GasMetrics> {
        let mut reset_metrics = GasMetrics::default();

        // Load metrics, passing in a closure to store the reset metrics
        _ = load_random_metrics(metrics_store, |height, acc_metrics| {
            // Store accumulated metrics at reset height
            if reset_height == height || reset_height >= HEIGHT {
                reset_metrics = acc_metrics.clone();
            }
        })?;

        Ok(reset_metrics)
    }

    /// Generates random [`GasData`] based on the provided seed value, allowing for the simulation
    /// of varied gas data values.
    fn random_gas_data(seed: u64) -> GasData {
        /// Defines a limit for gas data values.
        const GAS_LIMIT: u64 = 100_000;

        // Initialize gas usage with the provided seed
        let mut gas_used = seed;

        // Closure to generate a random gas value
        let mut random_gas = || {
            // Introduce variability using the seed and current gas_used
            let variation = seed.wrapping_add(gas_used);
            gas_used = gas_used.wrapping_mul(6364136223846793005).wrapping_add(variation);
            gas_used
        };

        // Create GasData with random values constrained by GAS_LIMIT
        GasData {
            paid: random_gas() % GAS_LIMIT,
            wasm: random_gas() % GAS_LIMIT,
            zk_circuits: random_gas() % GAS_LIMIT,
            signatures: random_gas() % GAS_LIMIT,
            deployments: random_gas() % GAS_LIMIT,
        }
    }

    /// Normalizes the [`GasMetrics`] timestamp to the start of the hour for test comparisons.
    fn normalize_metrics_timestamp(metrics: &GasMetrics) -> Result<GasMetrics> {
        let mut normalized_metrics = metrics.clone();
        normalized_metrics.timestamp = GasMetricsKey::normalize_timestamp(&metrics.timestamp)?;
        Ok(normalized_metrics)
    }
}
