use std::io;

use crate::{
    serial::{Decodable, Encodable},
    Result,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Nullifier {
    pub repr: [u8; 32],
}

impl Nullifier {
    pub fn new(repr: [u8; 32]) -> Self {
        Self { repr }
    }
}

impl Encodable for Nullifier {
    fn encode<S: io::Write>(&self, s: S) -> Result<usize> {
        self.repr.encode(s)
    }
}

impl Decodable for Nullifier {
    fn decode<D: io::Read>(d: D) -> Result<Self> {
        Ok(Self {
            repr: Decodable::decode(d)?,
        })
    }
}
