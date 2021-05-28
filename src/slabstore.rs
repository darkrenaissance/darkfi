use std::path::Path;
use std::sync::Arc;

use crate::serial::{deserialize, serialize, Decodable};
use crate::{slab::Slab, Result};

use rocksdb::{IteratorMode, Options, DB};

pub struct SlabStore {
    db: DB,
    opt: Options,
    path: Arc<Path>,
}

impl SlabStore {
    pub fn new(path: &Path) -> Result<Self> {

        let mut opt = Options::default();
        opt.create_if_missing(true);

        let db = DB::open(&opt, path)?;

        let path = Arc::from(path);

        Ok(SlabStore { db, opt, path })
    }

    pub fn get(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>> {
        let value = self.db.get(key)?;
        Ok(value)
    }

    pub fn put(&self, value: Vec<u8>) -> Result<()> {
        let slab: Slab = deserialize(&value)?;
        let last_index = self.get_last_index()?;
        let key = last_index + 1;
        if slab.get_index() == key {
            let key = serialize(&key);
            self.db.put(key, value)?;
        } 
        Ok(())
    }

    pub fn get_value_deserialized<T: Decodable>(&self, key: Vec<u8>) -> Result<Option<T>> {
        let value = self.db.get(key)?;
        match value {
            Some(v) => {
                let v = deserialize(&v)?;
                Ok(Some(v))
            }
            None => Ok(None),
        }
    }

    pub fn get_last_index(&self) -> Result<u64> {
        let last_index = self.db.iterator(IteratorMode::End).next();
        match last_index {
            Some((index, _)) => Ok(deserialize(&index)?),
            None => Ok(0),
        }
    }

    pub fn get_last_index_as_bytes(&self) -> Result<Vec<u8>> {
        let last_index = self.db.iterator(IteratorMode::End).next();
        match last_index {
            Some((index, _)) => Ok(index.to_vec()),
            None => Ok(serialize::<u64>(&0)),
        }
    }

    pub fn destroy(&self) -> Result<()> {
        DB::destroy(&self.opt, self.path.clone())?;
        Ok(())
    }
}
