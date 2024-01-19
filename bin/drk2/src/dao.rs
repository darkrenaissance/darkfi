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

use darkfi::Result;
use darkfi_dao_contract::client::{
    DAO_TREES_COL_DAOS_TREE, DAO_TREES_COL_PROPOSALS_TREE, DAO_TREES_TABLE,
};
use darkfi_sdk::crypto::MerkleTree;
use darkfi_serial::serialize;

use crate::Drk;

impl Drk {
    /// Initialize wallet with tables for the DAO contract
    pub async fn initialize_dao(&self) -> Result<()> {
        // Initialize DAO wallet schema
        let wallet_schema = include_str!("../../../src/contract/dao/wallet.sql");
        if let Err(e) = self.wallet.exec_batch_sql(wallet_schema).await {
            eprintln!("Error initializing DAO schema: {e:?}");
            exit(2);
        }

        // Check if we have to initialize the Merkle trees.
        // We check if one exists, but we actually create two. This should be written
        // a bit better and safer.
        // For now, on success, we don't care what's returned, but in the future
        // we should actually check it.
        if self
            .wallet
            .query_single(DAO_TREES_TABLE, vec![DAO_TREES_COL_DAOS_TREE], &[])
            .await
            .is_err()
        {
            eprintln!("Initializing DAO Merkle trees");
            let tree = MerkleTree::new(100);
            self.put_dao_trees(&tree, &tree).await?;
            eprintln!("Successfully initialized Merkle trees for the DAO contract");
        }

        Ok(())
    }

    /// Replace the DAO Merkle trees in the wallet.
    pub async fn put_dao_trees(
        &self,
        daos_tree: &MerkleTree,
        proposals_tree: &MerkleTree,
    ) -> Result<()> {
        // First we remove old records
        let query = format!("DELETE FROM {};", DAO_TREES_TABLE);
        if let Err(e) = self.wallet.exec_sql(&query, &[]).await {
            eprintln!("Error removing DAO trees: {e:?}");
            exit(2);
        }

        // then we insert the new one
        let query = format!(
            "INSERT INTO {} ({}, {}) VALUES (?1, ?2);",
            DAO_TREES_TABLE, DAO_TREES_COL_DAOS_TREE, DAO_TREES_COL_PROPOSALS_TREE,
        );
        if let Err(e) = self
            .wallet
            .exec_sql(&query, rusqlite::params![serialize(daos_tree), serialize(proposals_tree)])
            .await
        {
            eprintln!("Error replacing DAO trees: {e:?}");
            exit(2);
        }

        Ok(())
    }
}
