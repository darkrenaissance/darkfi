use std::io::{Error, Read, Write};

use x25519_dalek::PublicKey as X25519PublicKey;

use crate::{Decodable, Encodable, ReadExt, WriteExt};

impl Encodable for X25519PublicKey {
    #[inline]
    fn encode<S: Write>(&self, mut s: S) -> Result<usize, Error> {
        s.write_slice(self.as_bytes())?;
        Ok(32)
    }
}

impl Decodable for X25519PublicKey {
    #[inline]
    fn decode<D: Read>(mut d: D) -> Result<Self, Error> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        Ok(Self::from(bytes))
    }
}
