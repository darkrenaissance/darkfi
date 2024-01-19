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

use std::process::exit;

use rusqlite::types::Value;

use darkfi::{zk::halo2::Field, Result};
use darkfi_money_contract::client::{
    MONEY_INFO_COL_LAST_SCANNED_SLOT, MONEY_INFO_TABLE, MONEY_TREE_COL_TREE, MONEY_TREE_TABLE,
};
use darkfi_sdk::{
    crypto::{MerkleNode, MerkleTree},
    pasta::pallas,
};
use darkfi_serial::serialize;

use crate::{
    error::{WalletDbError, WalletDbResult},
    Drk,
};

impl Drk {
    /// Initialize wallet with tables for the Money contract
    pub async fn initialize_money(&self) -> Result<()> {
        // Initialize Money wallet schema
        let wallet_schema = include_str!("../../../src/contract/money/wallet.sql");
        if let Err(e) = self.wallet.exec_batch_sql(wallet_schema).await {
            eprintln!("Error initializing Money schema: {e:?}");
            exit(2);
        }

        // Check if we have to initialize the Merkle tree.
        // We check if we find a row in the tree table, and if not, we create a
        // new tree and push it into the table.
        // For now, on success, we don't care what's returned, but in the future
        // we should actually check it.
        if self.wallet.query_single(MONEY_TREE_TABLE, vec![MONEY_TREE_COL_TREE], &[]).await.is_err()
        {
            eprintln!("Initializing Money Merkle tree");
            let mut tree = MerkleTree::new(100);
            tree.append(MerkleNode::from(pallas::Base::ZERO));
            let _ = tree.mark().unwrap();
            self.put_money_tree(&tree).await?;
            eprintln!("Successfully initialized Merkle tree for the Money contract");
        }

        // We maintain the last scanned slot as part of the Money contract,
        // but at this moment it is also somewhat applicable to DAO scans.
        if self.last_scanned_slot().await.is_err() {
            let query = format!(
                "INSERT INTO {} ({}) VALUES (?1);",
                MONEY_INFO_TABLE, MONEY_INFO_COL_LAST_SCANNED_SLOT
            );
            if let Err(e) = self.wallet.exec_sql(&query, rusqlite::params![0]).await {
                eprintln!("Error inserting last scanned slot: {e:?}");
                exit(2);
            }
        }

        Ok(())
    }

    /// Replace the Money Merkle tree in the wallet.
    pub async fn put_money_tree(&self, tree: &MerkleTree) -> Result<()> {
        // First we remove old record
        let query = format!("DELETE FROM {};", MONEY_TREE_TABLE);
        if let Err(e) = self.wallet.exec_sql(&query, &[]).await {
            eprintln!("Error removing Money tree: {e:?}");
            exit(2);
        }

        // then we insert the new one
        let query =
            format!("INSERT INTO {} ({}) VALUES (?1);", MONEY_TREE_TABLE, MONEY_TREE_COL_TREE,);
        if let Err(e) = self.wallet.exec_sql(&query, rusqlite::params![serialize(tree)]).await {
            eprintln!("Error replacing Money tree: {e:?}");
            exit(2);
        }

        Ok(())
    }

    /// Get the last scanned slot from the wallet
    pub async fn last_scanned_slot(&self) -> WalletDbResult<u64> {
        let ret = self
            .wallet
            .query_single(MONEY_INFO_TABLE, vec![MONEY_INFO_COL_LAST_SCANNED_SLOT], &[])
            .await?;
        let Value::Integer(slot) = ret[0] else {
            return Err(WalletDbError::ParseColumnValueError);
        };
        let Ok(slot) = u64::try_from(slot) else {
            return Err(WalletDbError::ParseColumnValueError);
        };

        Ok(slot)
    }
}
