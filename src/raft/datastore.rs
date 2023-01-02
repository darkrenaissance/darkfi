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

use std::marker::PhantomData;

use darkfi_serial::{deserialize, serialize, Decodable, Encodable};
use log::debug;
use sled::Batch;

use crate::{Error, Result};

use super::primitives::{Log, NodeId};

const SLED_LOGS_TREE: &[u8] = b"_logs";
const SLED_COMMITS_TREE: &[u8] = b"_commits";
const _SLED_COMMITS_LENGTH_TREE: &[u8] = b"_commit_length";
const SLED_VOTED_FOR_TREE: &[u8] = b"_voted_for";
const SLED_CURRENT_TERM_TREE: &[u8] = b"_current_term";
const SLED_ID_TREE: &[u8] = b"_id";

pub struct DataStore<T> {
    _db: sled::Db,
    pub logs: DataTree<Log>,
    pub commits: DataTree<T>,
    pub voted_for: DataTree<Option<NodeId>>,
    pub current_term: DataTree<u64>,
    pub id: DataTree<NodeId>,
}

impl<T: Encodable + Decodable> DataStore<T> {
    pub fn new(db_path: &str) -> Result<Self> {
        let _db = sled::open(db_path)?;
        let logs = DataTree::new(&_db, SLED_LOGS_TREE)?;
        let commits = DataTree::new(&_db, SLED_COMMITS_TREE)?;
        let voted_for = DataTree::new(&_db, SLED_VOTED_FOR_TREE)?;
        let current_term = DataTree::new(&_db, SLED_CURRENT_TERM_TREE)?;
        let id = DataTree::new(&_db, SLED_ID_TREE)?;

        Ok(Self { _db, logs, commits, voted_for, current_term, id })
    }
    pub async fn flush(&self) -> Result<()> {
        debug!(target: "raft::datastore", "DataStore flush");
        self._db.flush_async().await?;
        Ok(())
    }
}

pub struct DataTree<T> {
    tree: sled::Tree,
    phantom: PhantomData<T>,
}

impl<T: Decodable + Encodable> DataTree<T> {
    pub fn new(db: &sled::Db, tree_name: &[u8]) -> Result<Self> {
        let tree = db.open_tree(tree_name)?;
        Ok(Self { tree, phantom: PhantomData })
    }

    pub fn insert(&self, data: &T) -> Result<()> {
        let serialized = serialize(data);
        let last_index: u64 = if let Some(d) = self.tree.last()? {
            u64::from_be_bytes(d.0.to_vec().try_into().unwrap()) + 1
        } else {
            0
        };

        self.tree.insert(last_index.to_be_bytes(), serialized)?;
        Ok(())
    }

    pub fn wipe_insert_all(&self, data: &[T]) -> Result<()> {
        self.tree.clear()?;

        let mut batch = Batch::default();

        for (i, d) in data.iter().enumerate() {
            let serialized = serialize(d);
            batch.insert(&(i as u64).to_be_bytes(), serialized);
        }

        self.tree.apply_batch(batch)?;

        Ok(())
    }

    pub fn get_all(&self) -> Result<Vec<T>> {
        let mut ret: Vec<T> = Vec::new();

        for i in self.tree.iter() {
            let da = deserialize(&i?.1)?;
            ret.push(da)
        }

        Ok(ret)
    }

    pub fn len(&self) -> u64 {
        self.tree.len() as u64
    }

    pub fn get_last(&self) -> Result<Option<T>> {
        if let Some(found) = self.tree.last()? {
            let da = deserialize(&found.1)?;
            return Ok(Some(da))
        }
        Ok(None)
    }

    pub fn get(&self, index: u64) -> Result<T> {
        let index_bytes = index.to_be_bytes();
        if let Some(found) = self.tree.get(index_bytes)? {
            let da = deserialize(&found)?;
            return Ok(da)
        }
        Err(Error::RaftError(format!(
            "Unable to get the item with index {} {:?}",
            index,
            self.is_empty()
        )))
    }

    pub fn is_empty(&self) -> bool {
        self.tree.is_empty()
    }
}
