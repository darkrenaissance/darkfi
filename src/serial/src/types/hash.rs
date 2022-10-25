use std::io::{Error, Read, Write};

use crate::{Decodable, Encodable, ReadExt, WriteExt};

#[cfg(feature = "blake3")]
impl Encodable for blake3::Hash {
    fn encode<S: Write>(&self, mut s: S) -> Result<usize, Error> {
        s.write_slice(self.as_bytes())?;
        Ok(32)
    }
}

#[cfg(feature = "blake3")]
impl Decodable for blake3::Hash {
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        Ok(bytes.into())
    }
}
