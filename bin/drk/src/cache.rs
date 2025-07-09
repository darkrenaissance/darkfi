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

use darkfi::{blockchain::HeaderHash, Error, Result};
use darkfi_sdk::{
    crypto::{
        pasta_prelude::PrimeField,
        smt::{PoseidonFp, SparseMerkleTree, StorageAdapter, SMT_FP_DEPTH},
        MerkleTree,
    },
    error::{ContractError, ContractResult},
    pasta::pallas,
};
use darkfi_serial::{deserialize, serialize};
use num_bigint::BigUint;
use sled_overlay::{sled, SledDbOverlay, SledDbOverlayStateDiff};
use tracing::error;

pub const SLED_SCANNED_BLOCKS_TREE: &[u8] = b"_scanned_blocks";
pub const SLED_STATE_INVERSE_DIFF_TREE: &[u8] = b"_state_inverse_diff";
pub const SLED_MERKLE_TREES_TREE: &[u8] = b"_merkle_trees";
pub const SLED_MONEY_SMT_TREE: &[u8] = b"_money_smt";

/// Structure holding all sled trees that define the blockchain cache.
#[derive(Clone)]
pub struct Cache {
    /// Main pointer to the sled db connection
    pub sled_db: sled::Db,
    /// The `sled` tree storing the scanned blocks from the blockchain,
    /// where the key is the height number, and the value is the blocks'
    /// hash.
    pub scanned_blocks: sled::Tree,
    /// The `sled` tree storing each blocks' full database state inverse
    /// changes, where the key is the block height number, and the value
    /// is the serialized database inverse diff.
    pub state_inverse_diff: sled::Tree,
    /// The `sled` tree storing the merkle trees of the blockchain,
    /// where the key is the tree name, and the value is the serialized
    /// merkle tree itself.
    pub merkle_trees: sled::Tree,
    /// The `sled` tree storing the Sparse Merkle Tree of the Money
    /// contract.
    // TODO: this could be a map of trees so more contracts can open
    // SMTs if needed
    pub money_smt: sled::Tree,
    // TODO: Perhaps we should also move transactions history here
}

impl Cache {
    /// Instantiate a new `Cache` with the given `sled` database.
    pub fn new(db: &sled::Db) -> Result<Self> {
        let scanned_blocks = db.open_tree(SLED_SCANNED_BLOCKS_TREE)?;
        let state_inverse_diff = db.open_tree(SLED_STATE_INVERSE_DIFF_TREE)?;
        let merkle_trees = db.open_tree(SLED_MERKLE_TREES_TREE)?;
        let money_smt = db.open_tree(SLED_MONEY_SMT_TREE)?;

        Ok(Self {
            sled_db: db.clone(),
            scanned_blocks,
            state_inverse_diff,
            merkle_trees,
            money_smt,
        })
    }

    /// Execute an atomic sled batch corresponding to inserts to the
    /// merkle trees tree. For each record, the bytes slice is used as
    /// the key, and the serialized merkle tree is used as value.
    pub fn insert_merkle_trees(&self, trees: &[(&[u8], &MerkleTree)]) -> Result<()> {
        let mut batch = sled::Batch::default();
        for (key, tree) in trees {
            batch.insert(*key, serialize(*tree));
        }
        self.merkle_trees.apply_batch(batch)?;
        Ok(())
    }

    /// Insert a `u32` and a block inverse diff into store's inverse
    /// diffs tree. The block height is used as the key, and the
    /// serialized database inverse diff is used as value.
    pub fn insert_state_inverse_diff(
        &self,
        height: &u32,
        diff: &SledDbOverlayStateDiff,
    ) -> Result<()> {
        self.state_inverse_diff.insert(height.to_be_bytes(), serialize(diff))?;
        Ok(())
    }

    /// Fetch given block height number from the store's state inverse
    /// diffs tree. The function will fail if the block height number
    /// was not found.
    pub fn get_state_inverse_diff(&self, height: &u32) -> Result<SledDbOverlayStateDiff> {
        match self.state_inverse_diff.get(height.to_be_bytes())? {
            Some(found) => Ok(deserialize(&found)?),
            None => Err(Error::BlockStateInverseDiffNotFound(*height)),
        }
    }
}

/// Overlay structure over a [`Cache`] instance.
pub struct CacheOverlay(pub SledDbOverlay);

impl CacheOverlay {
    /// Instantiate a new `CacheOverlay` over the given [`Cache`] instance.
    pub fn new(cache: &Cache) -> Result<CacheOverlay> {
        // Here we configure all our cache sled trees to be protected in the overlay
        let protected_trees = vec![
            SLED_SCANNED_BLOCKS_TREE,
            SLED_STATE_INVERSE_DIFF_TREE,
            SLED_MERKLE_TREES_TREE,
            SLED_MONEY_SMT_TREE,
        ];
        let mut overlay = SledDbOverlay::new(&cache.sled_db, protected_trees);

        // Open all our cache sled trees in the overlay
        overlay.open_tree(SLED_SCANNED_BLOCKS_TREE, true)?;
        overlay.open_tree(SLED_STATE_INVERSE_DIFF_TREE, true)?;
        overlay.open_tree(SLED_MERKLE_TREES_TREE, true)?;
        overlay.open_tree(SLED_MONEY_SMT_TREE, true)?;

        Ok(Self(overlay))
    }

    /// Insert a `u32` and a block hash into overlay's scanned blocks
    /// tree. The block height is used as the key, and the serialized
    /// blockhash string is used as value.
    pub fn insert_scanned_block(&mut self, height: &u32, hash: &HeaderHash) -> Result<()> {
        self.0.insert(
            SLED_SCANNED_BLOCKS_TREE,
            &height.to_be_bytes(),
            &serialize(&hash.to_string()),
        )?;
        Ok(())
    }
}

pub type CacheSmt = SparseMerkleTree<
    'static,
    SMT_FP_DEPTH,
    { SMT_FP_DEPTH + 1 },
    pallas::Base,
    PoseidonFp,
    CacheSmtStorage,
>;

pub struct CacheSmtStorage {
    pub overlay: CacheOverlay,
    tree: Vec<u8>,
}

impl CacheSmtStorage {
    pub fn new(overlay: CacheOverlay, tree: &[u8]) -> Self {
        Self { overlay, tree: tree.to_vec() }
    }

    pub fn snapshot(&self) -> Result<HashMap<BigUint, pallas::Base>> {
        let mut smt = HashMap::new();
        for record in self.overlay.0.iter(&self.tree)? {
            let (key, value) = record?;
            let mut repr = [0; 32];
            repr.copy_from_slice(&value);
            let Some(value) = pallas::Base::from_repr(repr).into() else {
                return Err(Error::ParseFailed(
                    "[cache::CacheSmtStorage::snapshot] Value conversion failed",
                ))
            };
            smt.insert(BigUint::from_bytes_le(&key), value);
        }
        Ok(smt)
    }
}

impl StorageAdapter for CacheSmtStorage {
    type Value = pallas::Base;

    fn put(&mut self, key: BigUint, value: pallas::Base) -> ContractResult {
        if let Err(e) = self.overlay.0.insert(&self.tree, &key.to_bytes_le(), &value.to_repr()) {
            error!(target: "cache::StorageAdapter::put", "Inserting key {key:?}, value {value:?} into DB failed: {e}");
            return Err(ContractError::SmtPutFailed)
        }
        Ok(())
    }

    fn get(&self, key: &BigUint) -> Option<pallas::Base> {
        let value = match self.overlay.0.get(&self.tree, &key.to_bytes_le()) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "cache::StorageAdapter::get", "Fetching key {key:?} from DB failed: {e}");
                return None
            }
        };

        let value = value?;

        let mut repr = [0; 32];
        repr.copy_from_slice(&value);

        pallas::Base::from_repr(repr).into()
    }

    fn del(&mut self, key: &BigUint) -> ContractResult {
        if let Err(e) = self.overlay.0.remove(&self.tree, &key.to_bytes_le()) {
            error!(target: "cache::StorageAdapter::del", "Removing key {key:?} from DB failed: {e}");
            return Err(ContractError::SmtDelFailed)
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use darkfi::{zk::halo2::Field, Result};
    use darkfi_sdk::{
        crypto::smt::{gen_empty_nodes, util::FieldHasher, PoseidonFp, SparseMerkleTree},
        pasta::pallas,
    };
    use rand::rngs::OsRng;
    use sled_overlay::sled;

    use crate::cache::{Cache, CacheOverlay, CacheSmtStorage, SLED_MONEY_SMT_TREE};

    #[test]
    fn test_cache_smt() -> Result<()> {
        // Setup cache and its overlay
        let sled_db = sled::Config::new().temporary(true).open()?;
        let cache = Cache::new(&sled_db)?;
        let overlay = CacheOverlay::new(&cache)?;

        // Setup SMT
        const HEIGHT: usize = 3;
        let hasher = PoseidonFp::new();
        let empty_leaf = pallas::Base::ZERO;
        let empty_nodes = gen_empty_nodes::<{ HEIGHT + 1 }, _, _>(&hasher, empty_leaf);
        let store = CacheSmtStorage::new(overlay, SLED_MONEY_SMT_TREE);
        let mut smt = SparseMerkleTree::<HEIGHT, { HEIGHT + 1 }, _, _, _>::new(
            store,
            hasher.clone(),
            &empty_nodes,
        );

        // Verify database is empty
        assert!(cache.money_smt.is_empty());

        let leaves = vec![
            (pallas::Base::from(1), pallas::Base::random(&mut OsRng)),
            (pallas::Base::from(2), pallas::Base::random(&mut OsRng)),
            (pallas::Base::from(3), pallas::Base::random(&mut OsRng)),
        ];
        smt.insert_batch(leaves.clone()).unwrap();

        let hash1 = leaves[0].1;
        let hash2 = leaves[1].1;
        let hash3 = leaves[2].1;

        let hash = |l, r| hasher.hash([l, r]);

        let hash01 = hash(empty_nodes[3], hash1);
        let hash23 = hash(hash2, hash3);

        let hash0123 = hash(hash01, hash23);
        let root = hash(hash0123, empty_nodes[1]);
        assert_eq!(root, smt.root());

        // Now try to construct a membership proof for leaf 3
        let pos = leaves[2].0;
        let path = smt.prove_membership(&pos);
        assert_eq!(path.path[0], empty_nodes[1]);
        assert_eq!(path.path[1], hash01);
        assert_eq!(path.path[2], hash2);

        assert_eq!(hash23, hash(path.path[2], hash3));
        assert_eq!(hash0123, hash(path.path[1], hash(path.path[2], hash3)));
        assert_eq!(root, hash(hash(path.path[1], hash(path.path[2], hash3)), path.path[0]));

        assert!(path.verify(&root, &hash3, &pos));

        // Grab the overlay diff
        let diff = smt.store.overlay.0.diff(&[])?;

        // Apply the overlay
        smt.store.overlay.0.apply_diff(&diff)?;

        // Verify database contains keys
        assert!(!cache.money_smt.is_empty());

        // We are now going to rollback the changes
        smt.store.overlay.0.apply_diff(&diff.inverse())?;

        // Verify database is empty again
        assert!(cache.money_smt.is_empty());

        Ok(())
    }
}
