use std::sync::Arc;

use log::debug;

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
        debug!(target: "SLABSTORE", "get value");
        let value = self.rocks.get(key)?;
        Ok(value)
    }

    pub fn put(&self, slab: Slab) -> Result<Option<u64>> {
        debug!(target: "SLABSTORE", "Put slab");
        let last_index = self.get_last_index()?;
        let key = last_index + 1;

        if slab.get_index() == key {
            self.rocks.put(key, slab)?;
            Ok(Some(key))
        } else {
            Ok(None)
        }
    }

    pub fn get_value_deserialized(&self, key: Vec<u8>) -> Result<Option<Slab>> {
        self.rocks.get_value_deserialized::<Slab>(key)
    }

    pub fn get_last_index(&self) -> Result<u64> {
        debug!(target: "SLABSTORE", "Get last index");
        let last_index = self.rocks.iterator(IteratorMode::End)?.next();
        match last_index {
            Some((index, _)) => Ok(deserialize(&index)?),
            None => Ok(0),
        }
    }

    pub fn get_last_index_as_bytes(&self) -> Result<Vec<u8>> {
        debug!(target: "SLABSTORE", "Get last index as bytes");
        let last_index = self.rocks.iterator(IteratorMode::End)?.next();
        match last_index {
            Some((index, _)) => Ok(index.to_vec()),
            None => Ok(serialize::<u64>(&0)),
        }
    }
}
