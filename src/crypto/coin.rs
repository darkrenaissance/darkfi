use std::io;

use crate::{
    error::Result,
    serial::{Decodable, Encodable},
};

pub struct Coin {
    pub repr: [u8; 32],
}

impl Coin {
    pub fn new(repr: [u8; 32]) -> Self {
        Self { repr }
    }
}

impl Encodable for Coin {
    fn encode<S: io::Write>(&self, s: S) -> Result<usize> {
        Ok(self.repr.encode(s)?)
    }
}

impl Decodable for Coin {
    fn decode<D: io::Read>(d: D) -> Result<Self> {
        Ok(Self {
            repr: Decodable::decode(d)?,
        })
    }
}
