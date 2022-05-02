use std::marker::PhantomData;

use log::debug;
use sled::Batch;

use crate::{
    util::serial::{deserialize, serialize, Decodable, Encodable},
    Result,
};

use super::primitives::{Log, NodeId};

const SLED_LOGS_TREE: &[u8] = b"_logs";
const SLED_COMMITS_TREE: &[u8] = b"_commits";
const _SLED_COMMITS_LENGTH_TREE: &[u8] = b"_commit_length";
const SLED_VOTED_FOR_TREE: &[u8] = b"_voted_for";
const SLED_CURRENT_TERM_TREE: &[u8] = b"_current_term";

pub struct DataStore<T> {
    _db: sled::Db,
    pub logs: DataTree<Log>,
    pub commits: DataTree<T>,
    pub voted_for: DataTree<Option<NodeId>>,
    pub current_term: DataTree<u64>,
}

impl<T: Encodable + Decodable> DataStore<T> {
    pub fn new(db_path: &str) -> Result<Self> {
        let _db = sled::open(db_path)?;
        let logs = DataTree::new(&_db, SLED_LOGS_TREE)?;
        let commits = DataTree::new(&_db, SLED_COMMITS_TREE)?;
        let voted_for = DataTree::new(&_db, SLED_VOTED_FOR_TREE)?;
        let current_term = DataTree::new(&_db, SLED_CURRENT_TERM_TREE)?;

        Ok(Self { _db, logs, commits, voted_for, current_term })
    }
    pub async fn cancel(&self) -> Result<()> {
        debug!(target: "raft", "DataStore flush");
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
        let datahash = blake3::hash(&serialized);
        self.tree.insert(datahash.as_bytes(), serialized)?;
        Ok(())
    }

    pub fn wipe_insert_all(&self, data: &Vec<T>) -> Result<()> {
        self.tree.clear()?;

        let mut batch = Batch::default();

        for i in data {
            let serialized = serialize(i);
            let hash = blake3::hash(&serialized);
            batch.insert(hash.as_bytes(), serialized);
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

    pub fn get_last(&self) -> Result<Option<T>> {
        if let Some(found) = self.tree.last()? {
            let da = deserialize(&found.1)?;
            return Ok(Some(da))
        }
        Ok(None)
    }
}
