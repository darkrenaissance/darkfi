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

use sled::{Batch, IVec};

struct SledCache(BTreeMap<IVec, IVec>);

impl SledCache {
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
pub struct SledOverlay {
    pub tree: sled::Tree,
    cache: SledCache,
    removed: BTreeMap<IVec, IVec>,
}

impl SledOverlay {
    pub fn new(db: &sled::Tree) -> Self {
        Self { tree: db.clone(), cache: SledCache::new(), removed: BTreeMap::new() }
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

    pub fn aggregate(&self) -> sled::Batch {
        let mut batch = Batch::default();

        for (k, v) in self.cache.iter() {
            batch.insert(k, v);
        }

        for k in self.removed.keys() {
            batch.remove(k);
        }

        batch
    }
}
