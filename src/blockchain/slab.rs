use crate::serial::{Decodable, Encodable};
use crate::Result;

pub struct Slab {
    asset_type: String,
    index: u64,
    payload: Vec<u8>,
}

impl Slab {
    pub fn new(asset_type: String, payload: Vec<u8>) -> Self {
        let index = 0;
        Slab {
            asset_type,
            index,
            payload,
        }
    }

    pub fn set_index(&mut self, index: u64) {
        self.index = index;
    }

    pub fn get_index(&self) -> u64 {
        self.index
    }

    pub fn get_payload(&self) -> Vec<u8> {
        self.payload.clone()
    }
}

impl Encodable for Slab {
    fn encode<S: std::io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.asset_type.encode(&mut s)?;
        len += self.index.encode(&mut s)?;
        len += self.payload.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for Slab {
    fn decode<D: std::io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            asset_type: Decodable::decode(&mut d)?,
            index: Decodable::decode(&mut d)?,
            payload: Decodable::decode(&mut d)?,
        })
    }
}
