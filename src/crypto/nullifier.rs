use std::io;

use crate::{
    error::Result,
    serial::{Decodable, Encodable},
};

pub struct Nullifier {
    pub repr: [u8; 32],
}

impl Nullifier {
    pub fn new(repr: [u8; 32]) -> Self {
        Self { repr }
    }
}

impl Encodable for Nullifier {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        Ok(self.repr.encode(s)?)
    }
}

impl Decodable for Nullifier {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            repr: Decodable::decode(d)?,
        })
    }
}
