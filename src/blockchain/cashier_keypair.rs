use crate::serial::{Decodable, Encodable};
use crate::Result;

#[derive(Clone, Debug)]
pub struct CashierKeypair {
    zk_public: jubjub::SubgroupPoint,
    payload: Vec<u8>,
}

impl CashierKeypair {
    pub fn new(zk_public: jubjub::SubgroupPoint, payload: Vec<u8>) -> Self {
        CashierKeypair { zk_public, payload }
    }

    pub fn set_index(&mut self, index: jubjub::SubgroupPoint) {
        self.zk_public = index;
    }

    pub fn get_index(&self) -> jubjub::SubgroupPoint {
        self.zk_public
    }

    pub fn get_payload(&self) -> Vec<u8> {
        self.payload.clone()
    }
}

impl Encodable for CashierKeypair {
    fn encode<S: std::io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.zk_public.encode(&mut s)?;
        len += self.payload.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for CashierKeypair {
    fn decode<D: std::io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            zk_public: Decodable::decode(&mut d)?,
            payload: Decodable::decode(&mut d)?,
        })
    }
}
