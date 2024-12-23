/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use tinyjson::JsonValue;

use darkfi::{Error, Result};
use darkfi_sdk::blockchain::block_epoch;

use crate::{metrics_store::GasMetrics, ExplorerService};

#[derive(Debug, Clone)]
/// Structure representing basic statistic extracted from the database.
pub struct BaseStatistics {
    /// Current blockchain height
    pub height: u32,
    /// Current blockchain epoch (based on current height)
    pub epoch: u8,
    /// Blockchains' last block hash
    pub last_block: String,
    /// Blockchain total blocks
    pub total_blocks: usize,
    /// Blockchain total transactions
    pub total_txs: usize,
}

impl BaseStatistics {
    /// Auxiliary function to convert `BaseStatistics` into a `JsonValue` array.
    pub fn to_json_array(&self) -> JsonValue {
        JsonValue::Array(vec![
            JsonValue::Number(self.height as f64),
            JsonValue::Number(self.epoch as f64),
            JsonValue::String(self.last_block.clone()),
            JsonValue::Number(self.total_blocks as f64),
            JsonValue::Number(self.total_txs as f64),
        ])
    }
}

/// Structure representing metrics extracted from the database.
#[derive(Default)]
pub struct MetricStatistics {
    /// Metrics used to store explorer statistics
    pub metrics: GasMetrics,
}

impl MetricStatistics {
    pub fn new(metrics: GasMetrics) -> Self {
        Self { metrics }
    }

    /// Auxiliary function to convert [`MetricStatistics`] into a [`JsonValue`] array.
    pub fn to_json_array(&self) -> JsonValue {
        JsonValue::Array(vec![
            JsonValue::Number(self.metrics.avg_total_gas_used() as f64),
            JsonValue::Number(self.metrics.total_gas.min as f64),
            JsonValue::Number(self.metrics.total_gas.max as f64),
            JsonValue::Number(self.metrics.avg_wasm_gas_used() as f64),
            JsonValue::Number(self.metrics.wasm_gas.min as f64),
            JsonValue::Number(self.metrics.wasm_gas.max as f64),
            JsonValue::Number(self.metrics.avg_zk_circuits_gas_used() as f64),
            JsonValue::Number(self.metrics.zk_circuits_gas.min as f64),
            JsonValue::Number(self.metrics.zk_circuits_gas.max as f64),
            JsonValue::Number(self.metrics.avg_signatures_gas_used() as f64),
            JsonValue::Number(self.metrics.signatures_gas.min as f64),
            JsonValue::Number(self.metrics.signatures_gas.max as f64),
            JsonValue::Number(self.metrics.avg_deployments_gas_used() as f64),
            JsonValue::Number(self.metrics.deployments_gas.min as f64),
            JsonValue::Number(self.metrics.deployments_gas.max as f64),
            JsonValue::Number(self.metrics.timestamp.inner() as f64),
        ])
    }
}
impl ExplorerService {
    /// Fetches the latest [`BaseStatistics`] from the explorer database, or returns `None` if no block exists.
    pub fn get_base_statistics(&self) -> Result<Option<BaseStatistics>> {
        let last_block = self.last_block();
        Ok(last_block
            // Throw database error if last_block retrievals fails
            .map_err(|e| {
                Error::DatabaseError(format!(
                    "[get_base_statistics] Retrieving last block failed: {:?}",
                    e
                ))
            })?
            // Calculate base statistics and return result
            .map(|(height, header_hash)| {
                let epoch = block_epoch(height);
                let total_blocks = self.get_block_count();
                let total_txs = self.get_transaction_count();
                BaseStatistics { height, epoch, last_block: header_hash, total_blocks, total_txs }
            }))
    }

    /// Fetches the latest metrics from the explorer database, returning a vector of
    /// [`MetricStatistics`] if found, or an empty Vec if no metrics exist.
    pub async fn get_metrics_statistics(&self) -> Result<Vec<MetricStatistics>> {
        // Fetch all metrics from the metrics store, handling any potential errors
        let metrics = self.db.metrics_store.get_all_metrics().map_err(|e| {
            Error::DatabaseError(format!(
                "[get_metrics_statistics] Retrieving metrics failed: {:?}",
                e
            ))
        })?;

        // Transform the fetched metrics into `MetricStatistics`, collect them into a vector
        let metric_statistics =
            metrics.iter().map(|metrics| MetricStatistics::new(metrics.clone())).collect();

        Ok(metric_statistics)
    }

    /// Fetches the latest metrics from the explorer database, returning [`MetricStatistics`] if found,
    /// or zero-initialized defaults when not.
    pub async fn get_latest_metrics_statistics(&self) -> Result<MetricStatistics> {
        // Fetch the latest metrics, handling any potential errors
        match self.db.metrics_store.get_last().map_err(|e| {
            Error::DatabaseError(format!(
                "[get_metrics_statistics] Retrieving latest metrics failed: {:?}",
                e
            ))
        })? {
            // Transform metrics into `MetricStatistics` when found
            Some((_, metrics)) => Ok(MetricStatistics::new(metrics)),
            // Return default statistics when no metrics exist
            None => Ok(MetricStatistics::default()),
        }
    }
}
