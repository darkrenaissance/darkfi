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

use std::collections::HashMap;

use lazy_static::lazy_static;
use sled_overlay::sled;
use tracing::info;

use darkfi::{blockchain::Blockchain, error::Result, util::path::expand_path};

use darkfi_sdk::crypto::{DAO_CONTRACT_ID, DEPLOYOOOR_CONTRACT_ID, MONEY_CONTRACT_ID};

use crate::store::{contract_metadata::ContractMetaStore, metrics::MetricsStore};

/// Stores, manages, and provides access to explorer metrics
pub mod metrics;

/// Stores, manages, and provides access to contract metadata
pub mod contract_metadata;

/// Represents the explorer database backed by a `sled` database connection, responsible for maintaining
/// persistent state required for blockchain exploration. It serves as the core data layer for the Explorer application,
/// storing and managing blockchain data, metrics, and contract-related information.
pub struct ExplorerDb {
    /// The main `sled` database connection used for data storage and retrieval
    pub sled_db: sled::Db,
    /// Local copy of the Darkfi blockchain used for block synchronization and exploration
    pub blockchain: Blockchain,
    /// Store for tracking chain-related metrics
    pub metrics_store: MetricsStore,
    /// Store for managing contract metadata, source code, and related data
    pub contract_meta_store: ContractMetaStore,
}

impl ExplorerDb {
    /// Creates a new `ExplorerDb` instance
    pub fn new(db_path: String) -> Result<Self> {
        let db_path = expand_path(db_path.as_str())?;
        let sled_db = sled::open(&db_path)?;
        let blockchain = Blockchain::new(&sled_db)?;
        let metrics_store = MetricsStore::new(&sled_db)?;
        let contract_meta_store = ContractMetaStore::new(&sled_db)?;
        info!(target: "explorerd", "Initialized explorer database {}: block count: {}, tx count: {}", db_path.display(), blockchain.len(), blockchain.txs_len());
        Ok(Self { sled_db, blockchain, metrics_store, contract_meta_store })
    }
}

// Contract source archives used to bootstrap native contracts during explorer startup
lazy_static! {
    pub static ref NATIVE_CONTRACT_SOURCE_ARCHIVES: HashMap<String, &'static [u8]> = {
        let mut src_map = HashMap::new();
        src_map.insert(
            MONEY_CONTRACT_ID.to_string(),
            &include_bytes!("../../native_contracts_src/money_contract_src.tar")[..],
        );
        src_map.insert(
            DAO_CONTRACT_ID.to_string(),
            &include_bytes!("../../native_contracts_src/dao_contract_src.tar")[..],
        );
        src_map.insert(
            DEPLOYOOOR_CONTRACT_ID.to_string(),
            &include_bytes!("../../native_contracts_src/deployooor_contract_src.tar")[..],
        );
        src_map
    };
}
