use std::io;

use pasta_curves::{group::ff::PrimeField, pallas};

use crate::{
    util::serial::{Decodable, Encodable, ReadExt, WriteExt},
    Result,
};

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Coin(pub pallas::Base);

impl Coin {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        pallas::Base::from_repr(bytes).map(Coin).unwrap()
    }

    pub fn to_bytes(self) -> [u8; 32] {
        self.0.to_repr()
    }
}

impl Encodable for Coin {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        s.write_slice(&self.to_bytes()[..])?;
        Ok(32)
    }
}

impl Decodable for Coin {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        Ok(Self::from_bytes(bytes))
    }
}
