use crate::{
    util::serial::{Decodable, Encodable},
    Result,
};

#[derive(Clone, Debug)]
pub struct Slab {
    index: u64,
    payload: Vec<u8>,
}

impl Slab {
    pub fn new(payload: Vec<u8>) -> Self {
        let index = 0;
        Slab { index, payload }
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
        len += self.index.encode(&mut s)?;
        len += self.payload.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for Slab {
    fn decode<D: std::io::Read>(mut d: D) -> Result<Self> {
        Ok(Self { index: Decodable::decode(&mut d)?, payload: Decodable::decode(&mut d)? })
    }
}
