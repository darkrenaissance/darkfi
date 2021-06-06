use std::sync::Arc;

use crate::serial::{deserialize, serialize};
use crate::Result;

use super::slab::Slab;
use super::rocks::{columns, IteratorMode, Rocks};

pub struct SlabStore {
    rocks: Rocks,
}

impl SlabStore {
    pub fn new(rocks: Rocks) -> Result<Arc<Self>> {
        Ok(Arc::new(SlabStore { rocks }))
    }

    pub fn get(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>> {
        let cf = self.rocks.cf_handle::<columns::Slabs>()?;
        let value = self.rocks.get_cf(cf, key)?;
        Ok(value)
    }

    pub fn put(&self, value: Vec<u8>) -> Result<Option<Vec<u8>>> {
        let slab: Slab = deserialize(&value)?;
        let last_index = self.get_last_index()?;
        let key = last_index + 1;

        if slab.get_index() == key {
            let key = serialize(&key);

            let cf = self.rocks.cf_handle::<columns::Slabs>()?;
            self.rocks.put_cf(cf, key.clone(), value)?;

            Ok(Some(key))
        } else {
            Ok(None)
        }
    }

    pub fn get_value_deserialized(&self, key: Vec<u8>) -> Result<Option<Slab>> {
        let value = self.get(key)?;
        match value {
            Some(v) => {
                let v: Slab = deserialize(&v)?;
                Ok(Some(v))
            }
            None => Ok(None),
        }
    }

    pub fn get_last_index(&self) -> Result<u64> {
        let cf = self.rocks.cf_handle::<columns::Slabs>()?;
        let last_index = self.rocks.iterator(cf, IteratorMode::End).next();
        match last_index {
            Some((index, _)) => Ok(deserialize(&index)?),
            None => Ok(0),
        }
    }

    pub fn get_last_index_as_bytes(&self) -> Result<Vec<u8>> {
        let cf = self.rocks.cf_handle::<columns::Slabs>()?;
        let last_index = self.rocks.iterator(cf, IteratorMode::End).next();
        match last_index {
            Some((index, _)) => Ok(index.to_vec()),
            None => Ok(serialize::<u64>(&0)),
        }
    }
}
