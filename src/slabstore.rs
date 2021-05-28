use std::path::Path;
use std::sync::Arc;

use crate::serial::{deserialize, serialize, Decodable, Encodable};
use crate::Result;

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
        let key = self.increase_index()?;
        self.db.put(key, value)?;
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

    pub fn set_value<T: Encodable>(&self, value: T) -> Result<()> {
        let key = self.increase_index()?;
        let value = serialize(&value);
        self.db.put(key, value)?;
        Ok(())
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

    fn increase_index(&self) -> Result<Vec<u8>> {
        let mut key = self.get_last_index()?;
        key += 1;
        let key = serialize(&key);
        Ok(key)
    }

    pub fn destroy(&self) -> Result<()> {
        DB::destroy(&self.opt, self.path.clone())?;
        Ok(())
    }
}
