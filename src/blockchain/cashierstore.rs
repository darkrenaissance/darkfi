use std::sync::Arc;

use crate::serial::{deserialize, serialize};
use crate::Result;

use super::rocks::{columns, IteratorMode, RocksColumn};
use super::cashier_keypair::CashierKeypair;

pub struct CashierStore {
    rocks: RocksColumn<columns::CashierKeys>,
}

impl CashierStore {
    pub fn new(rocks: RocksColumn<columns::CashierKeys>) -> Result<Arc<Self>> {
        Ok(Arc::new(CashierStore { rocks }))
    }

    pub fn get(&self, key: jubjub::SubgroupPoint) -> Result<Option<Vec<u8>>> {
        let value = self.rocks.get(key)?;
        Ok(value)
    }

    pub fn put(&self, keypair: CashierKeypair) -> Result<Option<jubjub::SubgroupPoint>> {

        let index = keypair.get_index();

        match self.get(index) {
            Ok(_v) => Ok(None),
            Err(_e) => {
                self.rocks.put(index.clone(), keypair)?;
                Ok(Some(index))
            },
        }
    }

    pub fn get_value_deserialized(&self, key: Vec<u8>) -> Result<Option<CashierKeypair>> {
        self.rocks.get_value_deserialized::<CashierKeypair>(key)
    }
    // Fix this
    // pub fn get_last_index(&self) -> Result<jubjub::SubgroupPoint> {
    //     let last_index = self.rocks.iterator(IteratorMode::End)?.next();
    //     match last_index {
    //         Some((index, _)) => Ok(deserialize(&index)?),
    //         None => Ok()
    //     }
    // }

    pub fn get_last_index_as_bytes(&self) -> Result<Vec<u8>> {
        let last_index = self.rocks.iterator(IteratorMode::End)?.next();
        match last_index {
            Some((index, _)) => Ok(index.to_vec()),
            None => Ok(serialize::<u64>(&0)),
        }
    }
}
