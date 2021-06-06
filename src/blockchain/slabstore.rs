use std::sync::Arc;

use crate::serial::{deserialize, serialize};
use crate::Result;

use super::rocks::{columns, IteratorMode, RocksColumn};
use super::slab::Slab;

pub struct SlabStore {
    rocks: RocksColumn<columns::Slabs>,
}

impl SlabStore {
    pub fn new(rocks: RocksColumn<columns::Slabs>) -> Result<Arc<Self>> {
        Ok(Arc::new(SlabStore { rocks }))
    }

    pub fn get(&self, key: Vec<u8>) -> Result<Option<Vec<u8>>> {
        let value = self.rocks.get(key)?;
        Ok(value)
    }

    pub fn put(&self, value: Vec<u8>) -> Result<Option<u64>> {
        let slab: Slab = deserialize(&value)?;
        let last_index = self.get_last_index()?;
        let key = last_index + 1;

        if slab.get_index() == key {
            self.rocks.put(key.clone(), value)?;
            Ok(Some(key))
        } else {
            Ok(None)
        }
    }

    pub fn get_value_deserialized(&self, key: Vec<u8>) -> Result<Option<Slab>> {
        self.rocks.get_value_deserialized::<Slab>(key)
    }

    pub fn get_last_index(&self) -> Result<u64> {
        let last_index = self.rocks.iterator(IteratorMode::End)?.next();
        match last_index {
            Some((index, _)) => Ok(deserialize(&index)?),
            None => Ok(0),
        }
    }

    pub fn get_last_index_as_bytes(&self) -> Result<Vec<u8>> {
        let last_index = self.rocks.iterator(IteratorMode::End)?.next();
        match last_index {
            Some((index, _)) => Ok(index.to_vec()),
            None => Ok(serialize::<u64>(&0)),
        }
    }
}
