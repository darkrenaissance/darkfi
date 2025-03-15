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

use log::debug;

use darkfi::Result;

use crate::store::ExplorerDb;

/// Handles core block-related functionality
pub mod blocks;

/// Implements functionality for smart contracts
pub mod contracts;

/// Powers metrics gathering and analytical capabilities
pub mod statistics;

/// Manages transaction data processing
pub mod transactions;

/// Represents the service layer for the Explorer application, bridging the RPC layer and the database.
/// It encapsulates explorer business logic and provides a unified interface for core functionalities,
/// providing a clear separation of concerns between RPC handling and data management layers.
///
/// Core functionalities include:
///
/// - Data Transformation: Converting database data into structured responses suitable for RPC callers.
/// - Blocks: Synchronization, retrieval, counting, and management.
/// - Contracts: Handling native and user contract data, source code, tar files, and metadata.
/// - Metrics: Providing metric-related data over the life of the chain.
/// - Transactions: Synchronization, calculating gas data, retrieval, counting, and related block information.
pub struct ExplorerService {
    /// Explorer database instance
    pub db: ExplorerDb,
}

impl ExplorerService {
    /// Creates a new `ExplorerService` instance.
    pub fn new(db_path: String) -> Result<Self> {
        // Initialize explorer database
        let db = ExplorerDb::new(db_path)?;

        Ok(Self { db })
    }

    /// Initializes the explorer service by deploying native contracts and loading native contract
    /// source code and metadata required for its operation.
    pub async fn init(&self) -> Result<()> {
        self.deploy_native_contracts().await?;
        self.load_native_contract_sources()?;
        self.load_native_contract_metadata()?;
        Ok(())
    }

    /// Resets the explorer state to the specified height. If a genesis block height is provided,
    /// all blocks and transactions are purged from the database. Otherwise, the state is reverted
    /// to the given height. The explorer metrics are updated to reflect the updated blocks and
    /// transactions up to the reset height, ensuring consistency. Returns a result indicating
    /// success or an error if the operation fails.
    pub fn reset_explorer_state(&self, height: u32) -> Result<()> {
        debug!(target: "explorerd::reset_explorer_state", "Resetting explorer state to height: {height}");

        // Check if a genesis block reset or to a specific height
        match height {
            // Reset for genesis height 0, purge blocks and transactions
            0 => {
                self.reset_blocks()?;
                self.reset_transactions()?;
                debug!(target: "explorerd::reset_explorer_state", "Reset explorer state to accept a new genesis block");
            }
            // Reset for all other heights
            _ => {
                self.reset_to_height(height)?;
                debug!(target: "explorerd::reset_explorer_state", "Reset blocks to height: {height}");
            }
        }

        // Reset gas metrics to the specified height to reflect the updated blockchain state
        self.db.metrics_store.reset_gas_metrics(height)?;
        debug!(target: "explorerd::reset_explorer_state", "Reset metrics store to height: {height}");

        Ok(())
    }
}
