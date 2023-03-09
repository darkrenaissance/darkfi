/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use std::collections::{btree_map::Iter, BTreeMap};

use sled::{
    transaction::{ConflictableTransactionError, TransactionError},
    Batch, IVec, Transactional,
};

#[derive(Debug, PartialEq)]
struct CacheNotFoundError;

struct TreeCache(BTreeMap<IVec, IVec>);

impl TreeCache {
    fn new() -> Self {
        Self(BTreeMap::new())
    }

    fn contains_key(&self, key: &IVec) -> bool {
        self.0.contains_key(key)
    }

    fn get(&self, key: &IVec) -> Option<IVec> {
        self.0.get(key).cloned()
    }

    fn insert(&mut self, key: IVec, value: IVec) -> Option<IVec> {
        self.0.insert(key, value)
    }

    fn remove(&mut self, key: &IVec) -> Option<IVec> {
        self.0.remove(key)
    }

    fn iter(&self) -> Iter<'_, IVec, IVec> {
        self.0.iter()
    }
}

/// We instantiate an overlay on top of a `sled::Tree` directly.
pub struct TreeOverlay {
    tree: sled::Tree,
    cache: TreeCache,
    removed: BTreeMap<IVec, IVec>,
}

impl TreeOverlay {
    pub fn new(db: &sled::Tree) -> Self {
        Self { tree: db.clone(), cache: TreeCache::new(), removed: BTreeMap::new() }
    }

    pub fn contains_key(&self, key: &[u8]) -> Result<bool, sled::Error> {
        if self.removed.contains_key::<IVec>(&key.into()) {
            return Ok(false)
        }

        if self.cache.contains_key(&key.into()) || self.tree.contains_key(key)? {
            return Ok(true)
        }

        Ok(false)
    }

    pub fn get(&self, key: &[u8]) -> Result<Option<IVec>, sled::Error> {
        if self.removed.contains_key::<IVec>(&key.into()) {
            return Ok(None)
        }

        if let Some(v) = self.cache.get(&key.into()) {
            return Ok(Some(v.clone()))
        }

        self.tree.get(key)
    }

    pub fn insert(&mut self, key: &[u8], value: &[u8]) -> Result<Option<IVec>, sled::Error> {
        let mut prev: Option<IVec> = self.cache.insert(key.into(), value.into());

        if self.removed.contains_key::<IVec>(&key.into()) {
            self.removed.remove(key);
            return Ok(None)
        }

        if prev.is_none() {
            prev = self.tree.get::<IVec>(key.into())?;
        }

        Ok(prev)
    }

    pub fn remove(&mut self, key: &[u8]) -> Result<Option<IVec>, sled::Error> {
        if self.removed.contains_key::<IVec>(&key.into()) {
            return Ok(None)
        }

        self.removed.insert(key.into(), vec![].into());

        Ok(self.cache.remove(&key.into()))
    }

    pub fn aggregate(&self) -> Option<sled::Batch> {
        if self.cache.0.is_empty() && self.removed.is_empty() {
            return None
        }
        let mut batch = Batch::default();

        for (k, v) in self.cache.iter() {
            batch.insert(k, v);
        }

        for k in self.removed.keys() {
            batch.remove(k);
        }

        Some(batch)
    }
}

/// We instantiate overlays on top of requested
/// sled tree keys.
pub struct SledOverlay2 {
    db: sled::Db,
    trees: BTreeMap<IVec, sled::Tree>,
    caches: BTreeMap<IVec, TreeOverlay>,
}

impl SledOverlay2 {
    pub fn new(db: &sled::Db, trees_keys: &[&str]) -> Result<Self, sled::Error> {
        let mut trees = BTreeMap::new();
        let mut caches = BTreeMap::new();
        for tree_key in trees_keys {
            let tree = db.open_tree(tree_key)?;
            let cache = TreeOverlay::new(&tree);
            trees.insert(tree_key.clone().into(), tree.clone());
            caches.insert(tree_key.clone().into(), cache);
        }
        Ok(Self { db: db.clone(), trees, caches })
    }

    fn get_cache(&self, tree_key: IVec) -> Result<&TreeOverlay, sled::Error> {
        if let Some(v) = self.caches.get(&tree_key) {
            return Ok(v)
        }
        Err(sled::Error::CollectionNotFound(tree_key.clone()))
    }

    fn get_cache_mut(&mut self, tree_key: IVec) -> Result<&mut TreeOverlay, sled::Error> {
        if let Some(v) = self.caches.get_mut(&tree_key) {
            return Ok(v)
        }
        Err(sled::Error::CollectionNotFound(tree_key.clone()))
    }

    pub fn contains_key(&self, tree_key: &str, key: &[u8]) -> Result<bool, sled::Error> {
        let cache = self.get_cache(tree_key.clone().into())?;
        cache.contains_key(key)
    }

    pub fn get(&self, tree_key: &str, key: &[u8]) -> Result<Option<IVec>, sled::Error> {
        let cache = self.get_cache(tree_key.clone().into())?;
        cache.get(key)
    }

    pub fn insert(
        &mut self,
        tree_key: &str,
        key: &[u8],
        value: &[u8],
    ) -> Result<Option<IVec>, sled::Error> {
        let cache = self.get_cache_mut(tree_key.clone().into())?;
        cache.insert(key, value)
    }

    pub fn remove(&mut self, tree_key: &str, key: &[u8]) -> Result<Option<IVec>, sled::Error> {
        let cache = self.get_cache_mut(tree_key.clone().into())?;
        cache.remove(key)
    }

    pub fn execute(&mut self) -> Result<(), TransactionError<sled::Error>> {
        let mut trees = vec![];
        let mut batches = vec![];
        for (key, tree) in &self.trees {
            let cache = self.get_cache(key.clone())?;
            if let Some(batch) = cache.aggregate() {
                trees.push(tree);
                batches.push(batch);
            }
        }

        trees.transaction(|trees| {
            for (index, tree) in trees.iter().enumerate() {
                tree.apply_batch(&batches[index])?;
            }

            Ok::<(), ConflictableTransactionError<sled::Error>>(())
        })?;

        self.db.flush()?;

        Ok(())
    }
}
