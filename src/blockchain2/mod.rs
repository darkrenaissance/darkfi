use std::io;

use crate::{
    impl_vec,
    util::serial::{Decodable, Encodable, ReadExt, VarInt, WriteExt},
    Result,
};

pub mod blockstore;
pub mod nfstore;
pub mod rootstore;
pub mod txstore;

impl Encodable for blake3::Hash {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        s.write_slice(self.as_bytes())?;
        Ok(32)
    }
}

impl Decodable for blake3::Hash {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        Ok(bytes.into())
    }
}

impl_vec!(blake3::Hash);
