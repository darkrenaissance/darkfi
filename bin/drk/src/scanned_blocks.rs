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

use darkfi_serial::deserialize;

use crate::{
    cache::CacheOverlay,
    error::{WalletDbError, WalletDbResult},
    Drk,
};

impl Drk {
    /// Get a scanned block information record.
    pub fn get_scanned_block_hash(&self, height: &u32) -> WalletDbResult<String> {
        let Ok(query_result) = self.cache.scanned_blocks.get(height.to_be_bytes()) else {
            return Err(WalletDbError::QueryExecutionFailed);
        };
        let Some(hash_bytes) = query_result else {
            return Err(WalletDbError::RowNotFound);
        };
        let Ok(hash) = deserialize(&hash_bytes) else {
            return Err(WalletDbError::ParseColumnValueError);
        };
        Ok(hash)
    }

    /// Fetch all scanned block information records.
    pub fn get_scanned_block_records(&self) -> WalletDbResult<Vec<(u32, String)>> {
        let mut scanned_blocks = vec![];

        for record in self.cache.scanned_blocks.iter() {
            let Ok((key, value)) = record else {
                return Err(WalletDbError::QueryExecutionFailed);
            };
            let Ok(key) = deserialize(&key) else {
                return Err(WalletDbError::ParseColumnValueError);
            };
            let Ok(value) = deserialize(&value) else {
                return Err(WalletDbError::ParseColumnValueError);
            };
            scanned_blocks.push((key, value));
        }

        Ok(scanned_blocks)
    }

    /// Get the last scanned block height and hash from the wallet.
    /// If database is empty default (0, '-') is returned.
    pub fn get_last_scanned_block(&self) -> WalletDbResult<(u32, String)> {
        let Ok(query_result) = self.cache.scanned_blocks.last() else {
            return Err(WalletDbError::QueryExecutionFailed);
        };
        let Some((key, value)) = query_result else { return Ok((0, String::from("-"))) };
        let key: [u8; 4] = match key.as_ref().try_into() {
            Ok(k) => k,
            Err(_) => return Err(WalletDbError::ParseColumnValueError),
        };
        let key = u32::from_be_bytes(key);
        let Ok(value) = deserialize(&value) else {
            return Err(WalletDbError::ParseColumnValueError);
        };
        Ok((key, value))
    }

    /// Reset the scanned blocks information records in the cache.
    pub fn reset_scanned_blocks(&self, output: &mut Vec<String>) -> WalletDbResult<()> {
        output.push(String::from("Resetting scanned blocks"));
        if let Err(e) = self.cache.scanned_blocks.clear() {
            output.push(format!(
                "[reset_scanned_blocks] Resetting scanned blocks tree failed: {e:?}"
            ));
            return Err(WalletDbError::GenericError)
        }
        if let Err(e) = self.cache.state_inverse_diff.clear() {
            output.push(format!(
                "[reset_scanned_blocks] Resetting state inverse diffs tree failed: {e:?}"
            ));
            return Err(WalletDbError::GenericError)
        }
        output.push(String::from("Successfully reset scanned blocks"));

        Ok(())
    }

    /// Reset state to provided block height.
    /// If genesis block height(0) was provided, perform a full reset.
    pub fn reset_to_height(&self, height: u32, output: &mut Vec<String>) -> WalletDbResult<()> {
        output.push(format!("Resetting wallet state to block: {height}"));

        // If genesis block height(0) was provided,
        // perform a full reset.
        if height == 0 {
            return self.reset(output)
        }

        // Grab last scanned block height
        let (last, _) = self.get_last_scanned_block()?;

        // Check if requested height is after it
        if last <= height {
            output.push(String::from(
                "Requested block height is greater or equal to last scanned block",
            ));
            return Ok(())
        }

        // Create an overlay to apply the reverse diffs
        let mut overlay = match CacheOverlay::new(&self.cache) {
            Ok(o) => o,
            Err(e) => {
                output.push(format!("[reset_to_height] Creating cache overlay failed: {e:?}"));
                return Err(WalletDbError::GenericError)
            }
        };

        // Grab all state inverse diffs until requested height,
        // going backwards.
        for height in (height + 1..=last).rev() {
            let inverse_diff = match self.cache.get_state_inverse_diff(&height) {
                Ok(d) => d,
                Err(e) => {
                    output.push(format!(
                        "[reset_to_height] Retrieving state inverse diff from cache failed: {e:?}"
                    ));
                    return Err(WalletDbError::GenericError)
                }
            };

            // Apply it
            if let Err(e) = overlay.0.add_diff(&inverse_diff) {
                output.push(format!("[reset_to_height] Adding state inverse diff to the cache overlay failed: {e:?}"));
                return Err(WalletDbError::GenericError)
            }
            if let Err(e) = overlay.0.apply_diff(&inverse_diff) {
                output.push(format!("[reset_to_height] Applying state inverse diff to the cache overlay failed: {e:?}"));
                return Err(WalletDbError::GenericError)
            }

            // Remove it
            if let Err(e) = self.cache.state_inverse_diff.remove(height.to_be_bytes()) {
                output.push(format!(
                    "[reset_to_height] Removing state inverse diff from the cache failed: {e:?}"
                ));
                return Err(WalletDbError::GenericError)
            }

            // Flush sled
            if let Err(e) = self.cache.sled_db.flush() {
                output
                    .push(format!("[reset_to_height] Flushing cache sled database failed: {e:?}"));
                return Err(WalletDbError::GenericError)
            }
        }

        // Remove all wallet coins created after the reset height
        self.remove_money_coins_after(&height, output)?;

        // Unspent all wallet coins spent after the reset height
        self.unspent_money_coins_after(&height, output)?;

        // Unfreeze tokens mint authorities frozen after the reset
        // height.
        self.unfreeze_mint_authorities_after(&height, output)?;

        // Unconfirm DAOs minted after the reset height
        self.unconfirm_daos_after(&height, output)?;

        // Unconfirm DAOs proposals minted after the reset height
        self.unconfirm_dao_proposals_after(&height, output)?;

        // Reset execution information for DAOs proposals executed
        // after the reset height.
        self.unexec_dao_proposals_after(&height, output)?;

        // Remove all DAOs proposals votes created after the reset
        // height.
        self.remove_dao_votes_after(&height, output)?;

        // Unfreeze all contracts frozen after the reset height
        self.unfreeze_deploy_authorities_after(&height, output)?;

        // Set reverted status to all transactions executed after reset
        // height.
        self.revert_transactions_after(&height, output)?;

        output.push(String::from("Successfully reset wallet state"));
        Ok(())
    }
}
