use group::GroupEncoding;
use std::io;

use crate::crypto::{merkle::IncrementalWitness, merkle_node::MerkleNode};
use crate::error::{Error, Result};
use crate::serial::{Decodable, Encodable, ReadExt, WriteExt};

impl Encodable for jubjub::Fr {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        s.write_slice(&self.to_bytes()[..])?;
        Ok(32)
    }
}

impl Decodable for jubjub::Fr {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        let result = Self::from_bytes(&bytes);
        if result.is_some().into() {
            Ok(result.unwrap())
        } else {
            Err(Error::BadOperationType)
        }
    }
}

impl Encodable for jubjub::SubgroupPoint {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        s.write_slice(&self.to_bytes()[..])?;
        Ok(32)
    }
}

impl Decodable for jubjub::SubgroupPoint {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        let result = Self::from_bytes(&bytes);
        if result.is_some().into() {
            Ok(result.unwrap())
        } else {
            Err(Error::BadOperationType)
        }
    }
}
